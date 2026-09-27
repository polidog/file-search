mod block;
mod files;
mod score;
mod skill;

use anyhow::{Result, bail};
use block::SourceFile;
use clap::Parser;
use jev::cli::ProviderKind;
use skill::Agent;
use std::io::{BufRead, IsTerminal};
use std::path::PathBuf;
use std::process::ExitCode;

/// ソースコードを関数くらいの塊に切り、探したい内容に当てはまる確率を Jev に聞いて、高い順に並べる
///
/// パスを省略して標準入力にパスを流すと、それを使う (例: rg -l catch | file-search "エラーを握りつぶしている箇所")。
/// 見つかれば 0、閾値を超えるものが無ければ 1 で終わる (grep と同じ)
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// 探したい内容 (自然文でよい)
    #[arg(required_unless_present = "install_skill")]
    query: Option<String>,
    /// 探すファイルかディレクトリ。複数可 (省略時、標準入力がパイプならそこからパスを読む。でなければ .)
    paths: Vec<PathBuf>,
    /// 表示件数
    #[arg(short, long, default_value_t = 10)]
    num: usize,
    /// これ未満の確率は出さない (0〜1)
    #[arg(short, long, default_value_t = 0.5)]
    threshold: f64,
    /// 採点する塊の数の上限 (API 呼び出し量の歯止め)
    #[arg(short, long, default_value_t = 1000)]
    max: usize,
    /// 送らずに、リクエスト数・トークン数・料金の目安だけ出す
    #[arg(long)]
    dry_run: bool,
    #[arg(short, long, env = "JEV_PROVIDER", value_enum, default_value_t = ProviderKind::Typesafe)]
    provider: ProviderKind,
    /// このコマンドを使うスキルを ~/.claude/skills か ~/.codex/skills に書き出して終わる
    #[arg(long, value_enum, value_name = "AGENT", conflicts_with_all = ["query", "paths"])]
    install_skill: Option<Agent>,
}

fn main() -> Result<ExitCode> {
    let cli = Cli::parse();
    if let Some(agent) = cli.install_skill {
        println!("{}", agent.install()?.display());
        return Ok(ExitCode::SUCCESS);
    }
    let query = cli.query.expect("clap が query を必須にしている");
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

    let files = read_files(&files::expand(&roots)?, cli.max);
    if files.is_empty() {
        bail!("テキストファイルが見つかりません");
    }
    let jobs = score::plan(&files);
    if cli.dry_run {
        let (requests, tokens, usd) = score::estimate(&query, &jobs);
        let blocks: usize = files.iter().map(|f| f.blocks.len()).sum();
        println!("{} ファイル / {blocks} 塊 / {requests} リクエスト / 入力 約 {tokens} トークン / 約 ${usd:.4}", files.len());
        return Ok(ExitCode::SUCCESS);
    }

    let mut hits = score::run(cli.provider, &query, &jobs)?;
    hits.retain(|h| h.score >= cli.threshold);
    hits.sort_unstable_by(|a, b| b.score.total_cmp(&a.score));
    for h in hits.iter().take(cli.num) {
        println!("{:.2}  {}:{}  {}", h.score, h.file.path.display(), h.block.start, h.file.head(h.block));
    }
    Ok(if hits.is_empty() { ExitCode::from(1) } else { ExitCode::SUCCESS })
}

/// 塊の合計が max に届くまでファイルを読む。届いたら最後のファイルの塊を切り詰めて止める
fn read_files(paths: &[PathBuf], max: usize) -> Vec<SourceFile> {
    let mut files = Vec::new();
    let mut total = 0;
    for mut f in paths.iter().filter_map(|p| SourceFile::read(p)) {
        if total + f.blocks.len() > max {
            f.blocks.truncate(max - total);
            if !f.blocks.is_empty() {
                files.push(f);
            }
            eprintln!("{max} 塊で打ち切りました (--max で増やせます)");
            break;
        }
        total += f.blocks.len();
        files.push(f);
    }
    files
}
