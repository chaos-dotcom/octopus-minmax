//! Python-compatible `repr()` and formatting.
//!
//! The Python implementation logs parsed JSON with `f"{value}"`, which is Python's
//! `repr`, and logs floats with `f"{x:.2f}"`.  Both have to be reproduced exactly
//! for the log output (and therefore the notification text) to match byte for byte.

use serde_json::Value;

/// `repr(float)` - shortest round-trip digits, Python's exponent formatting.
pub fn repr_float(value: f64) -> String {
    if value.is_nan() {
        return "nan".to_string();
    }
    if value.is_infinite() {
        return if value > 0.0 { "inf" } else { "-inf" }.to_string();
    }
    // Rust's Debug formatting for f64 produces the same shortest round-trip digits
    // as Python's repr; only the exponent spelling differs (`1e16` vs `1e+16`).
    let raw = format!("{:?}", value);
    match raw.find(['e', 'E']) {
        None => raw,
        Some(index) => {
            let (mantissa, exponent) = raw.split_at(index);
            let digits = &exponent[1..];
            let (sign, magnitude) = match digits.strip_prefix('-') {
                Some(rest) => ("-", rest),
                None => ("+", digits),
            };
            let padded = if magnitude.len() < 2 {
                format!("0{}", magnitude)
            } else {
                magnitude.to_string()
            };
            format!("{}e{}{}", mantissa, sign, padded)
        }
    }
}

/// `repr(str)` - Python's quote selection and escape rules.
pub fn repr_str(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(text.len() + 2);
    out.push(quote);
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if is_printable(c) => out.push(c),
            c if (c as u32) < 0x100 => out.push_str(&format!("\\x{:02x}", c as u32)),
            c if (c as u32) < 0x10000 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push_str(&format!("\\U{:08x}", c as u32)),
        }
    }
    out.push(quote);
    out
}

/// Approximation of Python's `str.isprintable()`, which is what `repr` uses.
fn is_printable(ch: char) -> bool {
    let code = ch as u32;
    if code < 0x20 || code == 0x7f {
        return false;                          // C0 controls and DEL
    }
    if (0x80..=0x9f).contains(&code) {
        return false;                          // C1 controls
    }
    if (0x7f..=0x9f).contains(&code) {
        return false;
    }
    // Format characters and separators that Python reports as non-printable.
    matches!(code,
        0x00ad | 0x061c | 0x180e | 0x200b..=0x200f | 0x2028 | 0x2029 | 0x202a..=0x202e
        | 0x2060..=0x2064 | 0x2066..=0x206f | 0xfeff | 0xfff9..=0xfffb | 0x110bd | 0x110cd
        | 0xe0001 | 0xe0020..=0xe007f) == false
}

/// `repr(value)` for a parsed JSON document.
pub fn repr_json(value: &Value) -> String {
    let mut out = String::new();
    write_repr(value, &mut out);
    out
}

fn write_repr(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("None"),
        Value::Bool(true) => out.push_str("True"),
        Value::Bool(false) => out.push_str("False"),
        Value::Number(number) => {
            if let Some(unsigned) = number.as_u64() {
                out.push_str(&unsigned.to_string());
            } else if let Some(signed) = number.as_i64() {
                out.push_str(&signed.to_string());
            } else {
                out.push_str(&repr_float(number.as_f64().unwrap_or(f64::NAN)));
            }
        }
        Value::String(text) => out.push_str(&repr_str(text)),
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                write_repr(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            for (index, (key, item)) in map.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                out.push_str(&repr_str(key));
                out.push_str(": ");
                write_repr(item, out);
            }
            out.push('}');
        }
    }
}

/// `f"{value:.Nf}"` - Python's fixed-point float formatting.
pub fn fixed(value: f64, digits: usize) -> String {
    format!("{:.*}", digits, value)
}

/// `float(f"{value:.4f}")` - format then parse back, as the Python engine does.
pub fn round_trip_4(value: f64) -> f64 {
    fixed(value, 4).parse::<f64>().unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn float_repr_matches_python() {
        // Values and expected output produced by CPython 3.11.15 `repr()`.
        let cases: [(f64, &str); 16] = [
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (1.0, "1.0"),
            (75.0, "75.0"),
            (0.1, "0.1"),
            (1.0 / 3.0, "0.3333333333333333"),
            (1e15, "1000000000000000.0"),
            (1e16, "1e+16"),
            (1e17, "1e+17"),
            (1e-4, "0.0001"),
            (1e-5, "1e-05"),
            (1e300, "1e+300"),
            (5e-324, "5e-324"),
            (1.7976931348623157e308, "1.7976931348623157e+308"),
            (5.399999999999999, "5.399999999999999"),
            (1000.0, "1000.0"),
        ];
        for (value, expected) in cases {
            assert_eq!(repr_float(value), expected, "repr({:?})", value);
        }
    }

    #[test]
    fn fixed_formatting_matches_python() {
        assert_eq!(fixed(0.6199999999999999, 2), "0.62");
        assert_eq!(fixed(0.75, 4), "0.7500");
        assert_eq!(fixed(29.75 / 100.0, 2), "0.30");
        assert_eq!(round_trip_4(0.12345 * 3.0), 0.3704);
    }

    #[test]
    fn dict_and_list_repr_matches_python() {
        let value = json!({"data": {"obtainKrakenToken": {"token": "abc"}}, "n": 1, "ok": true});
        assert_eq!(
            repr_json(&value),
            "{'data': {'obtainKrakenToken': {'token': 'abc'}}, 'n': 1, 'ok': True}"
        );
        let list = json!([1, 2.5, null, false, "x"]);
        assert_eq!(repr_json(&list), "[1, 2.5, None, False, 'x']");
    }

    #[test]
    fn string_repr_matches_python() {
        assert_eq!(repr_str("it's"), "\"it's\"");
        assert_eq!(repr_str("both ' and \""), "'both \\' and \"'");
        assert_eq!(repr_str("tab\tnl\n"), "'tab\\tnl\\n'");
        assert_eq!(repr_str("café"), "'café'");
        assert_eq!(repr_str("\u{7f}"), "'\\x7f'");
        assert_eq!(repr_str("\u{1f600}"), "'😀'");
    }
}
