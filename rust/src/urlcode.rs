//! `urllib.parse.quote` / `urlencode`, as used by the Home Assistant client.

const UNRESERVED: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_.-~";

/// `urllib.parse.quote(text, safe="")`
pub fn quote(text: &str) -> String {
    let mut out = String::new();
    for byte in text.as_bytes() {
        if UNRESERVED.contains(byte) {
            out.push(*byte as char);
        } else {
            out.push_str(&format!("%{:02X}", byte));
        }
    }
    out
}

/// `urllib.parse.urlencode` (quote_plus): spaces become `+`.
pub fn quote_plus(text: &str) -> String {
    let mut out = String::new();
    for byte in text.as_bytes() {
        match *byte {
            b' ' => out.push('+'),
            other if UNRESERVED.contains(&other) => out.push(other as char),
            other => out.push_str(&format!("%{:02X}", other)),
        }
    }
    out
}

/// `requests`' `params=` encoding: `key=value&key2=value2`.
pub fn encode_params(params: &[(&str, &str)]) -> String {
    params
        .iter()
        .map(|(key, value)| format!("{}={}", quote_plus(key), quote_plus(value)))
        .collect::<Vec<_>>()
        .join("&")
}
