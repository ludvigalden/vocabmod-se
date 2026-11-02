//! Minimal deterministic CSV writing: fields quoted only when a comma,
//! quote, or newline makes quoting necessary, LF line endings.

/// One encoded row, terminator included.
pub fn row(fields: &[&str]) -> String {
    let mut line = String::new();
    for (index, field) in fields.iter().enumerate() {
        if index > 0 {
            line.push(',');
        }
        if field.contains([',', '"', '\n']) {
            line.push('"');
            line.push_str(&field.replace('"', "\"\""));
            line.push('"');
        } else {
            line.push_str(field);
        }
    }
    line.push('\n');
    line
}

/// A whole table: header plus rows, deterministically encoded.
pub fn table<const N: usize>(header: &[&str; N], rows: &[[String; N]]) -> String {
    let mut text = String::with_capacity(64 + rows.len() * 96);
    text.push_str(&row(header));
    for cells in rows {
        let fields: Vec<&str> = cells.iter().map(String::as_str).collect();
        text.push_str(&row(&fields));
    }
    text
}
