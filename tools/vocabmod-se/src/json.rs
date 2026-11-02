//! Minimal deterministic JSON writing: object keys in construction order,
//! two-space indentation, LF line endings, one trailing newline. Strings
//! escape only what JSON requires; non-ASCII text stays literal UTF-8.

/// One JSON value as the exports build it.
pub enum Value {
    String(String),
    Array(Vec<Value>),
    /// Members in the order they are written.
    Object(Vec<(&'static str, Value)>),
}

impl Value {
    pub fn str(text: &str) -> Self {
        Value::String(text.to_owned())
    }
}

/// A whole document, pretty-printed, terminator included.
pub fn document(value: &Value) -> String {
    let mut text = String::new();
    write(&mut text, value, 0);
    text.push('\n');
    text
}

fn write(out: &mut String, value: &Value, depth: usize) {
    match value {
        Value::String(text) => quote(out, text),
        Value::Array(items) if items.is_empty() => out.push_str("[]"),
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                newline(out, depth + 1);
                write(out, item, depth + 1);
            }
            newline(out, depth);
            out.push(']');
        }
        Value::Object(members) if members.is_empty() => out.push_str("{}"),
        Value::Object(members) => {
            out.push('{');
            for (index, (key, member)) in members.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                newline(out, depth + 1);
                quote(out, key);
                out.push_str(": ");
                write(out, member, depth + 1);
            }
            newline(out, depth);
            out.push('}');
        }
    }
}

fn newline(out: &mut String, depth: usize) {
    out.push('\n');
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn quote(out: &mut String, text: &str) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
}
