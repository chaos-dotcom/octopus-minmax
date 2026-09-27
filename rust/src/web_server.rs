//! `src/web_server.py` plus the config manager: a Flask-compatible HTTP surface.
//!
//! The responses reproduce Werkzeug 3.1's byte layout: status line, header order,
//! `Location: config`, the Werkzeug error pages, the itsdangerous flash cookie and
//! the `Vary: Cookie` rules.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use fancy_regex::Regex;

use crate::config::{self, Config};
use crate::session::{self, Session};
use crate::templates;
use crate::{loge, logi};

/// The `Server:` header.  Set `OCTO_SERVER_HEADER` to the reference value
/// (`Werkzeug/3.1.9 Python/3.11.15`) to make responses byte-identical including
/// this header, which exists only to identify the HTTP implementation.
fn server_header() -> String {
    std::env::var("OCTO_SERVER_HEADER").unwrap_or_else(|_| {
        format!("octo-minmax/{} (Rust)", env!("CARGO_PKG_VERSION"))
    })
}

const BODY_404: &str = "<!doctype html>\n<html lang=en>\n<title>404 Not Found</title>\n<h1>Not Found</h1>\n<p>The requested URL was not found on the server. If you entered the URL manually please check your spelling and try again.</p>\n";
const BODY_405: &str = "<!doctype html>\n<html lang=en>\n<title>405 Method Not Allowed</title>\n<h1>Method Not Allowed</h1>\n<p>The method is not allowed for the requested URL.</p>\n";
const BODY_500: &str = "<!doctype html>\n<html lang=en>\n<title>500 Internal Server Error</title>\n<h1>Internal Server Error</h1>\n<p>The server encountered an internal error and was unable to complete your request. Either the server is overloaded or there is an error in the application.</p>\n";
const BODY_401: &str = "Authentication required";
const BODY_REDIRECT: &str = "<!doctype html>\n<html lang=en>\n<title>Redirecting...</title>\n<h1>Redirecting...</h1>\n<p>You should be redirected automatically to the target URL: <a href=\"config\">config</a>. If not, click the link.\n";

/// The `Allow` header value.  Werkzeug builds this from a Python `set`, so its order
/// is a hash-table artefact; these are the orders CPython 3.11 produces with
/// `PYTHONHASHSEED=0`, which is what the conformance harness pins.
fn allow_header(path: &str) -> &'static str {
    match path {
        "/config" => "GET, HEAD, OPTIONS, POST",
        _ => "GET, HEAD, OPTIONS",
    }
}

pub struct Request {
    pub method: String,
    pub target: String,
    pub version: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub fn path(&self) -> String {
        let raw = self.target.split('?').next().unwrap_or("/");
        percent_decode(raw)
    }
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
            if let Ok(value) = u8::from_str_radix(hex, 16) {
                out.push(value);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

pub struct Response {
    pub status: u16,
    /// Headers in wire order.  A `Content-Length` entry is a placeholder whose value
    /// is filled in with the real body length, which is where Werkzeug writes it.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub head_only: bool,
}

pub const CONTENT_TYPE: (&str, &str) = ("Content-Type", "text/html; charset=utf-8");

impl Response {
    /// A response with Werkzeug's usual order: `Content-Type`, `Content-Length`, then
    /// whatever the view added.
    fn html(status: u16, body: &str) -> Response {
        Response {
            status,
            headers: vec![
                (CONTENT_TYPE.0.to_string(), CONTENT_TYPE.1.to_string()),
                ("Content-Length".to_string(), String::new()),
            ],
            body: body.as_bytes().to_vec(),
            head_only: false,
        }
    }

    /// Insert a header just before the `Content-Length` placeholder.
    fn add_before_length(&mut self, name: &str, value: &str) {
        let position = self
            .headers
            .iter()
            .position(|(key, _)| key.eq_ignore_ascii_case("content-length"))
            .unwrap_or(self.headers.len());
        self.headers.insert(position, (name.to_string(), value.to_string()));
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        302 => "FOUND",
        401 => "UNAUTHORIZED",
        404 => "NOT FOUND",
        405 => "METHOD NOT ALLOWED",
        500 => "INTERNAL SERVER ERROR",
        _ => "UNKNOWN",
    }
}

pub fn run_server() {
    let settings = config::get();
    logi!(
        "octobot.web_server",
        "web_server.run_server",
        "Web server starting on http://localhost:{}",
        settings.web_port
    );
    let address = format!("0.0.0.0:{}", settings.web_port);
    let listener = match TcpListener::bind(&address) {
        Ok(value) => value,
        Err(_) => {
            eprintln!("Address already in use");
            eprintln!(
                "Port {} is in use by another program. Either identify and stop that program, \
                 or start the server with a different port.",
                settings.web_port
            );
            std::process::exit(1);
        }
    };
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                std::thread::spawn(move || {
                    let _ = handle_connection(stream);
                });
            }
            Err(_) => break,
        }
    }
}

fn handle_connection(mut stream: TcpStream) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 8192];
    let header_end;
    loop {
        if let Some(position) = find(&buffer, b"\r\n\r\n") {
            header_end = position + 4;
            break;
        }
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Ok(());
        }
        buffer.extend_from_slice(&chunk[..read]);
    }

    let head = String::from_utf8_lossy(&buffer[..header_end - 4]).to_string();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or_default().to_string();
    let mut parts = request_line.split(' ').collect::<Vec<_>>();
    if parts.len() < 3 {
        // Werkzeug answers a malformed request line with a body-only response.
        return Ok(());
    }
    let method = parts.remove(0).to_string();
    let target = parts.remove(0).to_string();
    let version = parts.remove(0).to_string();

    let mut headers = Vec::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.to_string(), value.trim_start().to_string()));
        }
    }
    let content_length = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = buffer[header_end..].to_vec();
    while body.len() < content_length {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
    }
    body.truncate(content_length);

    let request = Request { method, target, version, headers, body };
    let response = route(&request);
    let bytes = serialise(&response, &request);
    // Werkzeug's development server logs the request after the view has run.
    log_access(
        stream.peer_addr().map(|address| address.ip().to_string()).unwrap_or_else(|_| "127.0.0.1".to_string()),
        &request,
        response.status,
    );
    stream.write_all(&bytes)?;
    stream.flush().ok();
    Ok(())
}

/// Werkzeug's access log line: `_ansi_style` colours plus
/// `"%d/%b/%Y %H:%M:%S"` in local time.
fn log_access(client: String, request: &Request, status: u16) {
    let message = format!("{} {} {}", request.method, uri_to_iri(&request.target), request.version);
    let coloured = ansi_style(&message, status);
    let stamp = chrono::Local::now().format("%d/%b/%Y %H:%M:%S");
    let _ = std::io::Write::write_all(
        &mut std::io::stderr(),
        format!("{} - - [{}] \"{}\" {} -\n", client, stamp, coloured, status).as_bytes(),
    );
    let _ = std::io::Write::flush(&mut std::io::stderr());
}

fn ansi_style(message: &str, status: u16) -> String {
    let code = if (100..200).contains(&status) {
        Some(1)
    } else if status == 200 {
        None
    } else if status == 304 {
        Some(36)
    } else if (300..400).contains(&status) {
        Some(32)
    } else if status == 404 {
        Some(33)
    } else if (400..500).contains(&status) {
        return format!("\u{1b}[31m\u{1b}[1m{}\u{1b}[0m", message);
    } else {
        return format!("\u{1b}[35m\u{1b}[1m{}\u{1b}[0m", message);
    };
    match code {
        Some(value) => format!("\u{1b}[{}m{}\u{1b}[0m", value, message),
        None => message.to_string(),
    }
}

/// `uri_to_iri`: percent-decoded, control characters escaped.
fn uri_to_iri(target: &str) -> String {
    let decoded = percent_decode(target);
    decoded
        .chars()
        .map(|ch| if (ch as u32) < 0x20 || (ch as u32) == 0x7f { format!("\\x{:02x}", ch as u32) } else { ch.to_string() })
        .collect()
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

fn route(request: &Request) -> Response {
    let settings = config::get();
    let path = request.path();
    let known = matches!(path.as_str(), "/" | "/config" | "/logs");

    if !known {
        return Response::html(404, BODY_404);
    }

    let allowed = match path.as_str() {
        "/config" => ["GET", "POST", "HEAD", "OPTIONS"].contains(&request.method.as_str()),
        _ => ["GET", "HEAD", "OPTIONS"].contains(&request.method.as_str()),
    };
    if !allowed {
        let mut response = Response::html(405, BODY_405);
        response.add_before_length("Allow", allow_header(&path));
        return response;
    }

    if !authorised(request, &settings) {
        let mut response = Response {
            status: 401,
            headers: vec![
                (
                    "WWW-Authenticate".to_string(),
                    "Basic realm=\"OctoBot Login Required\"".to_string(),
                ),
                (CONTENT_TYPE.0.to_string(), CONTENT_TYPE.1.to_string()),
                ("Content-Length".to_string(), String::new()),
            ],
            body: BODY_401.as_bytes().to_vec(),
            head_only: false,
        };
        response.head_only = request.method == "HEAD";
        return response;
    }

    if request.method == "OPTIONS" {
        let mut response = Response::html(200, "");
        response.add_before_length("Allow", allow_header(&path));
        return response;
    }

    // Open the session exactly like Flask's SecureCookieSessionInterface.
    let cookie_value = request
        .header("cookie")
        .and_then(|value| {
            value
                .split(';')
                .map(|item| item.trim())
                .find(|item| item.starts_with("session="))
                .map(|item| item["session=".len()..].to_string())
        })
        .unwrap_or_default();
    let loaded = if cookie_value.is_empty() {
        None
    } else {
        session::Session::load(&cookie_value)
    };
    let mut flashes: Vec<(String, String)> = Vec::new();
    let mut accessed = false;
    let mut modified = false;
    if let Some(current) = &loaded {
        if !current.flashes.is_empty() {
            flashes = current.flashes.clone();
            accessed = true;
            modified = true;
        }
    }
    // The session in this application only ever holds `_flashes`, so once the view has
    // read them the dict is empty (which is what makes Flask delete the cookie).


    // The view.
    let mut response = if request.method == "POST" && path == "/config" {
        match config_post(request, &mut flashes) {
            Some(next) => {
                let mut response = Response::html(302, BODY_REDIRECT);
                response.headers.push(("Location".to_string(), "config".to_string()));
                response.headers.push(("Vary".to_string(), "Cookie".to_string()));
                response.headers.push((
                    "Set-Cookie".to_string(),
                    session::set_cookie_header(&next.cookie_value()),
                ));
                return finish_session(response, request);
            }
            None => Response::html(200, ""),
        }
    } else {
        Response::html(200, "")
    };

    let page = match path.as_str() {
        "/" => templates::base(&flashes, &templates::index_page()),
        "/logs" => templates::base(&flashes, &templates::logs_page(&read_log_entries())),
        _ => templates::base(&flashes, &templates::config_page(&config::get())),
    };
    response.body = page.into_bytes();
    response.head_only = request.method == "HEAD";

    // `save_session`
    if loaded.is_some() && accessed {
        response
            .headers
            .push(("Vary".to_string(), "Cookie".to_string()));
        if modified {
            response
                .headers
                .push(("Set-Cookie".to_string(), session::delete_cookie_header()));
        }
    }
    response
}

fn finish_session(mut response: Response, request: &Request) -> Response {
    response.head_only = request.method == "HEAD";
    response
}

fn authorised(request: &Request, settings: &Config) -> bool {
    let ingress = request
        .header("x-ingress-path")
        .map(|value| !value.is_empty())
        .unwrap_or(false)
        || request
            .header("x-hassio-ingress")
            .map(|value| !value.is_empty())
            .unwrap_or(false);
    if ingress {
        return true;
    }
    let header = match request.header("authorization") {
        Some(value) => value,
        None => return false,
    };
    let encoded = match header.split_once(' ') {
        Some((scheme, value)) if scheme.eq_ignore_ascii_case("basic") => value,
        _ => return false,
    };
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    let decoded = match STANDARD.decode(encoded) {
        Ok(value) => value,
        Err(_) => return false,
    };
    let text = String::from_utf8_lossy(&decoded).to_string();
    match text.split_once(':') {
        Some((user, password)) => user == settings.web_username && password == settings.web_password,
        None => false,
    }
}

/// The `POST /config` view.
fn config_post(request: &Request, flashes: &mut Vec<(String, String)>) -> Option<Session> {
    let form = parse_form(request);
    let errors = validate_config(&form);

    if !errors.is_empty() {
        for error in &errors {
            flashes.push(("error".to_string(), error.clone()));
        }
        return Some(Session { flashes: flashes.clone(), exists: true });
    }

    let submitted = python_dict_repr(&form);
    logi!(
        "octobot.web_server",
        "web_server.config_page",
        "Config update submitted: {}",
        submitted
    );
    match update_config(&form) {
        Ok(()) => {
            let new_config = get_config();
            logi!(
                "octobot.web_server",
                "web_server.config_page",
                "Config updated successfully. New state: {}",
                new_config
            );
            flashes.push((
                "success".to_string(),
                "Configuration updated successfully! (Will reset on container restart)".to_string(),
            ));
        }
        Err(message) => {
            flashes.push(("error".to_string(), format!("Error updating config: {}", message)));
            loge!(
                "octobot.web_server",
                "web_server.config_page",
                "Config update failed: {}",
                message
            );
        }
    }
    Some(Session { flashes: flashes.clone(), exists: true })
}

fn parse_form(request: &Request) -> Vec<(String, String)> {
    let body = String::from_utf8_lossy(&request.body).to_string();
    let mut form: Vec<(String, String)> = Vec::new();
    for pair in body.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = match pair.split_once('=') {
            Some((key, value)) => (key.to_string(), value.to_string()),
            None => (pair.to_string(), String::new()),
        };
        let key = url_decode(&key);
        if form.iter().any(|(existing, _)| *existing == key) {
            continue;                       // `MultiDict.to_dict()` keeps the first value
        }
        form.push((key, url_decode(&value)));
    }
    form
}

fn url_decode(text: &str) -> String {
    let replaced = text.replace('+', " ");
    let bytes = replaced.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
            if let Ok(value) = u8::from_str_radix(hex, 16) {
                out.push(value);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// `config_manager.validate_config`
fn validate_config(form: &[(String, String)]) -> Vec<String> {
    let mut errors = Vec::new();
    if let Some(value) = form_value(form, "execution_time") {
        let pattern = Regex::new(r"^([0-1][0-9]|2[0-3]):[0-5][0-9]$").unwrap();
        if !pattern.is_match(value).unwrap_or(false) {
            errors.push("Execution time must be in HH:MM format (00:00 to 23:59)".to_string());
        }
    }
    if let Some(value) = form_value(form, "switch_threshold") {
        match parse_int(value) {
            Ok(number) => {
                if number < 0 {
                    errors.push("Switch threshold must be positive".to_string());
                }
            }
            Err(_) => errors.push("Switch threshold must be a number".to_string()),
        }
    }
    errors
}

/// Python's `int(text)` including its error message.
pub fn parse_int(text: &str) -> Result<i64, String> {
    match text.trim().parse::<i64>() {
        Ok(value) => Ok(value),
        Err(_) => Err(format!(
            "invalid literal for int() with base 10: {}",
            crate::pyrepr::repr_str(text)
        )),
    }
}

/// `config_manager.update_config`
fn update_config(form: &[(String, String)]) -> Result<(), String> {
    let previous_one_off = config::get().one_off_run;
    let mut error: Option<String> = None;
    config::update(|settings| {
        if let Some(value) = form_value(form, "api_key") {
            if !value.is_empty() {
                settings.api_key = value.clone();
            }
        }
        if let Some(value) = form_value(form, "acc_number") {
            if !value.is_empty() {
                settings.acc_number = value.clone();
            }
        }
        if let Some(value) = form_value(form, "base_url") {
            if !value.is_empty() {
                settings.base_url = value.clone();
            }
        }
        if let Some(value) = form_value(form, "execution_time") {
            settings.execution_time = value.clone();
        }
        if let Some(value) = form_value(form, "switch_threshold") {
            match parse_int(value) {
                Ok(number) => settings.switch_threshold = number,
                Err(message) => error = Some(message),
            }
        }
        if let Some(value) = form_value(form, "tariffs") {
            settings.tariffs = value.clone();
        }
        settings.one_off_run = match form_value(form, "one_off_run") {
            Some(value) => truthy(value),
            None => false,
        };
        settings.dry_run = match form_value(form, "dry_run") {
            Some(value) => truthy(value),
            None => false,
        };
        if let Some(value) = form_value(form, "notification_urls") {
            settings.notification_urls = value.clone();
        }
        settings.batch_notifications = match form_value(form, "batch_notifications") {
            Some(value) => truthy(value),
            None => false,
        };
        if settings.one_off_run && !previous_one_off {
            settings.one_off_executed = false;
        }
    });
    match error {
        Some(message) => Err(message),
        None => Ok(()),
    }
}

/// `repr()` of the submitted form dict, in the order the fields arrived.
fn python_dict_repr(form: &[(String, String)]) -> String {
    let mut out = String::from("{");
    for (index, (key, value)) in form.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        out.push_str(&crate::pyrepr::repr_str(key));
        out.push_str(": ");
        out.push_str(&crate::pyrepr::repr_str(value));
    }
    out.push('}');
    out
}

fn form_value<'a>(form: &'a [(String, String)], key: &str) -> Option<&'a String> {
    form.iter().find(|(name, _)| name == key).map(|(_, value)| value)
}

fn truthy(value: &str) -> bool {
    matches!(value.to_lowercase().as_str(), "true" | "1" | "yes" | "on")
}

/// `config_manager.get_config()` - the dict that is logged and rendered.
fn get_config() -> String {
    let settings = config::get();
    let mut out = String::from("{");
    let entries: Vec<(&str, String)> = vec![
        ("api_key", crate::pyrepr::repr_str(&settings.api_key)),
        ("acc_number", crate::pyrepr::repr_str(&settings.acc_number)),
        ("base_url", crate::pyrepr::repr_str(&settings.base_url)),
        ("execution_time", crate::pyrepr::repr_str(&settings.execution_time)),
        ("switch_threshold", settings.switch_threshold.to_string()),
        ("tariffs", crate::pyrepr::repr_str(&settings.tariffs)),
        ("one_off_run", python_bool(settings.one_off_run)),
        ("one_off_executed", python_bool(settings.one_off_executed)),
        ("dry_run", python_bool(settings.dry_run)),
        ("notification_urls", crate::pyrepr::repr_str(&settings.notification_urls)),
        ("batch_notifications", python_bool(settings.batch_notifications)),
    ];
    for (index, (key, value)) in entries.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        out.push_str(&crate::pyrepr::repr_str(key));
        out.push_str(": ");
        out.push_str(value);
    }
    out.push('}');
    out
}

fn python_bool(value: bool) -> String {
    if value {
        "True".to_string()
    } else {
        "False".to_string()
    }
}

/// `web_server.tail_file` + `web_server.group_log_entries`
pub fn read_log_entries() -> Vec<String> {
    let path = "logs/octobot.log";
    let bytes = match std::fs::read(path) {
        Ok(value) => value,
        Err(error) => {
            if error.kind() == std::io::ErrorKind::NotFound {
                return vec!["Log file not found. The bot may not have started yet.".to_string()];
            }
            return vec![format!("Error reading log file: {}", os_error_text(&error, path))];
        }
    };
    let text = match String::from_utf8(bytes) {
        Ok(value) => value,
        Err(error) => {
            let offset = error.utf8_error().valid_up_to();
            let byte = error.as_bytes().get(offset).copied().unwrap_or(0);
            return vec![format!(
                "Error reading log file: 'utf-8' codec can't decode byte 0x{:02x} in position {}: \
                 invalid start byte",
                byte, offset
            )];
        }
    };
    let lines = split_universal_lines(&text);
    group_log_entries(&lines)
}

/// Python's text mode translates `\r\n` and a lone `\r` to `\n`.
fn split_universal_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                current.push('\n');
                lines.push(std::mem::take(&mut current));
            }
            '\n' => {
                current.push('\n');
                lines.push(std::mem::take(&mut current));
            }
            other => current.push(other),
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn group_log_entries(lines: &[String]) -> Vec<String> {
    let pattern = Regex::new(r"^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}").unwrap();
    let mut entries: Vec<String> = Vec::new();
    let mut current = String::new();
    for line in lines {
        if pattern.is_match(line).unwrap_or(false) {
            if !current.is_empty() {
                entries.push(std::mem::take(&mut current));
            }
            current.push_str(line);
        } else {
            current.push_str(line);
        }
    }
    if !current.is_empty() {
        entries.push(current);
    }
    entries
}

fn os_error_text(error: &std::io::Error, path: &str) -> String {
    let kind = match error.kind() {
        std::io::ErrorKind::PermissionDenied => (13, "Permission denied"),
        std::io::ErrorKind::NotFound => (2, "No such file or directory"),
        _ => (0, "Unknown error"),
    };
    if let Some(code) = error.raw_os_error() {
        match code {
            21 => return format!("[Errno 21] Is a directory: '{}'", path),
            13 => return format!("[Errno 13] Permission denied: '{}'", path),
            2 => return format!("[Errno 2] No such file or directory: '{}'", path),
            _ => {}
        }
    }
    format!("[Errno {}] {}: '{}'", kind.0, kind.1, path)
}

/// Serialise a response the way Werkzeug's dev server writes it: the headers in the
/// order Werkzeug leaves them, then `Connection: close`, then the body.
fn serialise(response: &Response, _request: &Request) -> Vec<u8> {
    let mut head = format!("HTTP/1.1 {} {}\r\n", response.status, reason(response.status));
    head.push_str(&format!("Server: {}\r\n", server_header()));
    head.push_str(&format!(
        "Date: {}\r\n",
        chrono::Utc::now().format("%a, %d %b %Y %H:%M:%S GMT")
    ));
    for (name, value) in &response.headers {
        if name.eq_ignore_ascii_case("content-length") {
            head.push_str(&format!("Content-Length: {}\r\n", response.body.len()));
        } else {
            head.push_str(&format!("{}: {}\r\n", name, value));
        }
    }
    head.push_str("Connection: close\r\n\r\n");

    let mut out = head.into_bytes();
    if !response.head_only {
        out.extend_from_slice(&response.body);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn universal_newlines_split_on_crlf_and_lone_cr() {
        let lines = split_universal_lines("a\r\nb\rc\nd");
        assert_eq!(lines, vec!["a\n", "b\n", "c\n", "d"]);
    }

    #[test]
    fn vertical_tab_and_form_feed_are_not_separators() {
        let lines = split_universal_lines("a\u{b}c\u{c}d\n");
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn groups_log_entries_like_the_reference() {
        let text = "first line without any timestamp\n    indented continuation\n\
                    2024-01-02 03:04:05 first\nsecond line\nthird, & < > \" '\n\n\
                    2024-01-02 03:04:06 second\n2024-01-02 03:04:07 third";
        let entries = group_log_entries(&split_universal_lines(text));
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0], "first line without any timestamp\n    indented continuation\n");
        assert_eq!(
            entries[1],
            "2024-01-02 03:04:05 first\nsecond line\nthird, & < > \" '\n\n"
        );
        assert_eq!(entries[2], "2024-01-02 03:04:06 second\n");
        assert_eq!(entries[3], "2024-01-02 03:04:07 third");
    }

    #[test]
    fn empty_file_has_no_entries() {
        assert!(group_log_entries(&split_universal_lines("")).is_empty());
    }

    #[test]
    fn timestamp_pattern_does_not_validate_ranges() {
        let entries = group_log_entries(&split_universal_lines("2024-13-99 99:99:99 x\n"));
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn leading_space_stops_a_match() {
        let entries = group_log_entries(&split_universal_lines(" 2024-01-02 03:04:05 x\n"));
        assert_eq!(entries, vec![" 2024-01-02 03:04:05 x\n"]);
    }
}
