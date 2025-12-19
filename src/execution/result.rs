use super::message::{ExecutionMessage, SyslogLevel};

#[must_use]
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub status: u16,
    pub body: Vec<u8>,
    pub headers: Vec<(String, String)>,
    pub messages: Vec<ExecutionMessage>,
}

impl ExecutionResult {
    pub fn body_string(&self) -> String {
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

    pub fn has_warnings(&self) -> bool {
        self.messages
            .iter()
            .any(|m| m.is_error() || m.level == SyslogLevel::Warning)
    }

    pub fn errors(&self) -> impl Iterator<Item = &ExecutionMessage> {
        self.messages
            .iter()
            .filter(|m| m.is_error())
    }

    pub fn all_messages(&self) -> impl Iterator<Item = &ExecutionMessage> {
        self.messages.iter()
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn headers_all(&self, name: &str) -> Vec<&str> {
        self.headers
            .iter()
            .filter(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
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

        for (name, value) in &self.headers {
            builder = builder.header(name.as_str(), value.as_str());
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
