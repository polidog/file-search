use anyhow::{Context, Result, bail};
use clap::Parser;
use jev::cli::ProviderKind;
use jev::model::{Answer, Question, Request};
use jev::provider;
use serde_json::json;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::thread;

const LEVELS: [&str; 5] = ["irrelevant", "weak", "fair", "good", "excellent"];
/// Jev に渡す 1 ファイルあたりの抜粋の長さ (文字数)
const EXCERPT_CHARS: usize = 600;
/// 抜粋のために読むバイト数 (UTF-8 は 1 文字最大 4 バイト)
const EXCERPT_BYTES: u64 = EXCERPT_CHARS as u64 * 4;
/// 1 リクエストにまとめるファイル数
const CHUNK: usize = 20;
/// 同時に投げるリクエスト数
const PARALLEL: usize = 8;
const SKIP_DIRS: [&str; 3] = ["target", "node_modules", "vendor"];

/// ディレクトリ内のファイルを Jev で採点し、探したい内容と意味が近い順に並べる
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// 探したい内容 (自然文でよい)
    query: String,
    /// 探すディレクトリ
    #[arg(default_value = ".")]
    dir: PathBuf,
    /// 表示件数
    #[arg(short, long, default_value_t = 10)]
    num: usize,
    /// 採点するファイル数の上限 (API 呼び出し量の歯止め)
    #[arg(short, long, default_value_t = 200)]
    max_files: usize,
    #[arg(short, long, env = "JEV_PROVIDER", value_enum, default_value_t = ProviderKind::Typesafe)]
    provider: ProviderKind,
}

struct Doc {
    path: PathBuf,
    excerpt: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut paths = Vec::new();
    walk(&cli.dir, &mut paths).with_context(|| format!("{} を読めません", cli.dir.display()))?;

    let mut rest = paths.into_iter();
    let docs: Vec<Doc> = rest
        .by_ref()
        .filter_map(|path| Some(Doc { excerpt: excerpt(&path)?, path }))
        .take(cli.max_files)
        .collect();
    if docs.is_empty() {
        bail!("テキストファイルが見つかりません");
    }
    if rest.next().is_some() {
        eprintln!("{} ファイルで打ち切りました (--max-files で増やせます)", cli.max_files);
    }

    let scores = score_all(cli.provider, &cli.query, &docs)?;
    let mut ranked: Vec<_> = scores.into_iter().zip(&docs).collect();
    ranked.sort_unstable_by(|a, b| b.0.total_cmp(&a.0));
    for (score, doc) in ranked.into_iter().take(cli.num) {
        println!("{} {score:.1}  {}", stars(score), doc.path.display());
    }
    Ok(())
}

/// 隠しファイルとビルド成果物を飛ばしてファイルのパスを集める。読めないサブディレクトリは警告して飛ばす
// ponytail: .gitignore は見ない。要るなら ignore クレートへ
fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let mut entries = std::fs::read_dir(dir)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name();
        if name.to_string_lossy().starts_with('.') || SKIP_DIRS.iter().any(|d| name == *d) {
            continue;
        }
        let path = e.path();
        match e.file_type()? {
            t if t.is_dir() => {
                if let Err(err) = walk(&path, out) {
                    eprintln!("{}: {err}", path.display());
                }
            }
            t if t.is_file() => out.push(path),
            _ => {}
        }
    }
    Ok(())
}

/// 先頭だけ読んで抜粋を作る。UTF-8 でない・NUL を含む・空のファイルは None
fn excerpt(path: &Path) -> Option<String> {
    let mut buf = Vec::new();
    File::open(path).ok()?.take(EXCERPT_BYTES).read_to_end(&mut buf).ok()?;
    let text = match std::str::from_utf8(&buf) {
        Ok(s) => s,
        // 読んだ末尾で文字が切れているだけなら、そこまでを使う
        Err(e) if e.error_len().is_none() => std::str::from_utf8(&buf[..e.valid_up_to()]).ok()?,
        Err(_) => return None,
    };
    if text.contains('\0') || text.trim().is_empty() {
        return None;
    }
    Some(text.chars().take(EXCERPT_CHARS).collect())
}

/// CHUNK 件ずつ 1 リクエストにし、PARALLEL 本ずつ同時に投げる。docs と同じ順で点数を返す
fn score_all(kind: ProviderKind, query: &str, docs: &[Doc]) -> Result<Vec<f64>> {
    let mut scores = Vec::with_capacity(docs.len());
    for batch in docs.chunks(CHUNK * PARALLEL) {
        let results = thread::scope(|s| {
            let handles: Vec<_> = batch.chunks(CHUNK).map(|chunk| s.spawn(move || score(kind, query, chunk))).collect();
            handles.into_iter().map(|h| h.join().expect("採点スレッドが panic しました")).collect::<Vec<_>>()
        });
        for r in results {
            scores.extend(r?);
        }
    }
    Ok(scores)
}

fn score(kind: ProviderKind, query: &str, docs: &[Doc]) -> Result<Vec<f64>> {
    let res = provider::of(kind).evaluate(&request(query, docs))?;
    Ok((0..docs.len())
        .map(|i| match res.answers.get(&format!("r{i}")) {
            Some(Answer::Score { score, .. }) => *score,
            _ => 0.0,
        })
        .collect())
}

/// ファイル群を 1 つの state に入れ、ファイルごとに score の質問を 1 つ立てる (1 リクエストで並列評価)
fn request(query: &str, docs: &[Doc]) -> Request {
    let mut state = format!("Looking for: {query}\n");
    for (i, d) in docs.iter().enumerate() {
        state += &format!("\n[r{i}] {}\n{}\n", d.path.display(), d.excerpt);
    }
    let questions = (0..docs.len())
        .map(|i| {
            let instructions = json!(format!(
                "How well does the content of file [r{i}] match what the user is looking for, in meaning (not just keywords)?"
            ));
            (format!("r{i}"), Question::Score { instructions, criteria: LEVELS.map(|l| json!(l)).to_vec() })
        })
        .collect();
    Request { state: json!(state), questions }
}

/// score は 0 始まりのレベル番号。★ 1〜5 に丸める
fn stars(score: f64) -> String {
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
        let docs = [Doc { path: "a.txt".into(), excerpt: "s".into() }];
        let r = request("q", &docs);
        assert_eq!(r.state, json!("Looking for: q\n\n[r0] a.txt\ns\n"));
        assert!(r.questions.contains_key("r0"));
    }

    #[test]
    fn walk_and_excerpt() {
        let d = std::env::temp_dir().join(format!("file-search-{}", std::process::id()));
        std::fs::create_dir_all(d.join(".git")).unwrap();
        std::fs::create_dir_all(d.join("target")).unwrap();
        std::fs::write(d.join("a.md"), "hello").unwrap();
        std::fs::write(d.join(".git/x"), "no").unwrap();
        std::fs::write(d.join("target/y"), "no").unwrap();
        std::fs::write(d.join("bin"), [0xff, 0xfe, 0x00]).unwrap();
        std::fs::write(d.join("nul"), "a\0b").unwrap();
        // 読む範囲の末尾で「あ」(3 バイト) が切れる
        std::fs::write(d.join("long"), "x".repeat(EXCERPT_BYTES as usize - 1) + "あ").unwrap();
        let mut paths = Vec::new();
        walk(&d, &mut paths).unwrap();
        let docs: Vec<_> = paths.iter().filter_map(|p| Some((p.file_name()?, excerpt(p)?))).collect();
        std::fs::remove_dir_all(&d).unwrap();
        assert_eq!(docs.len(), 2);
        assert_eq!(docs[0].0, "a.md");
        assert_eq!(docs[1].0, "long");
        assert_eq!(docs[1].1.chars().count(), EXCERPT_CHARS);
    }
}
