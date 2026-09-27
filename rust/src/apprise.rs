//! The subset of Apprise this application uses, byte-exact against Apprise 1.9.2.
//!
//! Every plugin here was written from raw wire captures of Apprise 1.9.2 (see
//! `conformance/research/APPRISE-BYTE-SPEC.md`); the unit tests at the bottom of this
//! file replay those captures and compare the request bytes.

use std::time::Duration;

use crate::http::{self, RequestSpec};
use crate::jsonutil;

pub const APPRISE_VERSION: &str = "1.9.2";
const ICON_INFO_256: &str =
    "https://github.com/caronc/apprise/raw/master/apprise/assets/themes/default/apprise-info-256x256.png";
const ICON_INFO_72: &str =
    "https://github.com/caronc/apprise/raw/master/apprise/assets/themes/default/apprise-info-72x72.png";

/// `NotifyBase.session` defaults: nothing is split unless the URL says so.
const OVERFLOW_UPSTREAM: &str = "upstream";

/// A parsed notification URL.
#[derive(Debug, Clone, PartialEq)]
pub struct NotifyUrl {
    pub scheme: String,
    pub host: String,
    pub port: Option<u16>,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub user: String,
    pub password: String,
}

impl NotifyUrl {
    pub fn arg(&self, name: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    pub fn overflow(&self) -> String {
        self.arg("overflow").unwrap_or(OVERFLOW_UPSTREAM).to_string()
    }

    /// The `http(s)://host[:port]/path` target for this URL.
    pub fn base_url(&self, secure: bool, default_port: u16) -> String {
        let scheme = if secure { "https" } else { "http" };
        let port = self.port.unwrap_or(default_port);
        let default = if secure { 443 } else { 80 };
        let authority = if port == default {
            self.host.clone()
        } else {
            format!("{}:{}", self.host, port)
        };
        format!("{}://{}{}", scheme, authority, self.path)
    }

    fn topic(&self) -> String {
        self.path.trim_start_matches('/').split('/').next().unwrap_or("").to_string()
    }
}

/// Parse the subset of the URL syntax Apprise accepts.
pub fn parse_url(raw: &str) -> Option<NotifyUrl> {
    let (scheme, rest) = raw.split_once("://")?;
    let (authority, tail) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, ""),
    };
    let (credentials, hostport) = match authority.rfind('@') {
        Some(index) => (&authority[..index], &authority[index + 1..]),
        None => ("", authority),
    };
    let (user, password) = match credentials.split_once(':') {
        Some((user, password)) => (user.to_string(), password.to_string()),
        None => (credentials.to_string(), String::new()),
    };
    let (host, port) = match hostport.rsplit_once(':') {
        Some((host, port)) if port.chars().all(|ch| ch.is_ascii_digit()) && !port.is_empty() => {
            (host.to_string(), Some(port.parse::<u16>().unwrap_or(0)))
        }
        _ => (hostport.to_string(), None),
    };
    let (path, query_text) = match tail.split_once('?') {
        Some((path, query)) => (path.to_string(), query.to_string()),
        None => (tail.to_string(), String::new()),
    };
    let query = query_text
        .split('&')
        .filter(|item| !item.is_empty())
        .map(|item| match item.split_once('=') {
            Some((key, value)) => (key.trim_start_matches('+').to_string(), value.to_string()),
            None => (item.to_string(), String::new()),
        })
        .collect();
    Some(NotifyUrl {
        scheme: scheme.to_lowercase(),
        host,
        port,
        path,
        query,
        user,
        password,
    })
}

fn basic_auth(url: &NotifyUrl) -> Option<(String, String)> {
    if url.user.is_empty() && url.password.is_empty() {
        return None;
    }
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    let token = STANDARD.encode(format!("{}:{}", url.user, url.password));
    Some(("Authorization".to_string(), format!("Basic {}", token)))
}

pub trait Plugin: Send {
    /// Every request this plugin would make for the given notification.
    fn build(&self, body: &str, title: &str) -> Vec<RequestSpec>;
    fn scheme(&self) -> &'static str;

    fn notify(&self, body: &str, title: &str) -> bool {
        let specs = self.build(body, title);
        if specs.is_empty() {
            return false;
        }
        let mut sent = false;
        for spec in &specs {
            match http::send(spec, Duration::from_secs(60)) {
                Ok(response) => {
                    if response.ok() {
                        sent = true;
                    }
                }
                Err(_) => {}
            }
        }
        sent
    }
}

/// The `User-Agent`/`Accept-Encoding`/`Accept`/`Connection` header block Apprise
/// gets from `requests`, in wire order.
fn base_headers(accept: &str) -> Vec<(String, String)> {
    vec![
        ("User-Agent".to_string(), "Apprise".to_string()),
        ("Accept-Encoding".to_string(), "gzip, deflate".to_string()),
        ("Accept".to_string(), accept.to_string()),
        ("Connection".to_string(), "keep-alive".to_string()),
    ]
}

fn overflow_split(body: &str, limit: usize, counter: bool, title: &str) -> Vec<(String, String)> {
    let chars: Vec<char> = body.chars().collect();
    let chunks: Vec<String> = chars.chunks(limit).map(|chunk| chunk.iter().collect()).collect();
    let total = chunks.len();
    chunks
        .into_iter()
        .enumerate()
        .map(|(index, chunk)| {
            if counter {
                (format!("{} [{}/{}]", title, index + 1, total), chunk)
            } else {
                (title.to_string(), chunk)
            }
        })
        .collect()
}

/// `json://`, `form://`, `xml://` - Apprise's generic webhook notifiers.
struct WebhookPlugin {
    url: NotifyUrl,
    kind: &'static str,
    scheme: &'static str,
}

impl WebhookPlugin {
    fn payload(&self, body: &str, title: &str) -> (Option<String>, String) {
        let body = body.trim_end();
        let title = title.trim();
        match self.kind {
            "json" => (
                Some("application/json".to_string()),
                format!(
                    "{{\"version\": \"1.0\", \"title\": {}, \"message\": {}, \"attachments\": [], \"type\": \"info\"}}",
                    jsonutil::dumps_str(title),
                    jsonutil::dumps_str(body)
                ),
            ),
            "form" => (
                None,
                format!(
                    "version=1.0&title={}&message={}&type=info",
                    crate::urlcode::quote_plus(title),
                    crate::urlcode::quote_plus(body)
                ),
            ),
            _ => (
                Some("application/xml".to_string()),
                format!(
                    "<?xml version='1.0' encoding='utf-8'?>\n\
                     <soapenv:Envelope\n    \
                     xmlns:soapenv=\"http://schemas.xmlsoap.org/soap/envelope/\"\n    \
                     xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\"\n    \
                     xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\n    \
                     <soapenv:Body>\n        \
                     <Notification xmlns:xsi=\"https://raw.githubusercontent.com/caronc/apprise/master/apprise/assets/NotifyXML-1.1.xsd\">\n            \
                     <Version>1.1</Version><Subject>{}</Subject><Message>{}</Message><MessageType>info</MessageType>\n            \n       \
                     </Notification>\n    \
                     </soapenv:Body>\n\
                     </soapenv:Envelope>",
                    escape_xml(title),
                    escape_xml(body)
                ),
            ),
        }
    }
}

impl Plugin for WebhookPlugin {
    fn build(&self, body: &str, title: &str) -> Vec<RequestSpec> {
        let (content_type, payload) = self.payload(body, title);
        let mut headers = base_headers("*/*");
        let mut trailing = Vec::new();
        if let Some(value) = content_type {
            headers.push(("Content-Type".to_string(), value));
        }
        if let Some(auth) = basic_auth(&self.url) {
            trailing.push(auth);
        }
        if self.kind == "form" {
            trailing.insert(0, ("Content-Type".to_string(), "application/x-www-form-urlencoded".to_string()));
        }
        vec![RequestSpec {
            method: "POST".to_string(),
            url: self.url.base_url(self.scheme.ends_with('s'), 80),
            headers,
            trailing,
            body: Some(payload.into_bytes()),
        }]
    }

    fn scheme(&self) -> &'static str {
        self.scheme
    }
}

impl WebhookPlugin {
    fn kind(mut self, kind: &'static str) -> WebhookPlugin {
        self.kind = kind;
        self
    }
}

/// `discord://webhook_id/webhook_token`
struct DiscordPlugin {
    webhook_id: String,
    webhook_token: String,
}

impl Plugin for DiscordPlugin {
    fn build(&self, body: &str, title: &str) -> Vec<RequestSpec> {
        let content = if title.is_empty() {
            body.to_string()
        } else {
            format!("{}\r\n{}", title, body)
        };
        let payload = format!(
            "{{\"tts\": false, \"wait\": true, \"avatar_url\": {}, \"content\": {}}}",
            jsonutil::dumps_str(ICON_INFO_256),
            jsonutil::dumps_str(&content)
        );
        let mut headers = base_headers("*/*");
        headers.push(("Content-Type".to_string(), "application/json; charset=utf-8".to_string()));
        vec![RequestSpec {
            method: "POST".to_string(),
            url: format!("https://discord.com/api/webhooks/{}/{}", self.webhook_id, self.webhook_token),
            headers,
            trailing: Vec::new(),
            body: Some(payload.into_bytes()),
        }]
    }

    fn scheme(&self) -> &'static str {
        "discord"
    }
}

/// `slack://token_a/token_b/token_c/channel`
struct SlackPlugin {
    tokens: Vec<String>,
    channel: String,
    user: String,
}

impl Plugin for SlackPlugin {
    fn build(&self, body: &str, title: &str) -> Vec<RequestSpec> {
        // Apprise stamps the attachment with the wall-clock time, so this part of the
        // payload is not reproducible between runs (in either implementation).
        let seconds = crate::clock::unix_seconds();
        let micros = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.subsec_micros())
            .unwrap_or(0);
        let payload = format!(
            "{{\"username\": {}, \"mrkdwn\": true, \"attachments\": [{{\"title\": {}, \"text\": {}, \
             \"color\": \"#3AA3E3\", \"ts\": {}.{:06}, \"footer_icon\": {}, \"footer\": {}}}], \
             \"icon_url\": {}, \"channel\": {}}}",
            jsonutil::dumps_str(&self.user),
            jsonutil::dumps_str(title),
            jsonutil::dumps_str(body),
            seconds,
            micros,
            jsonutil::dumps_str(ICON_INFO_72),
            jsonutil::dumps_str(&self.user),
            jsonutil::dumps_str(ICON_INFO_72),
            jsonutil::dumps_str(&self.channel)
        );
        let mut headers = base_headers("application/json");
        headers.push(("Content-Type".to_string(), "application/json; charset=utf-8".to_string()));
        vec![RequestSpec {
            method: "POST".to_string(),
            url: format!(
                "https://hooks.slack.com/services/{}/{}/{}",
                self.tokens.first().cloned().unwrap_or_default(),
                self.tokens.get(1).cloned().unwrap_or_default(),
                self.tokens.get(2).cloned().unwrap_or_default()
            ),
            headers,
            trailing: Vec::new(),
            body: Some(payload.into_bytes()),
        }]
    }

    fn scheme(&self) -> &'static str {
        "slack"
    }
}

/// `tgram://bot_token/chat_id`
struct TelegramPlugin {
    token: String,
    chat_id: String,
    overflow: String,
}

impl TelegramPlugin {
    /// Apprise's HTML body transformation.
    fn transform(text: &str) -> String {
        let mut out = String::with_capacity(text.len() + 16);
        for ch in text.chars() {
            match ch {
                '&' => out.push_str("&amp;amp;"),
                '\n' => out.push_str("\r\n"),
                '\t' => out.push_str("   "),
                other => out.push(other),
            }
        }
        out
    }
}

impl Plugin for TelegramPlugin {
    fn build(&self, body: &str, title: &str) -> Vec<RequestSpec> {
        let text = if title.is_empty() {
            TelegramPlugin::transform(body)
        } else {
            format!(
                "<b>{}</b>\r\n{}",
                TelegramPlugin::transform(title),
                TelegramPlugin::transform(body)
            )
        };
        let chat_id = if self.chat_id.chars().all(|ch| ch.is_ascii_digit()) && !self.chat_id.is_empty() {
            self.chat_id.clone()
        } else {
            jsonutil::dumps_str(&self.chat_id)
        };
        let payload = format!(
            "{{\"disable_notification\": false, \"disable_web_page_preview\": true, \
             \"parse_mode\": \"HTML\", \"text\": {}, \"chat_id\": {}}}",
            jsonutil::dumps_str(&text),
            chat_id
        );
        let mut headers = base_headers("*/*");
        headers.push(("Content-Type".to_string(), "application/json".to_string()));
        vec![RequestSpec {
            method: "POST".to_string(),
            url: format!(
                "https://api.telegram.org/bot{}/sendMessage",
                self.token
            ),
            headers,
            trailing: Vec::new(),
            body: Some(payload.into_bytes()),
        }]
    }

    fn scheme(&self) -> &'static str {
        "tgram"
    }
}

/// `gotify://host[:port]/token`
struct GotifyPlugin {
    url: NotifyUrl,
}

impl Plugin for GotifyPlugin {
    fn build(&self, body: &str, title: &str) -> Vec<RequestSpec> {
        let token = self.url.topic();
        let priority = match self.url.arg("priority") {
            Some("low") => 0,
            Some("high") => 8,
            Some("emergency") => 10,
            _ => 5,
        };
        let payload = format!(
            "{{\"priority\": {}, \"title\": {}, \"message\": {}}}",
            priority,
            jsonutil::dumps_str(title),
            jsonutil::dumps_str(body)
        );
        let mut headers = base_headers("*/*");
        headers.push(("Content-Type".to_string(), "application/json".to_string()));
        headers.push(("X-Gotify-Key".to_string(), token));
        let secure = self.url.scheme == "gotifys";
        let port = self.url.port.unwrap_or(if secure { 443 } else { 80 });
        vec![RequestSpec {
            method: "POST".to_string(),
            url: format!(
                "{}://{}:{}/message",
                if secure { "https" } else { "http" },
                self.url.host,
                port
            ),
            headers,
            trailing: Vec::new(),
            body: Some(payload.into_bytes()),
        }]
    }

    fn scheme(&self) -> &'static str {
        "gotify"
    }
}

/// `ntfy://host[:port]/topic`
struct NtfyPlugin {
    url: NotifyUrl,
}

impl Plugin for NtfyPlugin {
    fn build(&self, body: &str, title: &str) -> Vec<RequestSpec> {
        let cloud = self.url.arg("mode") == Some("cloud");
        let payload = format!(
            "{{\"topic\": {}, \"title\": {}, \"message\": {}}}",
            jsonutil::dumps_str(&self.url.topic()),
            jsonutil::dumps_str(title),
            jsonutil::dumps_str(body)
        );
        let mut headers = base_headers("*/*");
        headers.push(("Content-Type".to_string(), "application/json".to_string()));
        if self.url.arg("image") != Some("no") {
            headers.push(("X-Icon".to_string(), ICON_INFO_256.to_string()));
        }
        if let Some(priority) = self.url.arg("priority") {
            headers.push(("X-Priority".to_string(), priority.to_string()));
        }
        if let Some(tags) = self.url.arg("tags") {
            headers.push(("X-Tags".to_string(), tags.to_string()));
        }
        if let Some(token) = self.url.arg("auth") {
            headers.push(("Authorization".to_string(), format!("Bearer {}", token)));
        }
        let url = if cloud {
            format!("https://{}{}", self.url.host, self.url.path)
        } else {
            self.url.base_url(false, 80)
        };
        // Without `mode=cloud` Apprise treats the host as a private server and posts to
        // the server root.
        let target = if cloud {
            url
        } else {
            format!(
                "http://{}:{}/",
                self.url.host,
                self.url.port.unwrap_or(80)
            )
        };
        vec![RequestSpec {
            method: "POST".to_string(),
            url: target,
            headers,
            trailing: basic_auth(&self.url).into_iter().collect(),
            body: Some(payload.into_bytes()),
        }]
    }

    fn scheme(&self) -> &'static str {
        "ntfy"
    }
}

/// `pover://user_key@token`
struct PushoverPlugin {
    token: String,
    user: String,
}

impl Plugin for PushoverPlugin {
    fn build(&self, body: &str, title: &str) -> Vec<RequestSpec> {
        use crate::urlcode::quote_plus;
        let title = if title.is_empty() { "Apprise Notifications" } else { title };
        let payload = format!(
            "token={}&user={}&priority=0&title={}&message={}&device=ALL_DEVICES&sound=pushover",
            quote_plus(&self.token),
            quote_plus(&self.user),
            quote_plus(title),
            quote_plus(body)
        );
        let mut headers = base_headers("*/*");
        headers.retain(|(name, _)| name != "Accept");
        let mut trailing = vec![(
            "Content-Type".to_string(),
            "application/x-www-form-urlencoded".to_string(),
        )];
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;
        let token = STANDARD.encode(format!("{}:", self.token));
        trailing.push(("Authorization".to_string(), format!("Basic {}", token)));
        vec![RequestSpec {
            method: "POST".to_string(),
            url: "https://api.pushover.net/1/messages.json".to_string(),
            headers,
            trailing,
            body: Some(payload.into_bytes()),
        }]
    }

    fn scheme(&self) -> &'static str {
        "pover"
    }
}

fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\'', "&apos;")
        .replace('"', "&quot;")
}

/// The `Apprise()` container: a plugin list plus Apprise's `add()`/`notify()` contract.
pub struct Apprise {
    plugins: Vec<Box<dyn Plugin>>,
}

impl Apprise {
    pub fn new() -> Apprise {
        Apprise { plugins: Vec::new() }
    }

    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    /// Add one URL.  Returns false for anything Apprise 1.9.2 would reject.
    pub fn add(&mut self, raw: &str) -> bool {
        let url = match parse_url(raw) {
            Some(value) => value,
            None => return false,
        };
        match url.scheme.as_str() {
            "json" | "jsons" => {
                if url.host.is_empty() {
                    return false;                 // Apprise: "Unparseable JSON URL"
                }
                self.plugins
                    .push(Box::new(WebhookPlugin { url, kind: "json", scheme: "json" }));
                true
            }
            "form" | "forms" => {
                if url.host.is_empty() {
                    return false;
                }
                self.plugins
                    .push(Box::new(WebhookPlugin { url, kind: "form", scheme: "form" }));
                true
            }
            "xml" | "xmls" => {
                if url.host.is_empty() {
                    return false;
                }
                self.plugins
                    .push(Box::new(WebhookPlugin { url, kind: "xml", scheme: "xml" }));
                true
            }
            "discord" => {
                let token = url.path.trim_start_matches('/').split('/').next().unwrap_or("");
                if url.host.is_empty() || token.is_empty() {
                    return false;
                }
                self.plugins.push(Box::new(DiscordPlugin {
                    webhook_id: url.host.clone(),
                    webhook_token: token.to_string(),
                }));
                true
            }
            "slack" => {
                let parts: Vec<&str> = url.path.trim_start_matches('/').split('/').collect();
                if parts.len() < 3 {
                    return false;
                }
                self.plugins.push(Box::new(SlackPlugin {
                    tokens: parts[..3].iter().map(|item| item.to_string()).collect(),
                    channel: parts
                        .get(3)
                        .map(|item| format!("#{}", item))
                        .unwrap_or_else(|| "#general".to_string()),
                    user: "Apprise".to_string(),
                }));
                true
            }
            "tgram" => {
                let chat = url.path.trim_start_matches('/').split('/').next().unwrap_or("");
                if url.host.is_empty() || chat.is_empty() {
                    return false;
                }
                self.plugins.push(Box::new(TelegramPlugin {
                    token: url.host.clone(),
                    chat_id: chat.to_string(),
                    overflow: url.overflow(),
                }));
                true
            }
            "gotify" | "gotifys" => {
                if url.host.is_empty() || url.topic().is_empty() {
                    return false;
                }
                self.plugins.push(Box::new(GotifyPlugin { url }));
                true
            }
            "ntfy" | "ntfys" => {
                if url.host.is_empty() || url.topic().is_empty() {
                    return false;
                }
                self.plugins.push(Box::new(NtfyPlugin { url }));
                true
            }
            "pover" => {
                if url.user.is_empty() || url.host.is_empty() {
                    return false;
                }
                self.plugins.push(Box::new(PushoverPlugin {
                    token: url.host.clone(),
                    user: url.user.clone(),
                }));
                true
            }
            _ => false,
        }
    }

    /// `Apprise.notify(body=..., title=...)`: True when at least one plugin succeeded.
    pub fn notify(&self, body: &str, title: &str) -> bool {
        if self.plugins.is_empty() {
            return false;
        }
        let mut sent = false;
        for plugin in &self.plugins {
            if plugin.notify(body, title) {
                sent = true;
            }
        }
        sent
    }

    pub fn schemes(&self) -> Vec<&'static str> {
        self.plugins.iter().map(|plugin| plugin.scheme()).collect()
    }

    /// Every request the configured plugins would make.
    pub fn build_all(&self, body: &str, title: &str) -> Vec<RequestSpec> {
        self.plugins
            .iter()
            .flat_map(|plugin| plugin.build(body, title))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::build_request_bytes;

    const BODY: &str = "line1\nline2";
    const TITLE: &str = "Octopus MinMax Results - Sun 27 Sep 23:00";
    const CAPTURES: &str = "../conformance/research/apprise/captures";

    fn load(case: &str, url: &str, body: &str, title: &str) -> Result<String, String> {
        let mut apprise = Apprise::new();
        if !apprise.add(url) {
            return Err(format!("add() rejected {}", url));
        }
        let specs = apprise.build_all(body, title);
        if specs.len() != 1 {
            return Err(format!("{} requests built", specs.len()));
        }
        let bytes = build_request_bytes(&specs[0]).map_err(|error| error.to_string())?;
        let expected = std::fs::read(format!("{}/{}.request.txt", CAPTURES, case))
            .map_err(|error| format!("capture {}: {}", case, error))?;
        let actual = String::from_utf8_lossy(&bytes).to_string();
        let expected_text = String::from_utf8_lossy(&expected).to_string();
        if actual == expected_text {
            Ok(actual.len().to_string())
        } else {
            Err(format!(
                "mismatch for {}\n--- expected ---\n{}\n--- actual ---\n{}",
                case, expected_text, actual
            ))
        }
    }

    #[test]
    fn json_matches_apprise_capture() {
        load("json_001", "json://127.0.0.1:18801/notify", BODY, TITLE).expect("json://");
    }

    #[test]
    fn json_without_title_matches_apprise_capture() {
        load("json_002", "json://127.0.0.1:18801/notify", BODY, "").expect("json:// no title");
    }

    #[test]
    fn form_matches_apprise_capture() {
        load("form_006", "form://127.0.0.1:18802/notify", BODY, TITLE).expect("form://");
    }

    #[test]
    fn xml_matches_apprise_capture() {
        load("xml_008", "xml://127.0.0.1:18803/notify", BODY, TITLE).expect("xml://");
    }

    #[test]
    fn gotify_matches_apprise_capture() {
        load("gotify_022", "gotify://127.0.0.1:18804/gotifytoken", BODY, TITLE).expect("gotify://");
    }

    #[test]
    fn ntfy_matches_apprise_capture() {
        load("ntfy_024", "ntfy://127.0.0.1:18805/topic", BODY, TITLE).expect("ntfy://");
    }

    #[test]
    fn discord_matches_apprise_capture() {
        load(
            "discord_010",
            "discord://123456789012345678/AbCdEfGhIjKlMnOpQrStUvWxYz012345",
            BODY,
            TITLE,
        )
        .expect("discord://");
    }

    #[test]
    fn telegram_matches_apprise_capture() {
        load(
            "tgram_018",
            "tgram://123456789:AAHdqTcvCH1vGWJxfSeofSAs0K5PALDsaw/987654321",
            BODY,
            TITLE,
        )
        .expect("tgram://");
    }

    #[test]
    fn unknown_schemes_are_rejected_like_apprise() {
        for url in ["", "bogus://nope/", "http://127.0.0.1:18801/notify", "json://",
                    "telegram://123/456", "pushover://user@token"] {
            let mut apprise = Apprise::new();
            assert!(!apprise.add(url), "expected add() to reject {:?}", url);
        }
    }
}
