use super::header::ResponseHeader;
use super::message::{ExecutionMessage, SyslogLevel};

/// Result of PHP script execution.
///
/// Contains the HTTP status code, response headers, body output,
/// and any PHP errors/warnings/notices logged during execution.
#[must_use]
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    status: u16,
    body: Vec<u8>,
    headers: Vec<ResponseHeader>,
    messages: Vec<ExecutionMessage>,
}

impl ExecutionResult {
    pub fn new(
        status: u16,
        body: Vec<u8>,
        headers: Vec<ResponseHeader>,
        messages: Vec<ExecutionMessage>,
    ) -> Self {
        Self {
            status,
            body,
            headers,
            messages,
        }
    }

    pub fn body(&self) -> Vec<u8> {
        self.body.to_owned()
    }

    pub fn take_body(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.body)
    }

    pub fn body_string(&self) -> String {
        if self.body.is_empty() {
            return String::default();
        }

        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn body_str(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.body)
    }

    pub fn status_code(&self) -> u16 {
        self.status
    }

    pub fn has_errors(&self) -> bool {
        self.messages
            .iter()
            .any(|m| m.is_error())
    }

    pub fn has_message_level(&self, level: SyslogLevel) -> bool {
        self.messages
            .iter()
            .any(|m| m.level == level)
    }

    pub fn errors(&self) -> impl Iterator<Item = &ExecutionMessage> {
        self.messages
            .iter()
            .filter(|m| m.is_error())
    }

    pub fn all_messages(&self) -> impl Iterator<Item = &ExecutionMessage> {
        self.messages.iter()
    }

    pub fn all_headers(&self) -> impl Iterator<Item = &ResponseHeader> {
        self.headers.iter()
    }

    /// Returns the first header value for a given header name, if any.
    pub fn header_val(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|h| {
                h.name()
                    .eq_ignore_ascii_case(name)
            })
            .map(|h| h.value())
    }

    /// Returns all headers for a given header name
    /// e.g. "cookie" -> vec!["SESSID=abc123", "lang=en", "..."]
    pub fn header_vals(&self, name: &str) -> Vec<&str> {
        self.headers
            .iter()
            .filter(|h| {
                h.name()
                    .eq_ignore_ascii_case(name)
            })
            .map(|h| h.value())
            .collect()
    }

    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn is_redirect(&self) -> bool {
        (300..400).contains(&self.status)
    }

    pub fn is_client_error(&self) -> bool {
        (400..500).contains(&self.status)
    }

    pub fn is_server_error(&self) -> bool {
        (500..600).contains(&self.status)
    }
}

impl Default for ExecutionResult {
    fn default() -> Self {
        Self {
            status: 200,
            body: Vec::new(),
            headers: Vec::new(),
            messages: Vec::new(),
        }
    }
}

#[cfg(feature = "http")]
impl ExecutionResult {
    pub fn into_http_response(self) -> http::Response<Vec<u8>> {
        let mut builder = http::Response::builder().status(self.status);

        for h in &self.headers {
            builder = builder.header(h.name(), h.value());
        }

        builder
            .body(self.body)
            .unwrap_or_else(|_| http::Response::new(Vec::new()))
    }
}

#[cfg(feature = "http")]
impl From<ExecutionResult> for http::Response<Vec<u8>> {
    fn from(res: ExecutionResult) -> Self {
        res.into_http_response()
    }
}
