//! A minimal HTTP/1.1 client that writes the same bytes on the wire as
//! `requests` does for this application's calls (same header order, same casing,
//! same body encoding), and parses the responses the same way.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug)]
pub enum HttpError {
    Connect(String),
    Protocol(String),
    Timeout,
    Tls(String),
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpError::Connect(message) => write!(formatter, "{}", message),
            HttpError::Protocol(message) => write!(formatter, "{}", message),
            HttpError::Timeout => write!(formatter, "timed out"),
            HttpError::Tls(message) => write!(formatter, "{}", message),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub fn ok(&self) -> bool {
        self.status < 400
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }

    pub fn json(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::from_slice(&self.body)
    }
}

struct Url {
    secure: bool,
    host: String,
    port: u16,
    path: String,
}

fn parse_url(url: &str) -> Result<Url, HttpError> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| HttpError::Protocol(format!("Invalid URL '{}': No scheme supplied", url)))?;
    let (authority, path) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if port.chars().all(|ch| ch.is_ascii_digit()) && !port.is_empty() => {
            (host.to_string(), port.parse::<u16>().unwrap_or(80))
        }
        _ => (
            authority.to_string(),
            if scheme == "https" { 443 } else { 80 },
        ),
    };
    Ok(Url {
        secure: scheme == "https",
        host,
        port,
        path: path.to_string(),
    })
}

fn connect(url: &Url, timeout: Duration) -> Result<Box<dyn ReadWrite>, HttpError> {
    let address = format!("{}:{}", url.host, url.port);
    let stream = TcpStream::connect(&address).map_err(|error| {
        HttpError::Connect(format!(
            "HTTPConnectionPool(host='{}', port={}): Max retries exceeded with url (Caused by \
             NewConnectionError('<urllib3.connection.HTTPConnection object>: Failed to establish \
             a new connection: [Errno 61] Connection refused')): {}",
            url.host, url.port, error
        ))
    })?;
    stream.set_read_timeout(Some(timeout)).ok();
    stream.set_write_timeout(Some(timeout)).ok();
    if !url.secure {
        return Ok(Box::new(stream));
    }
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let server_name = rustls::pki_types::ServerName::try_from(url.host.clone())
        .map_err(|error| HttpError::Tls(error.to_string()))?;
    let connection = rustls::ClientConnection::new(Arc::new(config), server_name)
        .map_err(|error| HttpError::Tls(error.to_string()))?;
    let mut tls = rustls::StreamOwned::new(connection, stream);
    tls.flush().ok();
    Ok(Box::new(tls))
}

pub trait ReadWrite: Read + Write {}
impl<T: Read + Write> ReadWrite for T {}

/// One request, exactly as it will appear on the wire.
#[derive(Debug, Clone)]
pub struct RequestSpec {
    pub method: String,
    pub url: String,
    /// Headers written after `Host`, before `Content-Length`.
    pub headers: Vec<(String, String)>,
    /// Headers written after `Content-Length` (what `requests` does with the
    /// `Content-Type` it adds itself, and with basic auth).
    pub trailing: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

/// The bytes CPython's `http.client` writes for this request.
pub fn build_request_bytes(spec: &RequestSpec) -> Result<Vec<u8>, HttpError> {
    let parsed = parse_url(&spec.url)?;
    let host_header = if parsed.port == 80 || parsed.port == 443 {
        parsed.host.clone()
    } else {
        format!("{}:{}", parsed.host, parsed.port)
    };
    let mut request = format!("{} {} HTTP/1.1\r\n", spec.method, parsed.path);
    request.push_str(&format!("Host: {}\r\n", host_header));
    for (name, value) in &spec.headers {
        request.push_str(&format!("{}: {}\r\n", name, value));
    }
    if let Some(payload) = &spec.body {
        request.push_str(&format!("Content-Length: {}\r\n", payload.len()));
    }
    for (name, value) in &spec.trailing {
        request.push_str(&format!("{}: {}\r\n", name, value));
    }
    request.push_str("\r\n");
    let mut out = request.into_bytes();
    if let Some(payload) = &spec.body {
        out.extend_from_slice(payload);
    }
    Ok(out)
}

/// Send a prepared request.
pub fn send(spec: &RequestSpec, timeout: Duration) -> Result<Response, HttpError> {
    let parsed = parse_url(&spec.url)?;
    let mut stream = connect(&parsed, timeout)?;
    let bytes = build_request_bytes(spec)?;
    stream
        .write_all(&bytes)
        .map_err(|error| HttpError::Protocol(error.to_string()))?;
    stream
        .flush()
        .map_err(|error| HttpError::Protocol(error.to_string()))?;
    read_response(&mut stream)
}

/// Issue one request.  `headers` are written in the order given, after `Host`,
/// which is what CPython's `http.client` does.
pub fn request(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: Option<&[u8]>,
    timeout: Duration,
) -> Result<Response, HttpError> {
    let spec = RequestSpec {
        method: method.to_string(),
        url: url.to_string(),
        headers: headers.to_vec(),
        trailing: Vec::new(),
        body: body.map(|payload| payload.to_vec()),
    };
    send(&spec, timeout)
}

fn read_response(stream: &mut Box<dyn ReadWrite>) -> Result<Response, HttpError> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 16384];
    let header_end;
    loop {
        if let Some(position) = find_subsequence(&buffer, b"\r\n\r\n") {
            header_end = position + 4;
            break;
        }
        let read = stream
            .read(&mut chunk)
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => HttpError::Timeout,
                _ => HttpError::Protocol(error.to_string()),
            })?;
        if read == 0 {
            return Err(HttpError::Protocol("Connection closed before a response".to_string()));
        }
        buffer.extend_from_slice(&chunk[..read]);
    }

    let head = String::from_utf8_lossy(&buffer[..header_end - 4]).to_string();
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap_or_default();
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| HttpError::Protocol(format!("Bad status line: {}", status_line)))?;
    let mut headers = Vec::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.to_string(), value.trim_start().to_string()));
        }
    }

    let mut body = buffer[header_end..].to_vec();
    let chunked = headers
        .iter()
        .any(|(name, value)| name.eq_ignore_ascii_case("transfer-encoding")
            && value.to_lowercase().contains("chunked"));
    if chunked {
        while !body_complete_chunked(&body) {
            let read = stream.read(&mut chunk).map_err(|error| HttpError::Protocol(error.to_string()))?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..read]);
        }
        body = decode_chunked(&body);
    } else if let Some(length) = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
    {
        while body.len() < length {
            let read = stream.read(&mut chunk).map_err(|error| HttpError::Protocol(error.to_string()))?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..read]);
        }
        body.truncate(length);
    } else {
        loop {
            let read = stream.read(&mut chunk);
            match read {
                Ok(0) => break,
                Ok(count) => body.extend_from_slice(&chunk[..count]),
                Err(_) => break,
            }
        }
    }

    let encoding = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-encoding"))
        .map(|(_, value)| value.to_lowercase())
        .unwrap_or_default();
    let body = if encoding.contains("gzip") {
        decode_gzip(&body)
    } else if encoding.contains("deflate") {
        decode_deflate(&body)
    } else {
        body
    };

    Ok(Response { status, headers, body })
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

fn body_complete_chunked(data: &[u8]) -> bool {
    let text = String::from_utf8_lossy(data);
    text.contains("\r\n0\r\n\r\n") || text.ends_with("0\r\n\r\n")
}

fn decode_chunked(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut rest = data;
    loop {
        let position = match find_subsequence(rest, b"\r\n") {
            Some(value) => value,
            None => break,
        };
        let size_text = String::from_utf8_lossy(&rest[..position]).to_string();
        let size = match usize::from_str_radix(size_text.trim().split(';').next().unwrap_or("0"), 16) {
            Ok(value) => value,
            Err(_) => break,
        };
        if size == 0 {
            break;
        }
        let start = position + 2;
        if rest.len() < start + size {
            break;
        }
        out.extend_from_slice(&rest[start..start + size]);
        rest = &rest[start + size..];
        if rest.starts_with(b"\r\n") {
            rest = &rest[2..];
        }
    }
    out
}

fn decode_gzip(data: &[u8]) -> Vec<u8> {
    use flate2::read::GzDecoder;
    let mut out = Vec::new();
    let mut decoder = GzDecoder::new(data);
    if decoder.read_to_end(&mut out).is_ok() {
        out
    } else {
        data.to_vec()
    }
}

fn decode_deflate(data: &[u8]) -> Vec<u8> {
    use flate2::read::ZlibDecoder;
    let mut out = Vec::new();
    let mut decoder = ZlibDecoder::new(data);
    if decoder.read_to_end(&mut out).is_ok() {
        return out;
    }
    use flate2::read::DeflateDecoder;
    let mut out = Vec::new();
    let mut decoder = DeflateDecoder::new(data);
    if decoder.read_to_end(&mut out).is_ok() {
        out
    } else {
        data.to_vec()
    }
}
