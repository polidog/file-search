use anyhow::{Context, Result, bail};
use clap::Parser;
use jev::cli::ProviderKind;
use jev::model::{Answer, Question, Request};
use jev::provider;
use serde_json::json;
use std::io::{BufRead, IsTerminal};
use std::path::{Path, PathBuf};
use std::thread;

const LEVELS: [&str; 5] = ["irrelevant", "weak", "fair", "good", "excellent"];
/// Jev に渡す 1 塊あたりの長さ (文字数)
const BLOCK_CHARS: usize = 1000;
/// これより短い塊は次の塊とつなげる (行数)
const MIN_LINES: usize = 3;
/// これより長い塊は次の空行で切る (行数)
const MAX_LINES: usize = 40;
/// これより大きいファイルは生成物とみなして読まない
const MAX_FILE_BYTES: u64 = 1 << 20;
/// 1 リクエストにまとめる塊の数
const CHUNK: usize = 20;
/// 同時に投げるリクエスト数
const PARALLEL: usize = 8;
const SKIP_DIRS: [&str; 3] = ["target", "node_modules", "vendor"];

/// ソースコードを関数くらいの塊に切って Jev で採点し、探したい内容と意味が近い順に並べる
///
/// パスを省略して標準入力にパスを流すと、それを使う (例: rg -l retry | file-search "リトライしている処理")
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// 探したい内容 (自然文でよい)
    query: String,
    /// 探すファイルかディレクトリ。複数可 (省略時、標準入力がパイプならそこからパスを読む。でなければ .)
    paths: Vec<PathBuf>,
    /// 表示件数
    #[arg(short, long, default_value_t = 10)]
    num: usize,
    /// 採点する塊の数の上限 (API 呼び出し量の歯止め)
    #[arg(short, long, default_value_t = 200)]
    max: usize,
    #[arg(short, long, env = "JEV_PROVIDER", value_enum, default_value_t = ProviderKind::Typesafe)]
    provider: ProviderKind,
}

struct Block {
    path: PathBuf,
    /// 1 始まりの行番号
    line: usize,
    text: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let roots = if !cli.paths.is_empty() {
        cli.paths
    } else if !std::io::stdin().is_terminal() {
        let paths: Vec<PathBuf> = std::io::stdin().lock().lines().map_while(Result::ok).map(PathBuf::from).collect();
        if paths.is_empty() {
            bail!("標準入力にパスがありません");
        }
        paths
    } else {
        vec![PathBuf::from(".")]
    };
    let paths = expand(&roots)?;

    let mut rest = paths.iter().flat_map(|p| read_blocks(p));
    let blocks: Vec<Block> = rest.by_ref().take(cli.max).collect();
    if blocks.is_empty() {
        bail!("テキストファイルが見つかりません");
    }
    if rest.next().is_some() {
        eprintln!("{} 塊で打ち切りました (--max で増やせます)", cli.max);
    }

    let scores = score_all(cli.provider, &cli.query, &blocks)?;
    let mut ranked: Vec<_> = scores.into_iter().zip(&blocks).collect();
    ranked.sort_unstable_by(|a, b| b.0.total_cmp(&a.0));
    for (score, b) in ranked.into_iter().take(cli.num) {
        let head = b.text.lines().next().unwrap_or_default().trim();
        println!("{} {score:.1}  {}:{}  {head}", stars(score), b.path.display(), b.line);
    }
    Ok(())
}

/// ディレクトリは中のファイルに展開し、ファイルはそのまま使う
fn expand(roots: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for root in roots {
        let meta = std::fs::metadata(root).with_context(|| format!("{} を読めません", root.display()))?;
        if meta.is_dir() {
            walk(root, &mut paths).with_context(|| format!("{} を読めません", root.display()))?;
        } else {
            paths.push(root.clone());
        }
    }
    Ok(paths)
}

/// 隠しファイルとビルド成果物を飛ばしてファイルのパスを集める。読めないサブディレクトリは警告して飛ばす
// ponytail: .gitignore は見ない。要るなら rg -l / git ls-files をパイプで渡す
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

/// ファイルを読んで塊に切る。大きすぎる・UTF-8 でない・NUL を含むファイルは空
fn read_blocks(path: &Path) -> Vec<Block> {
    let too_big = std::fs::metadata(path).map_or(true, |m| m.len() > MAX_FILE_BYTES);
    let text = match std::fs::read_to_string(path) {
        Ok(t) if !too_big && !t.contains('\0') => t,
        _ => return Vec::new(),
    };
    split(&text).into_iter().map(|(line, text)| Block { path: path.to_owned(), line, text }).collect()
}

/// 空行のあとに行頭から始まる行で切る。MIN_LINES 未満の塊は次とつなげ、MAX_LINES を超えたら次の空行で切る
// ponytail: 字下げで判断するだけ。impl / class の中のメソッドは MAX_LINES でしか切れない。正確にやるなら tree-sitter
fn split(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    let mut start = 0;
    let mut prev_blank = true;
    for (i, line) in text.lines().enumerate() {
        let blank = line.trim().is_empty();
        let top = !blank && !line.starts_with(char::is_whitespace);
        let filled = cur.iter().filter(|l| !l.trim().is_empty()).count();
        if (prev_blank && top && filled >= MIN_LINES) || (blank && cur.len() >= MAX_LINES) {
            out.extend(finish(start, &cur));
            cur.clear();
        }
        if cur.is_empty() {
            start = i + 1;
        }
        if !(blank && cur.is_empty()) {
            cur.push(line);
        }
        prev_blank = blank;
    }
    out.extend(finish(start, &cur));
    out
}

fn finish(start: usize, lines: &[&str]) -> Option<(usize, String)> {
    let text = lines.join("\n");
    let text = text.trim_end();
    (!text.is_empty()).then(|| (start, text.chars().take(BLOCK_CHARS).collect()))
}

/// CHUNK 件ずつ 1 リクエストにし、PARALLEL 本ずつ同時に投げる。blocks と同じ順で点数を返す
fn score_all(kind: ProviderKind, query: &str, blocks: &[Block]) -> Result<Vec<f64>> {
    let mut scores = Vec::with_capacity(blocks.len());
    for batch in blocks.chunks(CHUNK * PARALLEL) {
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

fn score(kind: ProviderKind, query: &str, blocks: &[Block]) -> Result<Vec<f64>> {
    let res = provider::of(kind).evaluate(&request(query, blocks))?;
    Ok((0..blocks.len())
        .map(|i| match res.answers.get(&format!("r{i}")) {
            Some(Answer::Score { score, .. }) => *score,
            _ => 0.0,
        })
        .collect())
}

/// 塊を 1 つの state に入れ、塊ごとに score の質問を 1 つ立てる (1 リクエストで並列評価)
fn request(query: &str, blocks: &[Block]) -> Request {
    let mut state = format!("Looking for: {query}\n");
    for (i, b) in blocks.iter().enumerate() {
        state += &format!("\n[r{i}] {}:{}\n{}\n", b.path.display(), b.line, b.text);
    }
    let questions = (0..blocks.len())
        .map(|i| {
            let instructions = json!(format!(
                "How well does code block [r{i}] match what the user is looking for, in meaning (not just keywords)?"
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
        let blocks = [Block { path: "a.rs".into(), line: 3, text: "s".into() }];
        let r = request("q", &blocks);
        assert_eq!(r.state, json!("Looking for: q\n\n[r0] a.rs:3\ns\n"));
        assert!(r.questions.contains_key("r0"));
    }

    #[test]
    fn split_by_top_level() {
        let src = "use a;\nuse b;\n\nconst X: u8 = 1;\n\n/// doc\nfn f() {\n    1;\n\n    2;\n}\n\n\nfn g() {\n}\n";
        let got = split(src);
        let starts: Vec<_> = got.iter().map(|(l, t)| (*l, t.lines().next().unwrap())).collect();
        // use と const は短いのでつながる。関数の中の空行では切らない
        assert_eq!(starts, [(1, "use a;"), (6, "/// doc"), (14, "fn g() {")]);
        assert!(got[1].1.ends_with("    2;\n}"));
    }

    #[test]
    fn split_long_block_at_blank() {
        let body = "    x;\n".repeat(MAX_LINES) + "\n    y;\n";
        let got = split(&format!("impl A {{\n{body}}}\n"));
        assert_eq!(got.len(), 2);
        assert_eq!(got[1].0, MAX_LINES + 3);
    }

    #[test]
    fn expand_and_read() {
        let d = std::env::temp_dir().join(format!("file-search-{}", std::process::id()));
        std::fs::create_dir_all(d.join(".git")).unwrap();
        std::fs::create_dir_all(d.join("target")).unwrap();
        std::fs::write(d.join("a.md"), "hello").unwrap();
        std::fs::write(d.join(".git/x"), "no").unwrap();
        std::fs::write(d.join("target/y"), "no").unwrap();
        std::fs::write(d.join("bin"), [0xff, 0xfe, 0x00]).unwrap();
        std::fs::write(d.join("nul"), "a\0b").unwrap();
        let paths = expand(&[d.clone(), d.join("target/y")]).unwrap();
        assert!(expand(&[d.join("missing")]).is_err());
        let blocks: Vec<_> = paths.iter().flat_map(|p| read_blocks(p)).collect();
        std::fs::remove_dir_all(&d).unwrap();
        // ディレクトリからは隠し・target を飛ばすが、名指ししたファイルは読む
        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].path.ends_with("a.md"));
        assert!(blocks[1].path.ends_with("target/y"));
        assert_eq!((blocks[0].line, blocks[0].text.as_str()), (1, "hello"));
    }
}
