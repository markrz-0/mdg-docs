//! JavaScript-compatible text helpers.
//!
//! MDG files and CLI output must stay byte-identical with the original
//! TypeScript implementation, so JSON is written exactly like
//! `JSON.stringify` (number formatting, key order, escaping) and strings are
//! trimmed and compared with JavaScript semantics.

use std::cmp::Ordering;

use serde::Serialize;
use serde_json::Value;

use crate::jsmap::js_key_order;

/// Formats a number like JavaScript's `Number.prototype.toString`.
/// Non-finite values become `null`, as in `JSON.stringify`.
pub fn number_to_string(n: f64) -> String {
    if !n.is_finite() {
        return "null".to_string();
    }
    let mut buffer = ryu_js::Buffer::new();
    buffer.format_finite(n).to_string()
}

fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// JSON-encodes a string like `JSON.stringify`.
pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    write_string(&mut out, s);
    out
}

fn write_value(out: &mut String, value: &Value, indent: Option<&str>, depth: usize) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&number_to_string(n.as_f64().unwrap_or(f64::NAN))),
        Value::String(s) => write_string(out, s),
        Value::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                newline(out, indent, depth + 1);
                write_value(out, item, indent, depth + 1);
            }
            newline(out, indent, depth);
            out.push(']');
        }
        Value::Object(map) => {
            if map.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            for (i, key) in js_key_order(map.keys().map(String::as_str)).into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                newline(out, indent, depth + 1);
                write_string(out, key);
                out.push(':');
                if indent.is_some() {
                    out.push(' ');
                }
                write_value(out, &map[key], indent, depth + 1);
            }
            newline(out, indent, depth);
            out.push('}');
        }
    }
}

fn newline(out: &mut String, indent: Option<&str>, depth: usize) {
    if let Some(indent) = indent {
        out.push('\n');
        for _ in 0..depth {
            out.push_str(indent);
        }
    }
}

/// `JSON.stringify(value)`.
pub fn stringify(value: &Value) -> String {
    let mut out = String::new();
    write_value(&mut out, value, None, 0);
    out
}

/// `JSON.stringify(value, null, 2)`.
pub fn stringify_pretty(value: &Value) -> String {
    let mut out = String::new();
    write_value(&mut out, value, Some("  "), 0);
    out
}

/// Serializes any value to a JSON tree, keeping struct field order.
pub fn to_value<T: Serialize + ?Sized>(value: &T) -> Value {
    serde_json::to_value(value).expect("MDG values always serialize to JSON")
}

/// `JSON.stringify(serializable, null, 2)`.
pub fn to_pretty_json<T: Serialize + ?Sized>(value: &T) -> String {
    stringify_pretty(&to_value(value))
}

/// JavaScript `WhiteSpace` and `LineTerminator` characters (what
/// `String.prototype.trim` removes).
pub fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{9}' | '\u{a}' | '\u{b}' | '\u{c}' | '\u{d}' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
    )
}

/// `String.prototype.trim`.
pub fn trim(s: &str) -> &str {
    s.trim_matches(is_js_whitespace)
}

/// `String.prototype.trimEnd`.
pub fn trim_end(s: &str) -> &str {
    s.trim_end_matches(is_js_whitespace)
}

/// Compares strings by UTF-16 code units, like JavaScript's `<`.
pub fn compare_utf16(a: &str, b: &str) -> Ordering {
    if a.is_ascii() && b.is_ascii() {
        return a.cmp(b);
    }
    a.encode_utf16().cmp(b.encode_utf16())
}

/// String length in UTF-16 code units, like JavaScript's `length`.
pub fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn numbers_format_like_javascript() {
        assert_eq!(number_to_string(380.0), "380");
        assert_eq!(number_to_string(-0.0), "0");
        assert_eq!(number_to_string(0.1 + 0.2), "0.30000000000000004");
        assert_eq!(number_to_string(1e21), "1e+21");
        assert_eq!(number_to_string(1.5e-7), "1.5e-7");
        assert_eq!(number_to_string(123456789012345680000.0), "123456789012345680000");
        assert_eq!(number_to_string(f64::NAN), "null");
    }

    #[test]
    fn stringify_matches_json_stringify() {
        let v = json!({ "b": 1.0, "10": [], "a": { "x": "q\"\n\u{1}é" }, "2": {} });
        assert_eq!(stringify(&v), r#"{"2":{},"10":[],"b":1,"a":{"x":"q\"\n\u0001é"}}"#);
        assert_eq!(
            stringify_pretty(&json!({ "a": [1, { "b": null }], "c": true })),
            "{\n  \"a\": [\n    1,\n    {\n      \"b\": null\n    }\n  ],\n  \"c\": true\n}"
        );
    }

    #[test]
    fn trims_like_javascript() {
        assert_eq!(trim("\u{feff} a \u{2028}"), "a");
        assert_eq!(trim("\u{85}a"), "\u{85}a");
        assert_eq!(trim_end("a \n"), "a");
    }

    #[test]
    fn utf16_order() {
        assert_eq!(compare_utf16("a", "b"), Ordering::Less);
        // U+FF61 sorts after U+1F600 in code points but before it in UTF-16.
        assert_eq!(compare_utf16("\u{1F600}", "\u{FF61}"), Ordering::Less);
    }
}
