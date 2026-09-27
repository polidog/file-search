use crate::block::Block;
use anyhow::Result;
use jev::cli::ProviderKind;
use jev::model::{Answer, Question, Request};
use jev::provider;
use serde_json::json;
use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};
use std::thread;

const LEVELS: [&str; 5] = ["irrelevant", "weak", "fair", "good", "excellent"];
/// 同時に投げるリクエスト数
// ponytail: 固定値。429 が返るようならオプションにするかバックオフを入れる
const PARALLEL: usize = 16;

/// 塊を 1 つずつ別リクエストで採点する (一緒に送った塊に点数が引きずられないように)。blocks と同じ順で点数を返す
pub fn score_all(kind: ProviderKind, query: &str, blocks: &[Block]) -> Result<Vec<f64>> {
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let failed = AtomicBool::new(false);
    let progress = std::io::stderr().is_terminal();
    let results: Vec<(usize, Result<f64>)> = thread::scope(|s| {
        let workers: Vec<_> = (0..PARALLEL.min(blocks.len()))
            .map(|_| {
                s.spawn(|| {
                    let mut out = Vec::new();
                    while !failed.load(Relaxed) {
                        let i = next.fetch_add(1, Relaxed);
                        let Some(b) = blocks.get(i) else { break };
                        let r = score(kind, query, b);
                        failed.fetch_or(r.is_err(), Relaxed);
                        out.push((i, r));
                        let n = done.fetch_add(1, Relaxed) + 1;
                        if progress {
                            eprint!("\r採点中 {n}/{}", blocks.len());
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
    let mut scores = vec![0.0; blocks.len()];
    for (i, r) in results {
        scores[i] = r?;
    }
    Ok(scores)
}

fn score(kind: ProviderKind, query: &str, block: &Block) -> Result<f64> {
    let res = provider::of(kind).evaluate(&request(query, block))?;
    Ok(match res.answers.get("match") {
        Some(Answer::Score { score, .. }) => *score,
        _ => 0.0,
    })
}

fn request(query: &str, b: &Block) -> Request {
    let state = format!("Looking for: {query}\n\n{}:{}\n{}\n", b.path.display(), b.line, b.text);
    let instructions = json!("How well does this code block match what the user is looking for, in meaning (not just keywords)?");
    let q = Question::Score { instructions, criteria: LEVELS.map(|l| json!(l)).to_vec() };
    Request { state: json!(state), questions: [("match".to_string(), q)].into_iter().collect() }
}

/// score は 0 始まりのレベル番号。★ 1〜5 に丸める
pub fn stars(score: f64) -> String {
    let n = (score.round() as usize + 1).clamp(1, LEVELS.len());
    "★".repeat(n) + &"☆".repeat(LEVELS.len() - n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stars_and_request() {
        assert_eq!(stars(0.0), "★☆☆☆☆");
        assert_eq!(stars(3.6), "★★★★★");
        let r = request("q", &Block { path: "a.rs".into(), line: 3, text: "s".into() });
        assert_eq!(r.state, json!("Looking for: q\n\na.rs:3\ns\n"));
        assert_eq!(r.questions.len(), 1);
        assert!(r.questions.contains_key("match"));
    }
}
