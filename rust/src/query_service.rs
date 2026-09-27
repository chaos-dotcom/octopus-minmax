//! API access, mirroring `src/query_service.py` including its retry behaviour.

use std::sync::Mutex;
use std::time::Duration;

use serde_json::Value;

use crate::errors::{key_error, AppError, AppResult};
use crate::http;
use crate::pyrepr::repr_json;
use crate::queries;
use crate::{logd, loge, logi, logw};

pub const MAX_RETRIES: i64 = 5;
pub const BASE_WAIT_BEFORE_RETRY_SECONDS: u64 = 30;

pub const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/605.1.15 \
                              (KHTML, like Gecko) Chrome/138.0.0.0 Safari/605.1.15";

static SHARED_TOKEN: Mutex<Option<String>> = Mutex::new(None);

/// `http.HTTPStatus(status).phrase`, which is what `requests` puts in its error text.
pub fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        409 => "Conflict",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "",
    }
}

/// `requests.models.Response.raise_for_status()`
pub fn status_error(status: u16, url: &str) -> AppError {
    let kind = if status >= 500 { "Server Error" } else { "Client Error" };
    AppError::new(format!(
        "{} {}: {} for url: {}",
        status,
        kind,
        reason_phrase(status),
        url
    ))
}

/// An exception name and message, the way Python would report it.
struct Raised {
    type_name: &'static str,
    message: String,
}

impl Raised {
    fn from_http(error: http::HttpError) -> Raised {
        match error {
            http::HttpError::Connect(message) => Raised { type_name: "ConnectionError", message },
            http::HttpError::Timeout => Raised {
                type_name: "Timeout",
                message: "HTTPConnectionPool: Read timed out. (read timeout=60)".to_string(),
            },
            http::HttpError::Protocol(message) => Raised { type_name: "ChunkedEncodingError", message },
            http::HttpError::Tls(message) => Raised { type_name: "SSLError", message },
        }
    }
}

pub struct QueryService {
    base_url: String,
    api_key: String,
    /// The headers dict as the Python service builds it (request-specific headers only).
    headers: Vec<(&'static str, String)>,
    graphql_endpoint: String,
}

/// `{"query": "...", "variables": {}}` exactly as `requests`' `json=` encodes it.
fn graphql_payload(query: &str) -> Vec<u8> {
    let body = format!(
        "{{\"query\": {}, \"variables\": {{}}}}",
        crate::jsonutil::dumps_str(query)
    );
    body.into_bytes()
}

fn content_type_is_json(response: &http::Response) -> bool {
    response
        .header("content-type")
        .map(|value| value.starts_with("application/json"))
        .unwrap_or(false)
}

impl QueryService {
    pub fn new(api_key: &str, base_url: &str) -> AppResult<QueryService> {
        logd!("octobot.query_service", "query_service.__init__", "Initialising QueryService");
        let service = QueryService {
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
            headers: vec![
                ("Accept", "application/json, text/plain, */*".to_string()),
                ("Accept-Language", "en-US,en;q=0.9".to_string()),
                ("Content-Type", "application/json".to_string()),
            ],
            graphql_endpoint: format!("{}/graphql/", base_url.trim_end_matches('/')),
        };
        if SHARED_TOKEN.lock().unwrap().is_none() {
            let token = service.get_token()?;
            *SHARED_TOKEN.lock().unwrap() = Some(token);
        }
        Ok(service)
    }

    /// The header list `requests` writes for a call with this service's headers dict:
    /// session defaults first, then the request-specific headers, then Authorization.
    fn wire_headers(&self, with_auth: bool) -> Vec<(String, String)> {
        let mut headers: Vec<(String, String)> = vec![
            ("User-Agent".to_string(), USER_AGENT.to_string()),
            ("Accept-Encoding".to_string(), "gzip, deflate".to_string()),
            ("Accept".to_string(), "application/json, text/plain, */*".to_string()),
            ("Connection".to_string(), "keep-alive".to_string()),
        ];
        for (name, value) in &self.headers {
            if name == &"Accept" {
                continue;                     // already present as a session default
            }
            headers.push((name.to_string(), value.clone()));
        }
        if with_auth {
            if let Some(token) = SHARED_TOKEN.lock().unwrap().clone() {
                headers.push(("Authorization".to_string(), token));
            }
        }
        headers
    }

    fn get_token(&self) -> AppResult<String> {
        logd!("octobot.query_service", "query_service._get_token", "Getting token");
        let formatted = queries::fill(queries::TOKEN_QUERY, &[("api_key", &self.api_key)]);
        let payload = graphql_payload(&formatted);
        let outcome: AppResult<String> = (|| {
            let response = http::request(
                "POST",
                &self.graphql_endpoint,
                &self.wire_headers(false),
                Some(&payload),
                Duration::from_secs(60),
            )
            .map_err(|error| AppError::new(Raised::from_http(error).message))?;
            if response.status >= 400 {
                return Err(status_error(response.status, &self.graphql_endpoint));
            }
            let parsed: Value = response.json()?;
            logd!(
                "octobot.query_service",
                "query_service._get_token",
                "GQL query response: status={} | body={}",
                response.status,
                repr_json(&parsed)
            );
            if let Some(errors) = parsed.get("errors") {
                return Err(AppError::new(format!("GQL errors: {}", repr_json(errors))));
            }
            let token = parsed
                .get("data")
                .and_then(|data| data.get("obtainKrakenToken"))
                .and_then(|node| node.get("token"))
                .and_then(|value| value.as_str())
                .map(|value| value.to_string());
            match token {
                Some(value) if !value.is_empty() => {
                    let prefix: String = value.chars().take(20).collect();
                    logi!(
                        "octobot.query_service",
                        "query_service._get_token",
                        "Acquired token: {}...",
                        prefix
                    );
                    Ok(value)
                }
                _ => Err(AppError::new("GQL token missing from response")),
            }
        })();
        match outcome {
            Ok(token) => Ok(token),
            Err(error) => {
                loge!(
                    "octobot.query_service",
                    "query_service._get_token",
                    "Failed to get token: {} - {}",
                    "Exception",
                    error
                );
                Err(AppError::new("Failed to get token"))
            }
        }
    }

    /// `QueryService.execute_gql_query`
    pub fn execute_gql_query(&self, query: &str) -> AppResult<Value> {
        logd!(
            "octobot.query_service",
            "query_service.execute_gql_query",
            "Executing GQL query: '{}'",
            query
        );
        let mut retry: i64 = 0;
        let mut token_refreshed = false;
        let mut response: Option<http::Response> = None;
        while retry < MAX_RETRIES {
            let payload = graphql_payload(query);
            let mut raised: Option<Raised> = None;
            response = None;
            match http::request(
                "POST",
                &self.graphql_endpoint,
                &self.wire_headers(true),
                Some(&payload),
                Duration::from_secs(60),
            ) {
                Ok(value) => response = Some(value),
                Err(error) => raised = Some(Raised::from_http(error)),
            }

            if let Some(current) = response.clone() {
                match current.json() {
                    Ok(parsed) => {
                        logd!(
                            "octobot.query_service",
                            "query_service.execute_gql_query",
                            "GQL query response: status={} | body={}",
                            current.status,
                            repr_json(&parsed)
                        );
                        if current.ok() {
                            if let Some(errors) = parsed.get("errors") {
                                let codes: Vec<Option<String>> = errors
                                    .as_array()
                                    .map(|items| {
                                        items
                                            .iter()
                                            .map(|item| {
                                                item.get("extensions")
                                                    .and_then(|extensions| extensions.get("errorCode"))
                                                    .and_then(|code| code.as_str())
                                                    .map(|code| code.to_string())
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default();
                                if codes.iter().any(|code| code.as_deref() == Some("KT-CT-1124"))
                                    && !token_refreshed
                                {
                                    logd!(
                                        "octobot.query_service",
                                        "query_service.execute_gql_query",
                                        "JWT expired, refreshing token..."
                                    );
                                    match self.get_token() {
                                        Ok(value) => {
                                            *SHARED_TOKEN.lock().unwrap() = Some(value);
                                            token_refreshed = true;
                                            continue;
                                        }
                                        Err(error) => logw!(
                                            "octobot.query_service",
                                            "query_service.execute_gql_query",
                                            "Failed to refresh token: {}",
                                            error
                                        ),
                                    }
                                }
                                return Err(AppError::new(format!("GQL errors: {}", repr_json(errors))));
                            }
                            let data = parsed.get("data");
                            match data {
                                Some(value)
                                    if value.is_object()
                                        && !value.as_object().unwrap().is_empty() =>
                                {
                                    return Ok(value.clone())
                                }
                                _ => {
                                    return Err(AppError::new("No 'data' returned from GraphQL query"))
                                }
                            }
                        }
                        if (current.status == 401 || current.status == 403) && !token_refreshed {
                            logd!(
                                "octobot.query_service",
                                "query_service.execute_gql_query",
                                "Authentication failed, refreshing token..."
                            );
                            match self.get_token() {
                                Ok(value) => {
                                    *SHARED_TOKEN.lock().unwrap() = Some(value);
                                    token_refreshed = true;
                                    continue;
                                }
                                Err(error) => logw!(
                                    "octobot.query_service",
                                    "query_service.execute_gql_query",
                                    "Failed to refresh token: {}",
                                    error
                                ),
                            }
                        }
                    }
                    Err(_) => {
                        raised = Some(Raised {
                            type_name: "JSONDecodeError",
                            message: "Expecting value: line 1 column 1 (char 0)".to_string(),
                        });
                    }
                }
            }

            if let Some(error) = raised {
                logw!(
                    "octobot.query_service",
                    "query_service.execute_gql_query",
                    "Request exception on attempt {}/{}: {} - {}",
                    retry + 1,
                    MAX_RETRIES,
                    error.type_name,
                    error.message
                );
                if retry == MAX_RETRIES - 1 {
                    return Err(AppError::new(format!(
                        "GQL query failed after {} attempts: {}",
                        MAX_RETRIES, error.message
                    )));
                }
            }

            if retry == MAX_RETRIES - 1 {
                let status = response.as_ref().map(|item| item.status).unwrap_or(0);
                let text = response.as_ref().map(|item| item.text()).unwrap_or_default();
                logw!(
                    "octobot.query_service",
                    "query_service.execute_gql_query",
                    "GQL query failed after {} attempts: {}: {}",
                    MAX_RETRIES,
                    status,
                    text
                );
                return Err(AppError::new(format!(
                    "GQL query failed after {} attempts: {}: {}",
                    MAX_RETRIES, status, text
                )));
            }

            let status = response.as_ref().map(|item| item.status).unwrap_or(0);
            let wait_time = BASE_WAIT_BEFORE_RETRY_SECONDS * 2u64.pow(retry as u32);
            logd!(
                "octobot.query_service",
                "query_service.execute_gql_query",
                "Request failed with status {}. Retrying in {} seconds... (attempt {}/{})",
                status,
                wait_time,
                retry + 1,
                MAX_RETRIES
            );
            retry += 1;
            std::thread::sleep(Duration::from_secs(wait_time));
        }
        Err(AppError::new("GQL query failed"))
    }

    /// `QueryService.execute_rest_query`
    pub fn execute_rest_query(&self, url: &str) -> AppResult<Value> {
        logi!(
            "octobot.query_service",
            "query_service.execute_rest_query",
            "Executing REST query: {}",
            url
        );
        let outcome: AppResult<Value> = (|| {
            let response = http::request(
                "GET",
                url,
                &self.rest_headers(),
                None,
                Duration::from_secs(60),
            )
            .map_err(|error| AppError::new(Raised::from_http(error).message))?;
            let body = if content_type_is_json(&response) {
                match response.json() {
                    Ok(value) => repr_json(&value),
                    Err(error) => error.to_string(),
                }
            } else {
                response.text().chars().take(200).collect::<String>()
            };
            logd!(
                "octobot.query_service",
                "query_service.execute_rest_query",
                "REST query response: status={} | body={}",
                response.status,
                body
            );
            if response.status >= 400 {
                return Err(status_error(response.status, url));
            }
            response.json().map_err(AppError::from)
        })();
        match outcome {
            Ok(value) => Ok(value),
            Err(error) => {
                // `logger.exception` also writes a Python traceback, which this port cannot
                // reproduce; the message line itself is identical.
                loge!(
                    "octobot.query_service",
                    "query_service.execute_rest_query",
                    "Request failed for {}: {} - {}",
                    url,
                    "HTTPError",
                    error
                );
                Err(AppError::new(format!(
                    "ERROR: Request failed for {}: {} - {}",
                    url, "HTTPError", error
                )))
            }
        }
    }

    /// `requests.get` with no custom headers: the session defaults only.
    fn rest_headers(&self) -> Vec<(String, String)> {
        vec![
            ("User-Agent".to_string(), "python-requests/2.32.3".to_string()),
            ("Accept-Encoding".to_string(), "gzip, deflate".to_string()),
            ("Accept".to_string(), "*/*".to_string()),
            ("Connection".to_string(), "keep-alive".to_string()),
        ]
    }

    pub fn reset_shared_token() {
        *SHARED_TOKEN.lock().unwrap() = None;
    }

    pub fn shared_token() -> Option<String> {
        SHARED_TOKEN.lock().unwrap().clone()
    }

    pub fn set_shared_token(value: Option<String>) {
        *SHARED_TOKEN.lock().unwrap() = value;
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// `data["results"]`, raising `KeyError('results')` when it is missing.
    pub fn expect_results(value: &Value) -> AppResult<&Vec<Value>> {
        match value.get("results").and_then(|items| items.as_array()) {
            Some(items) => Ok(items),
            None => Err(key_error("results")),
        }
    }
}

/// Python's `float(value)`, including its error text for unusable values.
pub fn expect_float(value: &Value) -> AppResult<f64> {
    match value {
        Value::Number(number) => number.as_f64().ok_or_else(|| {
            AppError::new(format!(
                "could not convert value to float: {}",
                crate::pyrepr::repr_float(f64::NAN)
            ))
        }),
        Value::String(text) => text.trim().parse::<f64>().map_err(|_| {
            AppError::new(format!(
                "could not convert string to float: {}",
                crate::pyrepr::repr_str(text)
            ))
        }),
        Value::Bool(flag) => Ok(if *flag { 1.0 } else { 0.0 }),
        _ => Err(AppError::new(format!(
            "float() argument must be a string or a real number, not '{}'",
            match value {
                Value::Null => "NoneType",
                Value::Array(_) => "list",
                Value::Object(_) => "dict",
                _ => "object",
            }
        ))),
    }
}
