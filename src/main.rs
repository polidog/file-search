mod block;
mod files;
mod score;
mod skill;

use anyhow::{Result, bail};
use block::Block;
use clap::Parser;
use jev::cli::ProviderKind;
use skill::Agent;
use std::io::{BufRead, IsTerminal};
use std::path::PathBuf;

/// ソースコードを関数くらいの塊に切って Jev で採点し、探したい内容と意味が近い順に並べる
///
/// パスを省略して標準入力にパスを流すと、それを使う (例: rg -l retry | file-search "リトライしている処理")
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
    /// 採点する塊の数の上限 (API 呼び出し量の歯止め)
    #[arg(short, long, default_value_t = 1000)]
    max: usize,
    #[arg(short, long, env = "JEV_PROVIDER", value_enum, default_value_t = ProviderKind::Typesafe)]
    provider: ProviderKind,
    /// このコマンドを使うスキルを ~/.claude/skills か ~/.codex/skills に書き出して終わる
    #[arg(long, value_enum, value_name = "AGENT", conflicts_with_all = ["query", "paths"])]
    install_skill: Option<Agent>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Some(agent) = cli.install_skill {
        println!("{}", agent.install()?.display());
        return Ok(());
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
    let paths = files::expand(&roots)?;

    let mut rest = paths.iter().flat_map(|p| Block::read(p));
    let blocks: Vec<Block> = rest.by_ref().take(cli.max).collect();
    if blocks.is_empty() {
        bail!("テキストファイルが見つかりません");
    }
    if rest.next().is_some() {
        eprintln!("{} 塊で打ち切りました (--max で増やせます)", cli.max);
    }

    let scores = score::score_all(cli.provider, &query, &blocks)?;
    let mut ranked: Vec<_> = scores.into_iter().zip(&blocks).collect();
    ranked.sort_unstable_by(|a, b| b.0.total_cmp(&a.0));
    for (s, b) in ranked.into_iter().take(cli.num) {
        println!("{} {s:.1}  {}:{}  {}", score::stars(s), b.path.display(), b.line, b.head());
    }
    Ok(())
}
