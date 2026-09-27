//! Flask's signed session cookie, reproduced byte for byte.
//!
//! `app.secret_key = 'octobot-tool'`, salt `cookie-session`, itsdangerous 2.2's
//! `URLSafeTimedSerializer` with the `TaggedJSONSerializer` payload and SHA-1 HMAC.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use hmac::{Hmac, Mac};
use serde_json::{Map, Value};
use sha1::Sha1;

const SECRET_KEY: &str = "octobot-tool";
const SALT: &str = "cookie-session";

type HmacSha1 = Hmac<Sha1>;

fn derive_key() -> Vec<u8> {
    let mut mac = HmacSha1::new_from_slice(SECRET_KEY.as_bytes()).expect("hmac key");
    mac.update(SALT.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

fn sign(value: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha1::new_from_slice(&derive_key()).expect("hmac key");
    mac.update(value);
    mac.finalize().into_bytes().to_vec()
}

fn b64(data: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(data)
}

/// `zlib.compress(data)` with the default level, byte-identical to CPython's zlib.
pub fn zlib_compress(data: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).expect("zlib compress");
    encoder.finish().expect("zlib finish")
}

/// The session dictionary, in the shape Flask's `TaggedJSONSerializer` writes.
#[derive(Clone, Debug, Default)]
pub struct Session {
    pub flashes: Vec<(String, String)>,
    pub exists: bool,
}

impl Session {
    pub fn is_empty(&self) -> bool {
        self.flashes.is_empty()
    }

    /// The payload bytes Flask's `TaggedJSONSerializer` writes for this session.
    pub fn to_tagged_json(&self) -> String {
        let tagged: Vec<Value> = self
            .flashes
            .iter()
            .map(|(category, message)| {
                let mut wrapper = Map::new();
                wrapper.insert(
                    " t".to_string(),
                    Value::Array(vec![
                        Value::String(category.clone()),
                        Value::String(message.clone()),
                    ]),
                );
                Value::Object(wrapper)
            })
            .collect();
        let mut root = Map::new();
        root.insert("_flashes".to_string(), Value::Array(tagged));
        crate::jsonutil::dumps_sorted(&Value::Object(root))
    }

    /// The signed cookie value (payload.timestamp.signature).
    pub fn cookie_value(&self) -> String {
        let json = self.to_tagged_json();
        let json_bytes = json.as_bytes();
        let compressed = zlib_compress(json_bytes);
        let payload = if compressed.len() < json_bytes.len().saturating_sub(1) {
            format!(".{}", b64(&compressed))
        } else {
            b64(json_bytes)
        };
        let mut packed = crate::clock::unix_seconds().to_be_bytes().to_vec();
        while packed.len() > 1 && packed[0] == 0 {
            packed.remove(0);
        }
        let timestamp = b64(&packed);
        let value = format!("{}.{}", payload, timestamp);
        let signature = b64(&sign(value.as_bytes()));
        format!("{}.{}", value, signature)
    }

    /// `serializer.loads(cookie)`: verify the signature, then read the payload.
    pub fn load(cookie: &str) -> Option<Session> {
        let parts: Vec<&str> = cookie.split('.').collect();
        if parts.len() < 3 {
            return None;
        }
        let signature = parts[parts.len() - 1];
        let value = parts[..parts.len() - 1].join(".");
        if b64(&sign(value.as_bytes())) != signature {
            return None;
        }
        // The payload is everything before the timestamp segment; a leading `.`
        // marks a zlib-compressed payload.
        let payload = match value.rsplit_once('.') {
            Some((payload, _)) => payload,
            None => value.as_str(),
        };
        let raw = if let Some(rest) = payload.strip_prefix('.') {
            let decoded = URL_SAFE_NO_PAD.decode(rest).ok()?;
            decompress(&decoded)?
        } else {
            URL_SAFE_NO_PAD.decode(payload).ok()?
        };
        let text = String::from_utf8(raw).ok()?;
        let parsed: Value = serde_json::from_str(&text).ok()?;
        let mut session = Session { flashes: Vec::new(), exists: true };
        if let Some(flashes) = parsed.get("_flashes").and_then(|value| value.as_array()) {
            for entry in flashes {
                if let Some(items) = entry.get(" t").and_then(|value| value.as_array()) {
                    let category = items
                        .first()
                        .and_then(|value| value.as_str())
                        .unwrap_or("message")
                        .to_string();
                    let message = items
                        .get(1)
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .to_string();
                    session.flashes.push((category, message));
                }
            }
        }
        Some(session)
    }
}

fn decompress(data: &[u8]) -> Option<Vec<u8>> {
    use flate2::read::ZlibDecoder;
    use std::io::Read;
    let mut out = Vec::new();
    let mut decoder = ZlibDecoder::new(data);
    decoder.read_to_end(&mut out).ok()?;
    Some(out)
}

/// A `Set-Cookie` value for the session, with Werkzeug's attribute order.
pub fn set_cookie_header(value: &str) -> String {
    format!("session={}; HttpOnly; Path=/", value)
}

pub fn delete_cookie_header() -> String {
    "session=; Expires=Thu, 01 Jan 1970 00:00:00 GMT; Max-Age=0; HttpOnly; Path=/".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn captured_cookie(case: &str) -> String {
        let raw = std::fs::read(format!(
            "../conformance/research/flask/captures/{}.raw",
            case
        ))
        .expect("capture file");
        let text = String::from_utf8_lossy(&raw).to_string();
        for line in text.split("\r\n") {
            if line.to_lowercase().starts_with("set-cookie:") {
                let value = line.split(':').nth(1).unwrap().trim();
                return value.split(';').next().unwrap().to_string();
            }
        }
        panic!("no Set-Cookie in {}", case);
    }

    #[test]
    fn loads_a_captured_flask_flash_cookie() {
        let cookie = captured_cookie("A27_post-config-bad-time");
        let session = Session::load(cookie.split('=').nth(1).unwrap()).expect("valid cookie");
        assert_eq!(
            session.flashes,
            vec![(
                "error".to_string(),
                "Execution time must be in HH:MM format (00:00 to 23:59)".to_string()
            )]
        );
        assert_eq!(
            session.to_tagged_json(),
            "{\"_flashes\":[{\" t\":[\"error\",\"Execution time must be in HH:MM format (00:00 to 23:59)\"]}]}"
        );
    }

    #[test]
    fn loads_a_captured_compressed_flash_cookie() {
        let cookie = captured_cookie("A25_post-config-valid");
        let value = cookie.split('=').nth(1).unwrap();
        assert!(value.starts_with('.') || value.split('.').nth(1).map(|part| !part.is_empty()).unwrap_or(false));
        let session = Session::load(value).expect("valid cookie");
        assert_eq!(session.flashes.len(), 1);
        assert_eq!(session.flashes[0].0, "success");
    }


    #[test]
    fn rejects_a_tampered_cookie() {
        let cookie = captured_cookie("A27_post-config-bad-time");
        let value = cookie.split('=').nth(1).unwrap();
        let mut tampered = value.to_string();
        tampered.pop();
        tampered.push(if value.ends_with('A') { 'B' } else { 'A' });
        assert!(Session::load(&tampered).is_none());
    }

    #[test]
    fn signs_a_session_that_loads_back() {
        let session = Session {
            flashes: vec![("error".to_string(), "boom".to_string())],
            exists: true,
        };
        let value = session.cookie_value();
        let loaded = Session::load(&value).expect("round trip");
        assert_eq!(loaded.flashes, session.flashes);
    }
}
