//! Reading the delimited tables the tool writes: a minimal CSV reader
//! matching the tool's deterministic writer (fields quoted only when a
//! comma, quote, or newline makes quoting necessary; LF terminators)
//! and a plain TSV reader.

use std::path::Path;

/// One parsed delimited table: its header and its data rows.
pub(crate) struct Table {
    pub(crate) header: Vec<String>,
    pub(crate) rows: Vec<Vec<String>>,
}

impl Table {
    pub(crate) fn column(&self, name: &str) -> Option<usize> {
        self.header.iter().position(|h| h == name)
    }
}

/// Read a CSV file the tool's writer produced. A single-field row whose
/// only field starts with `#` is a trailing comment, skipped; the final
/// LF terminator never produces an empty row.
pub(crate) fn read_csv(path: &Path) -> Result<Table, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let text = String::from_utf8(bytes)
        .map_err(|_| format!("{} is not valid UTF-8", path.display()))?;
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut field = String::new();
    let mut row: Vec<String> = Vec::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(ch);
            }
        } else if ch == '"' && field.is_empty() {
            in_quotes = true;
        } else if ch == ',' {
            row.push(std::mem::take(&mut field));
        } else if ch == '\n' {
            row.push(std::mem::take(&mut field));
            if row.len() == 1 && row[0].starts_with('#') {
                row.clear();
            } else {
                rows.push(std::mem::take(&mut row));
            }
        } else {
            field.push(ch);
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    if rows.is_empty() {
        return Err(format!("{} has no header row", path.display()));
    }
    let header = rows.remove(0);
    Ok(Table { header, rows })
}

/// Read a TSV file: no quoting, tab-separated.
pub(crate) fn read_tsv(path: &Path) -> Result<Table, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let text = String::from_utf8(bytes)
        .map_err(|_| format!("{} is not valid UTF-8", path.display()))?;
    let mut lines = text.lines();
    let Some(header) = lines.next() else {
        return Err(format!("{} has no header row", path.display()));
    };
    Ok(Table {
        header: header.split('\t').map(str::to_string).collect(),
        rows: lines
            .filter(|l| !l.is_empty())
            .map(|l| l.split('\t').map(str::to_string).collect())
            .collect(),
    })
}

/// Read a delimited table by extension: `.tsv` tab-separated, else CSV.
pub(crate) fn read_delimited(path: &Path) -> Result<Table, String> {
    if path.extension().is_some_and(|e| e == "tsv") {
        read_tsv(path)
    } else {
        read_csv(path)
    }
}
