//! Generic HTTP client (ureq). TLS verification remains enabled by default.
//!
//! Sensitive headers (Authorization, Cookie, …) are redacted in diagnostics.
//! HTTP status codes are not treated as transport failures.

use std::collections::BTreeMap;
use std::time::Duration;

const SENSITIVE_HEADERS: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "cookie",
    "set-cookie",
];

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: i64,
    pub headers: BTreeMap<String, String>,
    pub body: String,
}

#[derive(Debug)]
pub enum HttpError {
    Transport(String),
    Timeout(String),
    Tls(String),
    BodyTooLarge { limit: u64 },
    Other(String),
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(m) => write!(f, "HTTP transport failure: {m}"),
            Self::Timeout(m) => write!(f, "HTTP timeout: {m}"),
            Self::Tls(m) => write!(f, "HTTP TLS failure: {m}"),
            Self::BodyTooLarge { limit } => {
                write!(f, "HTTP response body exceeds limit ({limit} bytes)")
            }
            Self::Other(m) => write!(f, "HTTP error: {m}"),
        }
    }
}

impl std::error::Error for HttpError {}

pub fn redact_header_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    SENSITIVE_HEADERS.iter().any(|h| *h == lower)
}

pub fn request(
    method: &str,
    url: &str,
    headers: &BTreeMap<String, String>,
    body: Option<&str>,
    timeout_ms: u64,
    max_body_bytes: u64,
) -> Result<HttpResponse, HttpError> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_millis(timeout_ms.max(1)))
        .timeout_read(Duration::from_millis(timeout_ms.max(1)))
        .build();

    let mut req = match method.to_ascii_uppercase().as_str() {
        "GET" => agent.get(url),
        "POST" => agent.post(url),
        "PUT" => agent.put(url),
        "PATCH" => agent.request("PATCH", url),
        "DELETE" => agent.delete(url),
        other => return Err(HttpError::Other(format!("unsupported HTTP method: {other}"))),
    };

    for (k, v) in headers {
        req = req.set(k, v);
    }

    let result = if let Some(b) = body {
        req.send_string(b)
    } else {
        req.call()
    };

    match result {
        Ok(resp) => read_response(resp, max_body_bytes),
        Err(ureq::Error::Status(code, resp)) => {
            // Non-2xx is still a successful HTTP exchange — return status + body.
            let mut out = read_response(resp, max_body_bytes)?;
            out.status = code as i64;
            Ok(out)
        }
        Err(ureq::Error::Transport(t)) => {
            let msg = t.to_string();
            let lower = msg.to_ascii_lowercase();
            if lower.contains("timed out") || lower.contains("timeout") {
                Err(HttpError::Timeout(msg))
            } else if lower.contains("tls") || lower.contains("ssl") || lower.contains("certificate")
            {
                Err(HttpError::Tls(msg))
            } else {
                Err(HttpError::Transport(msg))
            }
        }
    }
}

fn read_response(resp: ureq::Response, max_body_bytes: u64) -> Result<HttpResponse, HttpError> {
    let status = resp.status() as i64;
    let mut headers = BTreeMap::new();
    for name in resp.headers_names() {
        if let Some(v) = resp.header(&name) {
            let display = if redact_header_name(&name) {
                "<redacted>".to_string()
            } else {
                v.to_string()
            };
            headers.insert(name, display);
        }
    }
    let mut buf = Vec::new();
    let mut reader = resp.into_reader();
    let mut chunk = [0u8; 8192];
    loop {
        use std::io::Read;
        let n = reader
            .read(&mut chunk)
            .map_err(|e| HttpError::Transport(e.to_string()))?;
        if n == 0 {
            break;
        }
        if (buf.len() + n) as u64 > max_body_bytes {
            return Err(HttpError::BodyTooLarge {
                limit: max_body_bytes,
            });
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let body = String::from_utf8_lossy(&buf).into_owned();
    Ok(HttpResponse {
        status,
        headers,
        body,
    })
}
