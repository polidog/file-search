use std::path::{Path, PathBuf};

/// Jev に渡す 1 塊あたりの長さ (文字数)
const BLOCK_CHARS: usize = 1000;
/// これより短い塊は次の塊とつなげる (行数)
const MIN_LINES: usize = 3;
/// これより長い塊は次の空行で切る (行数)
const MAX_LINES: usize = 40;
/// これより大きいファイルは生成物とみなして読まない
const MAX_FILE_BYTES: u64 = 1 << 20;

/// ファイルから切り出した、関数くらいの大きさのコード片
pub struct Block {
    pub path: PathBuf,
    /// 1 始まりの行番号
    pub line: usize,
    pub text: String,
}

impl Block {
    /// ファイルを読んで塊に切る。大きすぎる・UTF-8 でない・NUL を含むファイルは空
    pub fn read(path: &Path) -> Vec<Block> {
        let too_big = std::fs::metadata(path).map_or(true, |m| m.len() > MAX_FILE_BYTES);
        let text = match std::fs::read_to_string(path) {
            Ok(t) if !too_big && !t.contains('\0') => t,
            _ => return Vec::new(),
        };
        split(&text).into_iter().map(|(line, text)| Block { path: path.to_owned(), line, text }).collect()
    }

    /// 出力用の見出し (塊の 1 行目)
    pub fn head(&self) -> &str {
        self.text.lines().next().unwrap_or_default().trim()
    }
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn read_skips_binary() {
        let d = std::env::temp_dir().join(format!("file-search-block-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("a.md"), "hello").unwrap();
        std::fs::write(d.join("bin"), [0xff, 0xfe, 0x00]).unwrap();
        std::fs::write(d.join("nul"), "a\0b").unwrap();
        let a = Block::read(&d.join("a.md"));
        let others = Block::read(&d.join("bin")).len() + Block::read(&d.join("nul")).len();
        std::fs::remove_dir_all(&d).unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!((a[0].line, a[0].head()), (1, "hello"));
        assert_eq!(others, 0);
    }
}
