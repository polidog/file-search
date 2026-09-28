use std::path::{Path, PathBuf};

/// これより短い塊は次の塊とつなげる (行数)
const MIN_LINES: usize = 3;
/// これより長い塊は次の空行で切る (行数)
const MAX_LINES: usize = 40;
/// これより大きいファイルは生成物とみなして読まない
const MAX_FILE_BYTES: u64 = 1 << 20;

/// 読み込んだファイルと、そこから切り出した塊
pub struct SourceFile {
    pub path: PathBuf,
    pub lines: Vec<String>,
    pub blocks: Vec<Block>,
}

/// 関数くらいの大きさのコード片。行番号は 1 始まりで end を含む
pub struct Block {
    pub start: usize,
    pub end: usize,
}

impl SourceFile {
    /// ファイルを読んで塊に切る。表 (CSV / Excel) は table に任せる。大きすぎる・UTF-8 でない・NUL を含む・空のファイルは None
    pub fn read(path: &Path) -> Option<SourceFile> {
        if crate::table::is_table(path) {
            return crate::table::read(path);
        }
        if std::fs::metadata(path).ok()?.len() > MAX_FILE_BYTES {
            return None;
        }
        let text = std::fs::read_to_string(path).ok().filter(|t| !t.contains('\0'))?;
        let blocks: Vec<Block> = split(&text).into_iter().map(|(start, end)| Block { start, end }).collect();
        (!blocks.is_empty()).then(|| SourceFile { path: path.to_owned(), lines: text.lines().map(String::from).collect(), blocks })
    }

    /// 出力用の見出し (塊の 1 行目)
    pub fn head(&self, b: &Block) -> &str {
        self.lines[b.start - 1].trim()
    }
}

/// 空行のあとに行頭から始まる行で切り、(開始行, 終了行) を返す。
/// MIN_LINES 未満の塊は次とつなげ、MAX_LINES を超えたら次の空行で切る
// ponytail: 字下げで判断するだけ。impl / class の中のメソッドは MAX_LINES でしか切れない。正確にやるなら tree-sitter
fn split(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = 0;
    // 塊に入れた行数と、そのうち空でない行数と、最後の空でない行
    let (mut len, mut filled, mut last) = (0, 0, 0);
    let mut prev_blank = true;
    for (i, line) in text.lines().enumerate() {
        let n = i + 1;
        let blank = line.trim().is_empty();
        let top = !blank && !line.starts_with(char::is_whitespace);
        if (prev_blank && top && filled >= MIN_LINES) || (blank && len >= MAX_LINES) {
            out.push((start, last));
            (len, filled) = (0, 0);
        }
        prev_blank = blank;
        if blank && len == 0 {
            continue;
        }
        if len == 0 {
            start = n;
        }
        len += 1;
        if !blank {
            filled += 1;
            last = n;
        }
    }
    if filled > 0 {
        out.push((start, last));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_by_top_level() {
        let src = "use a;\nuse b;\n\nconst X: u8 = 1;\n\n/// doc\nfn f() {\n    1;\n\n    2;\n}\n\n\nfn g() {\n}\n";
        // use と const は短いのでつながる。関数の中の空行では切らない
        assert_eq!(split(src), [(1, 4), (6, 11), (14, 15)]);
    }

    #[test]
    fn split_long_block_at_blank() {
        let body = "    x;\n".repeat(MAX_LINES) + "\n    y;\n";
        let got = split(&format!("impl A {{\n{body}}}\n"));
        assert_eq!(got, [(1, MAX_LINES + 1), (MAX_LINES + 3, MAX_LINES + 4)]);
    }

    #[test]
    fn read_skips_binary() {
        let d = std::env::temp_dir().join(format!("jev-sift-block-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("a.md"), "\nhello\n").unwrap();
        std::fs::write(d.join("bin"), [0xff, 0xfe, 0x00]).unwrap();
        std::fs::write(d.join("nul"), "a\0b").unwrap();
        std::fs::write(d.join("empty"), "\n \n").unwrap();
        let a = SourceFile::read(&d.join("a.md"));
        let others = ["bin", "nul", "empty"].map(|f| SourceFile::read(&d.join(f)).is_none());
        std::fs::remove_dir_all(&d).unwrap();
        let a = a.unwrap();
        assert_eq!((a.blocks.len(), a.head(&a.blocks[0])), (1, "hello"));
        assert_eq!(a.blocks[0].start, 2);
        assert_eq!(others, [true; 3]);
    }
}
