//! `json.dumps` with the defaults `requests` and Apprise use (`ensure_ascii=True`,
//! separators `", "` / `": "`, `repr(float)` for numbers).

use serde_json::Value;

/// Python's `json.dumps(text)` for a single string.
pub fn dumps_str(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            ch if (ch as u32) < 0x20 || (ch as u32) > 0x7e => {
                let code = ch as u32;
                if code > 0xffff {
                    let adjusted = code - 0x10000;
                    let high = 0xd800 + (adjusted >> 10);
                    let low = 0xdc00 + (adjusted & 0x3ff);
                    out.push_str(&format!("\\u{:04x}\\u{:04x}", high, low));
                } else {
                    out.push_str(&format!("\\u{:04x}", code));
                }
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// Python's `json.dumps(value)` with default settings.
pub fn dumps_value(value: &Value) -> String {
    let mut out = String::new();
    write_value(value, &mut out);
    out
}

fn write_value(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(number) => {
            if let Some(unsigned) = number.as_u64() {
                out.push_str(&unsigned.to_string());
            } else if let Some(signed) = number.as_i64() {
                out.push_str(&signed.to_string());
            } else {
                out.push_str(&crate::pyrepr::repr_float(number.as_f64().unwrap_or(f64::NAN)));
            }
        }
        Value::String(text) => out.push_str(&dumps_str(text)),
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                write_value(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            for (index, (key, item)) in map.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                out.push_str(&dumps_str(key));
                out.push_str(": ");
                write_value(item, out);
            }
            out.push('}');
        }
    }
}

/// Flask's session JSON: `sort_keys=True`, separators `(",", ":")`, `ensure_ascii=True`.
pub fn dumps_sorted(value: &Value) -> String {
    let mut out = String::new();
    write_sorted(value, &mut out);
    out
}

fn write_sorted(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(number) => {
            if let Some(unsigned) = number.as_u64() {
                out.push_str(&unsigned.to_string());
            } else if let Some(signed) = number.as_i64() {
                out.push_str(&signed.to_string());
            } else {
                out.push_str(&crate::pyrepr::repr_float(number.as_f64().unwrap_or(f64::NAN)));
            }
        }
        Value::String(text) => out.push_str(&dumps_str(text)),
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_sorted(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&dumps_str(key));
                out.push(':');
                write_sorted(&map[*key], out);
            }
            out.push('}');
        }
    }
}
