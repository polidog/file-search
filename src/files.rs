use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

const SKIP_DIRS: [&str; 3] = ["target", "node_modules", "vendor"];

/// ディレクトリは中のファイルに展開し、ファイルはそのまま使う
pub fn expand(roots: &[PathBuf]) -> Result<Vec<PathBuf>> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_skips_hidden_and_build_dirs() {
        let d = std::env::temp_dir().join(format!("file-search-files-{}", std::process::id()));
        std::fs::create_dir_all(d.join(".git")).unwrap();
        std::fs::create_dir_all(d.join("target")).unwrap();
        std::fs::write(d.join("a.md"), "").unwrap();
        std::fs::write(d.join(".git/x"), "").unwrap();
        std::fs::write(d.join("target/y"), "").unwrap();
        let paths = expand(&[d.clone(), d.join("target/y")]).unwrap();
        let missing = expand(&[d.join("missing")]);
        std::fs::remove_dir_all(&d).unwrap();
        // ディレクトリからは隠し・target を飛ばすが、名指ししたファイルは使う
        assert_eq!(paths, [d.join("a.md"), d.join("target/y")]);
        assert!(missing.is_err());
    }
}
