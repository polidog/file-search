//! 採点。jev-lint (https://github.com/mizchi/jev-lint) で測られた知見に沿っている:
//! - 順序のある 5 段階ではなく、「当てはまるか」を noul (確率) で聞く。criteria は必ず {true, false} の入れ子にする
//! - state にはファイル全体を入れ、塊は行番号で指す。周りのコードが見えるほうが判定が安定し、
//!   ファイルを 1 回送るだけで済むので安い。別のファイルの塊は同じ state に混ぜない
//! - state には 32,768 トークン、リクエスト全体には 64Ki トークンの上限がある。問いの数に上限は無いが、
//!   塊が多いと問いだけで全体の上限を超えるので、`max_tokens_exceeded` が返ったら塊を半分に割って投げ直す。429 は待って投げ直す
use crate::block::{Block, SourceFile};
use anyhow::{Result, anyhow};
use jev::cli::ProviderKind;
use jev::model::{Answer, NoulCriteria, Question, Request, Response};
use jev::provider;
use serde_json::json;
use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};
use std::thread;
use std::time::Duration;

/// 1 つの state に入れるソースの上限 (バイト)。日本語混じりでも 1 トークン ≒ 3 バイトより大きいので、
/// 32,768 トークンの state 上限に余裕をもって収まる
const STATE_BYTES: usize = 64_000;
/// 同時に投げるリクエスト数
const PARALLEL: usize = 16;
/// 429 / 5xx を投げ直す回数
const RETRIES: u32 = 5;
/// 入力 100 万トークンあたりの料金 (USD)。jev-lint が jev-1.13.0 で実測した $0.0906 / 2.16M トークンから
// ponytail: 定数。料金が変わったら更新する
const USD_PER_MTOK: f64 = 0.042;
/// サーバーがリクエストごとに足すトークン数 (jev-lint の実測)
const TOKENS_PER_REQUEST: usize = 270;

const TASK: &str = "The user is searching a codebase. Judge only the code at `lines` of `source`, and only whether it is what the user is looking for -- the rest of the file is there for context.";

/// 1 リクエストぶん: 1 つのファイルの、続いた塊の並び
pub struct Job<'a> {
    file: &'a SourceFile,
    blocks: &'a [Block],
}

pub struct Hit<'a> {
    pub score: f64,
    pub file: &'a SourceFile,
    pub block: &'a Block,
}

/// ファイルごとに、ソースが STATE_BYTES に収まる範囲で塊をまとめる
pub fn plan(files: &[SourceFile]) -> Vec<Job<'_>> {
    let mut jobs = Vec::new();
    for file in files {
        let (mut from, mut bytes) = (0, 0);
        for (i, b) in file.blocks.iter().enumerate() {
            let size: usize = file.lines[b.start - 1..b.end].iter().map(|l| l.len() + 8).sum();
            if i > from && bytes + size > STATE_BYTES {
                jobs.push(Job { file, blocks: &file.blocks[from..i] });
                (from, bytes) = (i, 0);
            }
            bytes += size;
        }
        jobs.push(Job { file, blocks: &file.blocks[from..] });
    }
    jobs
}

/// 送らずに見積もる: (リクエスト数, 入力トークン数, 料金 USD)
pub fn estimate(query: &str, jobs: &[Job]) -> (usize, usize, f64) {
    let tokens: usize =
        jobs.iter().map(|j| serde_json::to_string(&request(query, j)).map_or(0, |s| s.len()) / 3 + TOKENS_PER_REQUEST).sum();
    (jobs.len(), tokens, tokens as f64 / 1e6 * USD_PER_MTOK)
}

/// 全ジョブを PARALLEL 本ずつ投げ、塊ごとの確率を返す。1 つでも失敗したら残りは投げずにエラー
pub fn run<'a>(kind: ProviderKind, query: &str, jobs: &[Job<'a>]) -> Result<Vec<Hit<'a>>> {
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let failed = AtomicBool::new(false);
    let progress = std::io::stderr().is_terminal();
    let results: Vec<Result<Vec<Hit>>> = thread::scope(|s| {
        let workers: Vec<_> = (0..PARALLEL.min(jobs.len()))
            .map(|_| {
                s.spawn(|| {
                    let mut out = Vec::new();
                    while !failed.load(Relaxed) {
                        let Some(job) = jobs.get(next.fetch_add(1, Relaxed)) else { break };
                        let r = ask(&|req| provider::of(kind).evaluate(req), query, job);
                        failed.fetch_or(r.is_err(), Relaxed);
                        out.push(r);
                        let n = done.fetch_add(1, Relaxed) + 1;
                        if progress {
                            eprint!("\r採点中 {n}/{}", jobs.len());
                        }
                    }
                    out
                })
            })
            .collect();
        workers.into_iter().flat_map(|w| w.join().expect("採点スレッドが panic しました")).collect()
    });
    if progress {
        eprint!("\r\x1b[K");
    }
    results.into_iter().flat_map(|r| r.map_or_else(|e| vec![Err(e)], |v| v.into_iter().map(Ok).collect())).collect()
}

/// 1 ジョブを投げる。リクエストが大きすぎると言われたら塊を半分に割って、それぞれ投げ直す
fn ask<'a>(send: &dyn Fn(&Request) -> Result<Response>, query: &str, job: &Job<'a>) -> Result<Vec<Hit<'a>>> {
    let req = request(query, job);
    let res = match with_retry(|| send(&req)) {
        Err(e) if job.blocks.len() > 1 && e.to_string().contains("max_tokens_exceeded") => {
            let (a, b) = job.blocks.split_at(job.blocks.len() / 2);
            let mut hits = ask(send, query, &Job { file: job.file, blocks: a })?;
            hits.extend(ask(send, query, &Job { file: job.file, blocks: b })?);
            return Ok(hits);
        }
        r => r?,
    };
    job.blocks
        .iter()
        .enumerate()
        .map(|(i, block)| match res.answers.get(&id(i)) {
            Some(Answer::Noul { noul }) => Ok(Hit { score: *noul, file: job.file, block }),
            other => Err(anyhow!("{}:{} の答えが想定外です: {other:?}", job.file.path.display(), block.start)),
        })
        .collect()
}

/// 429 と 5xx は待って投げ直す (1, 2, 4, 8, 16 秒)。jev は "HTTP 429: ..." の形でエラーを返す
fn with_retry<T>(mut f: impl FnMut() -> Result<T>) -> Result<T> {
    for attempt in 0.. {
        match f() {
            Err(e) if attempt < RETRIES && is_transient(&e) => thread::sleep(Duration::from_secs(1 << attempt)),
            r => return r,
        }
    }
    unreachable!()
}

fn is_transient(e: &anyhow::Error) -> bool {
    let s = e.to_string();
    s.starts_with("HTTP 429") || s.starts_with("HTTP 5")
}

fn id(i: usize) -> String {
    format!("q{i:04}")
}

fn request(query: &str, job: &Job) -> Request {
    let first = job.blocks[0].start;
    let last = job.blocks[job.blocks.len() - 1].end;
    let mut source: String =
        job.file.lines[first - 1..last].iter().zip(first..).map(|(l, n)| format!("{n:>5}| {l}\n")).collect();
    if source.len() > STATE_BYTES {
        // 1 行が巨大なファイル (minify 済みなど)。文字の途中で切らないように縮める
        let mut cut = STATE_BYTES;
        while !source.is_char_boundary(cut) {
            cut -= 1;
        }
        source.truncate(cut);
    }
    let criteria = NoulCriteria {
        yes: Some(json!("The code at these lines itself is an instance of what the user is looking for.")),
        no: Some(json!(
            "It is not. It may sit next to such code, mention related words, or be loosely related, but it is not what the user is looking for."
        )),
    };
    let questions = job
        .blocks
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let instructions = json!({"task": TASK, "looking_for": query, "lines": format!("{}-{}", b.start, b.end)});
            (id(i), Question::Noul { instructions, criteria: Some(criteria.clone()) })
        })
        .collect();
    Request { state: json!({"file": job.file.path.display().to_string(), "source": source}), questions }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(n: usize, width: usize) -> SourceFile {
        SourceFile {
            path: "a.rs".into(),
            lines: (0..n).map(|_| "x".repeat(width)).collect(),
            blocks: (0..n).map(|i| Block { start: i + 1, end: i + 1 }).collect(),
        }
    }

    #[test]
    fn plan_splits_big_files() {
        let small = file(3, 10);
        let jobs = plan(std::slice::from_ref(&small));
        assert_eq!(jobs.len(), 1);
        // 1 行 ~32KB が 3 行: 2 行で上限を超えるので 1 行ずつになる
        let big = file(3, STATE_BYTES / 2);
        assert_eq!(plan(std::slice::from_ref(&big)).len(), 3);
    }

    #[test]
    fn request_shape() {
        let f = file(3, 1);
        let jobs = plan(std::slice::from_ref(&f));
        let r = request("q", &jobs[0]);
        assert_eq!(r.state["file"], "a.rs");
        assert_eq!(r.state["source"], "    1| x\n    2| x\n    3| x\n");
        assert_eq!(r.questions.len(), 3);
        let Question::Noul { instructions, criteria } = &r.questions["q0001"] else { panic!("noul でない") };
        assert_eq!(instructions["lines"], "2-2");
        assert_eq!(instructions["looking_for"], "q");
        // criteria は {true, false} の入れ子で送る (平らだと黙って捨てられる)
        let wire = serde_json::to_value(Question::Noul { instructions: json!(""), criteria: criteria.clone() }).unwrap();
        assert!(wire["criteria"]["true"].is_string() && wire["criteria"]["false"].is_string());
    }

    #[test]
    fn split_when_too_big() {
        // 問いが 2 個を超えると断るサーバー: 5 塊は 3 回割られて全部に答えが付く
        let f = file(5, 1);
        let jobs = plan(std::slice::from_ref(&f));
        let send = |req: &Request| -> Result<Response> {
            if req.questions.len() > 2 {
                return Err(anyhow!("HTTP 400: {{\"detail\":{{\"error_type\":\"max_tokens_exceeded\"}}}}"));
            }
            let answers = req.questions.keys().map(|k| (k.clone(), Answer::Noul { noul: 0.5 })).collect();
            Ok(Response { model: None, answers, usage: None })
        };
        let hits = ask(&send, "q", &jobs[0]).unwrap();
        assert_eq!(hits.iter().map(|h| h.block.start).collect::<Vec<_>>(), [1, 2, 3, 4, 5]);
    }

    #[test]
    fn retry_only_transient() {
        let mut calls = 0;
        let r: Result<()> = with_retry(|| {
            calls += 1;
            Err(anyhow!("HTTP 401: nope"))
        });
        assert!(r.is_err());
        assert_eq!(calls, 1);
        let mut calls = 0;
        let r = with_retry(|| {
            calls += 1;
            if calls < 2 { Err(anyhow!("HTTP 429: slow down")) } else { Ok(7) }
        });
        assert_eq!(r.unwrap(), 7);
    }
}
