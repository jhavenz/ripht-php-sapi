use std::path::{Path, PathBuf};

use crate::execution::ExecutionContext;
use crate::sapi::ServerVars;

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum WebRequestError {
    MissingMethod,
    InvalidMethod(String),
    ScriptNotFound(PathBuf),
}

impl std::fmt::Display for WebRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingMethod => write!(f, "HTTP method not specified"),
            Self::InvalidMethod(m) => write!(f, "Invalid HTTP method: {}", m),
            Self::ScriptNotFound(path) => {
                write!(f, "Script not found: {}", path.display())
            }
        }
    }
}

impl std::error::Error for WebRequestError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
}

impl TryFrom<&str> for Method {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.to_uppercase().as_str() {
            "GET" => Ok(Method::Get),
            "POST" => Ok(Method::Post),
            "PUT" => Ok(Method::Put),
            "DELETE" => Ok(Method::Delete),
            "PATCH" => Ok(Method::Patch),
            "HEAD" => Ok(Method::Head),
            "OPTIONS" => Ok(Method::Options),
            _ => Err(format!("Invalid HTTP method: {}", value)),
        }
    }
}

impl TryFrom<String> for Method {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Method::try_from(value.as_str())
    }
}

impl std::fmt::Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Method {
    pub fn as_str(&self) -> &'static str {
        match &self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Delete => "DELETE",
            Method::Patch => "PATCH",
            Method::Head => "HEAD",
            Method::Options => "OPTIONS",
        }
    }
}

#[derive(Debug, Clone)]
pub struct WebRequest {
    https: bool,
    body: Vec<u8>,
    server_port: u16,
    remote_port: u16,
    uri: Option<String>,
    remote_addr: String,
    server_addr: String,
    method: Option<Method>,
    server_name: String,
    server_protocol: String,
    path_info: Option<String>,
    headers: Vec<(String, String)>,
    cookies: Vec<(String, String)>,
    document_root: Option<PathBuf>,
    env_vars: Vec<(String, String)>,
    ini_overrides: Vec<(String, String)>,
}

impl Default for WebRequest {
    fn default() -> Self {
        Self {
            uri: None,
            method: None,
            server_name: "localhost".to_string(),
            server_port: 80,
            server_protocol: "HTTP/1.1".to_string(),
            remote_addr: "127.0.0.1".to_string(),
            remote_port: 0,
            server_addr: "127.0.0.1".to_string(),
            https: false,
            headers: Vec::new(),
            cookies: Vec::new(),
            body: Vec::new(),
            document_root: None,
            path_info: None,
            env_vars: Vec::new(),
            ini_overrides: Vec::new(),
        }
    }
}

impl WebRequest {
    #[must_use]
    pub fn new(method: Method) -> Self {
        Self {
            method: Some(method),
            ..Default::default()
        }
    }

    #[must_use]
    pub fn get() -> Self {
        Self::new(Method::Get)
    }

    #[must_use]
    pub fn post() -> Self {
        Self::new(Method::Post)
    }

    #[must_use]
    pub fn put() -> Self {
        Self::new(Method::Put)
    }

    #[must_use]
    pub fn delete() -> Self {
        Self::new(Method::Delete)
    }

    #[must_use]
    pub fn patch() -> Self {
        Self::new(Method::Patch)
    }

    #[must_use]
    pub fn head() -> Self {
        Self::new(Method::Head)
    }

    #[must_use]
    pub fn options() -> Self {
        Self::new(Method::Options)
    }

    #[must_use]
    pub fn with_uri(mut self, uri: impl Into<String>) -> Self {
        self.uri = Some(uri.into());
        self
    }

    #[must_use]
    pub fn with_header(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.headers
            .push((name.into(), value.into()));
        self
    }

    #[must_use]
    pub fn with_headers<I, K, V>(mut self, iter: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        self.headers.extend(
            iter.into_iter()
                .map(|(k, v)| (k.into(), v.into())),
        );
        self
    }

    #[must_use]
    pub fn with_cookie(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.cookies
            .push((name.into(), value.into()));
        self
    }

    #[must_use]
    pub fn with_cookies<I, K, V>(mut self, iter: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        self.cookies.extend(
            iter.into_iter()
                .map(|(k, v)| (k.into(), v.into())),
        );
        self
    }

    #[must_use]
    pub fn with_body(mut self, bytes: impl Into<Vec<u8>>) -> Self {
        self.body = bytes.into();
        self
    }

    #[must_use]
    pub fn with_content_type(self, ct: impl Into<String>) -> Self {
        self.with_header("Content-Type", ct)
    }

    #[must_use]
    pub fn with_raw_cookie_header(
        self,
        cookie_string: impl Into<String>,
    ) -> Self {
        self.with_header("Cookie", cookie_string)
    }

    #[must_use]
    pub fn with_server_name(mut self, name: impl Into<String>) -> Self {
        self.server_name = name.into();
        self
    }

    #[must_use]
    pub fn with_server_port(mut self, port: u16) -> Self {
        self.server_port = port;
        self
    }

    #[must_use]
    pub fn with_server_protocol(mut self, proto: impl Into<String>) -> Self {
        self.server_protocol = proto.into();
        self
    }

    #[must_use]
    pub fn with_remote_addr(mut self, addr: impl Into<String>) -> Self {
        self.remote_addr = addr.into();
        self
    }

    #[must_use]
    pub fn with_remote_port(mut self, port: u16) -> Self {
        self.remote_port = port;
        self
    }

    #[must_use]
    pub fn with_server_addr(mut self, addr: impl Into<String>) -> Self {
        self.server_addr = addr.into();
        self
    }

    #[must_use]
    pub fn with_https(mut self, enabled: bool) -> Self {
        self.https = enabled;
        self
    }

    #[must_use]
    pub fn with_document_root(mut self, path: impl Into<PathBuf>) -> Self {
        self.document_root = Some(path.into());
        self
    }

    #[must_use]
    pub fn with_path_info(mut self, path: impl Into<String>) -> Self {
        self.path_info = Some(path.into());
        self
    }

    #[must_use]
    pub fn with_env(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.env_vars
            .push((key.into(), value.into()));

        self
    }

    #[must_use]
    pub fn with_envs<I, K, V>(mut self, iter: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        self.env_vars.extend(
            iter.into_iter()
                .map(|(k, v)| (k.into(), v.into())),
        );

        self
    }

    #[must_use]
    pub fn with_ini(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.ini_overrides
            .push((key.into(), value.into()));

        self
    }

    #[must_use]
    pub fn with_ini_overrides<I, K, V>(mut self, iter: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        self.ini_overrides.extend(
            iter.into_iter()
                .map(|(k, v)| (k.into(), v.into())),
        );

        self
    }

    pub fn build(
        self,
        script_path: impl AsRef<Path>,
    ) -> Result<ExecutionContext, WebRequestError> {
        let method = self
            .method
            .ok_or(WebRequestError::MissingMethod)?;

        let script_path = script_path
            .as_ref()
            .to_path_buf();

        if !script_path.exists() {
            return Err(WebRequestError::ScriptNotFound(script_path));
        }

        let script_filename = std::fs::canonicalize(&script_path)
            .unwrap_or_else(|_| script_path.clone());

        let uri = self.uri.unwrap_or_else(|| {
            format!(
                "/{}",
                script_filename
                    .file_name()
                    .map(|s| s
                        .to_string_lossy()
                        .into_owned())
                    .unwrap_or_default()
            )
        });

        let document_root = self
            .document_root
            .unwrap_or_else(|| {
                script_filename
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("/"))
            });

        let (path, query_string) = parse_uri(&uri);

        let mut vars = ServerVars::web_defaults();

        vars.request_method(method.as_str())
            .request_uri(&uri)
            .query_string(&query_string.unwrap_or_default())
            .script_filename(&script_filename)
            .script_name(&path)
            .document_root(&document_root)
            .server_name(&self.server_name)
            .server_port(self.server_port)
            .server_addr(&self.server_addr)
            .remote_addr(&self.remote_addr)
            .remote_port(self.remote_port)
            .server_protocol(&self.server_protocol)
            .https(self.https);

        if let Some(ref path_info) = self.path_info {
            vars.path_info(path_info, &document_root);
        }

        if !self.cookies.is_empty() {
            let cookie_str = self
                .cookies
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("; ");

            vars.cookies(&cookie_str);
        }

        let mut has_content_type = false;
        let mut has_content_length = false;

        for (name, value) in &self.headers {
            if name.eq_ignore_ascii_case("Content-Type") {
                has_content_type = true;
            } else if name.eq_ignore_ascii_case("Content-Length") {
                has_content_length = true;
            }
            vars.http_header(name, value);
        }

        if !has_content_type && !self.body.is_empty() {
            vars.content_type("application/octet-stream");
        }

        if !has_content_length && !self.body.is_empty() {
            vars.content_length(self.body.len());
        }

        Ok(ExecutionContext {
            script_path,
            server_vars: vars,
            input: self.body,
            env_vars: self.env_vars,
            ini_overrides: self.ini_overrides,
        })
    }
}

fn parse_uri(uri: &str) -> (String, Option<String>) {
    match uri.find('?') {
        Some(pos) => (uri[..pos].to_string(), Some(uri[pos + 1..].to_string())),
        None => (uri.to_string(), None),
    }
}

#[cfg(feature = "http")]
mod http_compat {
    use super::*;

    pub fn from_http_request<B: AsRef<[u8]>>(
        req: http::Request<B>,
        script_path: impl AsRef<Path>,
    ) -> Result<ExecutionContext, WebRequestError> {
        let (parts, body) = req.into_parts();
        from_http_parts(parts, body.as_ref().to_vec(), script_path)
    }

    pub fn from_http_parts(
        parts: http::request::Parts,
        body: Vec<u8>,
        script_path: impl AsRef<Path>,
    ) -> Result<ExecutionContext, WebRequestError> {
        let method = Method::try_from(parts.method.as_str()).map_err(|_| {
            WebRequestError::InvalidMethod(parts.method.to_string())
        })?;

        let mut builder =
            WebRequest::new(method).with_uri(parts.uri.to_string());

        for (name, value) in parts.headers.iter() {
            if let Ok(value_str) = value.to_str() {
                builder = builder.with_header(name.as_str(), value_str);
            }
        }

        builder = builder.with_body(body);

        builder.build(script_path)
    }
}

#[cfg(feature = "http")]
pub use http_compat::{from_http_parts, from_http_request};
