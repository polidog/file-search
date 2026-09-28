//! CSV / TSV / Excel を表として読み、1 行を 1 塊にする
use crate::block::{Block, SourceFile};
use calamine::{Reader, open_workbook_auto};
use std::path::Path;

/// 拡張子で表と分かるか
pub fn is_table(path: &Path) -> bool {
    matches!(ext(path).as_deref(), Some("csv" | "tsv" | "xlsx" | "xlsm" | "xlsb" | "xls" | "ods"))
}

fn ext(path: &Path) -> Option<String> {
    path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase())
}

/// 表を読む。1 行目を見出し、2 行目からを塊にする。読めなければ None。Excel は最初のシートだけ
pub fn read(path: &Path) -> Option<SourceFile> {
    let ext = ext(path)?;
    let rows: Vec<Vec<String>> = if ext == "csv" || ext == "tsv" {
        let delimiter = if ext == "tsv" { b'\t' } else { b',' };
        let mut reader =
            csv::ReaderBuilder::new().has_headers(false).flexible(true).delimiter(delimiter).from_path(path).ok()?;
        let rows: Result<_, _> = reader.records().map(|r| r.map(|r| r.iter().map(String::from).collect())).collect();
        rows.ok()?
    } else {
        let mut book = open_workbook_auto(path).ok()?;
        let range = book.worksheet_range_at(0)?.ok()?;
        range.rows().map(|r| r.iter().map(ToString::to_string).collect()).collect()
    };
    from_rows(path, rows)
}

/// 見出しの行は 1 行目にそのまま置き (塊にはしない)、2 行目からは `見出し: 値 | 見出し: 値` に組み直して 1 行 1 塊にする。
/// 空の行は塊にしない。行番号は見出しを 1 とした行の番号で、`sed -n Np` と合う
// ponytail: 引用符の中で改行している CSV は、この行番号とファイルの行番号がずれる
fn from_rows(path: &Path, mut rows: Vec<Vec<String>>) -> Option<SourceFile> {
    if rows.len() < 2 {
        return None;
    }
    let header = rows.remove(0);
    let mut lines = vec![header.join(" | ")];
    lines.extend(rows.iter().map(|row| render(&header, row)));
    let blocks: Vec<Block> =
        lines.iter().enumerate().skip(1).filter(|(_, l)| !l.is_empty()).map(|(i, _)| Block { start: i + 1, end: i + 1 }).collect();
    (!blocks.is_empty()).then(|| SourceFile { path: path.to_owned(), lines, blocks })
}

/// 空のセルは飛ばし、値の中の改行や連続した空白は 1 つの空白にする (1 行に収めるため)
fn render(header: &[String], row: &[String]) -> String {
    row.iter()
        .enumerate()
        .map(|(i, v)| (header.get(i).map(String::as_str).unwrap_or(""), v.split_whitespace().collect::<Vec<_>>().join(" ")))
        .filter(|(_, v)| !v.is_empty())
        .map(|(h, v)| if h.is_empty() { v } else { format!("{h}: {v}") })
        .collect::<Vec<_>>()
        .join(" | ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_rows_become_blocks() {
        let d = std::env::temp_dir().join(format!("jev-sift-table-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("a.csv");
        std::fs::write(&p, "text,category\n\"hello, world\",a\n,\n\"two\nlines\",\n").unwrap();
        let f = read(&p);
        std::fs::remove_dir_all(&d).unwrap();
        let f = f.unwrap();
        // 見出しは 1 行目に置くだけ。空の 3 行目は塊にしない。値の中の改行は空白にする
        assert_eq!(f.lines[0], "text | category");
        assert_eq!(f.blocks.iter().map(|b| b.start).collect::<Vec<_>>(), [2, 4]);
        assert_eq!(f.head(&f.blocks[0]), "text: hello, world | category: a");
        assert_eq!(f.head(&f.blocks[1]), "text: two lines");
        assert!(is_table(Path::new("x.XLSX")) && !is_table(Path::new("x.rs")));
    }
}
