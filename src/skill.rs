use anyhow::{Context, Result};
use clap::ValueEnum;
use std::path::{Path, PathBuf};

const SKILL: &str = include_str!("../skill.md");

#[derive(Clone, Copy, ValueEnum)]
pub enum Agent {
    Claude,
    Codex,
}

impl Agent {
    /// スキルを書き出し、そのパスを返す
    pub fn install(self) -> Result<PathBuf> {
        install(&self.home()?)
    }

    /// エージェントの設定ディレクトリ。環境変数で移していればそちら
    fn home(self) -> Result<PathBuf> {
        let (var, dir) = match self {
            Agent::Claude => ("CLAUDE_CONFIG_DIR", ".claude"),
            Agent::Codex => ("CODEX_HOME", ".codex"),
        };
        if let Some(p) = std::env::var_os(var) {
            return Ok(p.into());
        }
        Ok(std::env::home_dir().context("ホームディレクトリが分かりません")?.join(dir))
    }
}

/// base/skills/jev-sift/SKILL.md に書き出し、そのパスを返す (既にあれば上書き)
fn install(base: &Path) -> Result<PathBuf> {
    let dir = base.join("skills/jev-sift");
    std::fs::create_dir_all(&dir).with_context(|| format!("{} を作れません", dir.display()))?;
    let path = dir.join("SKILL.md");
    std::fs::write(&path, SKILL).with_context(|| format!("{} に書けません", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_writes_skill() {
        assert!(SKILL.starts_with("---\nname: jev-sift\ndescription: "));
        let d = std::env::temp_dir().join(format!("jev-sift-skill-{}", std::process::id()));
        let path = install(&d).unwrap();
        let written = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_dir_all(&d).unwrap();
        assert_eq!(path, d.join("skills/jev-sift/SKILL.md"));
        assert_eq!(written, SKILL);
    }
}
