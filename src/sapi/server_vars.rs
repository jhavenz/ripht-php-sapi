use std::ffi::CString;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// CGI/1.1 compliant server variables for PHP request execution.
///
/// Implements meta-variable semantics per [RFC 3875 §4.1](https://datatracker.ietf.org/doc/html/rfc3875#section-4.1).
#[derive(Debug, Clone, Default)]
pub struct ServerVars {
    cookie: Option<String>,
    vars: Vec<(String, String)>,
    content_type: Option<String>,
    query_string: Option<String>,
    request_method: Option<String>,
}

impl ServerVars {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            vars: Vec::with_capacity(cap),
            ..Default::default()
        }
    }

    pub fn set(
        &mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> &mut Self {
        let key = key.into();
        let value = value.into();
        self.track_special(&key, &value);
        self.vars.push((key, value));
        self
    }

    pub fn extend<I, K, V>(&mut self, iter: I) -> &mut Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        for (k, v) in iter {
            self.set(k, v);
        }
        self
    }

    fn track_special(&mut self, key: &str, value: &str) {
        match key {
            "HTTP_COOKIE" => self.cookie = Some(value.to_string()),
            "CONTENT_TYPE" => self.content_type = Some(value.to_string()),
            "QUERY_STRING" => self.query_string = Some(value.to_string()),
            "REQUEST_METHOD" => self.request_method = Some(value.to_string()),
            _ => {}
        }
    }

    pub fn len(&self) -> usize {
        self.vars.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vars.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &(String, String)> {
        self.vars.iter()
    }

    pub fn into_vec(self) -> Vec<(String, String)> {
        self.vars
    }

    pub fn request_method(&mut self, method: &str) -> &mut Self {
        self.set("REQUEST_METHOD", method)
    }

    pub fn request_uri(&mut self, uri: &str) -> &mut Self {
        self.set("REQUEST_URI", uri)
    }

    pub fn query_string(&mut self, qs: &str) -> &mut Self {
        self.set("QUERY_STRING", qs)
    }

    pub fn request_time(&mut self) -> &mut Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();

        self.set("REQUEST_TIME", now.as_secs().to_string())
            .set("REQUEST_TIME_FLOAT", now.as_secs_f64().to_string())
    }

    pub fn script_filename(&mut self, path: &Path) -> &mut Self {
        self.set("SCRIPT_FILENAME", path.to_string_lossy())
    }

    pub fn script_name(&mut self, name: &str) -> &mut Self {
        self.set("SCRIPT_NAME", name)
            .set("PHP_SELF", name)
    }

    pub fn document_root(&mut self, path: &Path) -> &mut Self {
        self.set("DOCUMENT_ROOT", path.to_string_lossy())
    }

    pub fn set_empty_document_root(&mut self) -> &mut Self {
        self.set("DOCUMENT_ROOT", "")
    }

    pub fn path_info(
        &mut self,
        path_info: &str,
        document_root: &Path,
    ) -> &mut Self {
        self.set("PATH_INFO", path_info);
        let translated =
            format!("{}{}", document_root.to_string_lossy(), path_info);
        self.set("PATH_TRANSLATED", translated)
    }

    pub fn path_translated(&mut self, path: &Path) -> &mut Self {
        self.set("PATH_TRANSLATED", path.to_string_lossy())
    }

    /// Sets `SERVER_NAME` meta-variable.
    ///
    /// Per [RFC 3875 §4.1.14](https://datatracker.ietf.org/doc/html/rfc3875#section-4.1.14).
    pub fn server_name(&mut self, name: &str) -> &mut Self {
        self.set("SERVER_NAME", name)
    }

    pub fn server_port(&mut self, port: u16) -> &mut Self {
        self.set("SERVER_PORT", port.to_string())
    }

    pub fn server_addr(&mut self, addr: &str) -> &mut Self {
        self.set("SERVER_ADDR", addr)
    }

    /// Sets `SERVER_PROTOCOL` meta-variable.
    ///
    /// Per [RFC 3875 §4.1.16](https://datatracker.ietf.org/doc/html/rfc3875#section-4.1.16),
    /// format is `protocol/version` (e.g., `HTTP/1.1`).
    pub fn server_protocol(&mut self, proto: &str) -> &mut Self {
        self.set("SERVER_PROTOCOL", proto)
    }

    pub fn server_software(&mut self, software: &str) -> &mut Self {
        self.set("SERVER_SOFTWARE", software)
    }

    /// Sets `GATEWAY_INTERFACE` meta-variable.
    ///
    /// Per [RFC 3875 §4.1.4](https://datatracker.ietf.org/doc/html/rfc3875#section-4.1.4),
    /// this identifies the CGI specification version (e.g., `CGI/1.1`).
    pub fn gateway_interface(&mut self, gi: &str) -> &mut Self {
        self.set("GATEWAY_INTERFACE", gi)
    }

    pub fn remote_addr(&mut self, addr: &str) -> &mut Self {
        self.set("REMOTE_ADDR", addr)
    }

    pub fn remote_port(&mut self, port: u16) -> &mut Self {
        self.set("REMOTE_PORT", port.to_string())
    }

    pub fn https(&mut self, enabled: bool) -> &mut Self {
        if enabled {
            self.set("HTTPS", "on")
                .set("REQUEST_SCHEME", "https")
        } else {
            self.set("REQUEST_SCHEME", "http")
        }
    }

    pub fn http_header(&mut self, name: &str, value: &str) -> &mut Self {
        let upper = name
            .to_uppercase()
            .replace('-', "_");

        match upper.as_str() {
            "CONTENT_TYPE" => self.set("CONTENT_TYPE", value),
            "CONTENT_LENGTH" => self.set("CONTENT_LENGTH", value),
            _ => self.set(format!("HTTP_{}", upper), value),
        }
    }

    pub fn content_type(&mut self, ct: &str) -> &mut Self {
        self.set("CONTENT_TYPE", ct)
    }

    pub fn content_length(&mut self, len: usize) -> &mut Self {
        self.set("CONTENT_LENGTH", len.to_string())
    }

    pub fn cookies(&mut self, cookie_str: &str) -> &mut Self {
        self.set("HTTP_COOKIE", cookie_str)
    }

    pub fn argc(&mut self, count: usize) -> &mut Self {
        self.set("argc", count.to_string())
    }

    pub fn argv(&mut self, args: &str) -> &mut Self {
        self.set("argv", args)
    }

    pub fn pwd(&mut self, path: &Path) -> &mut Self {
        self.set("PWD", path.to_string_lossy())
    }

    /// Creates server variables with CGI/1.1 web defaults.
    ///
    /// Sets:
    /// - `GATEWAY_INTERFACE` to `CGI/1.1` per [RFC 3875 §4.1.4](https://datatracker.ietf.org/doc/html/rfc3875#section-4.1.4)
    /// - `REQUEST_TIME` and `REQUEST_TIME_FLOAT`
    pub fn web_defaults() -> Self {
        let mut vars = Self::with_capacity(24);

        // RFC 3875 §4.1.4: GATEWAY_INTERFACE
        // "The GATEWAY_INTERFACE variable MUST be set to the dialect of CGI
        // being used by the server to communicate with the script."
        vars.gateway_interface("CGI/1.1")
            .request_time();

        vars
    }

    pub fn cli_defaults() -> Self {
        let mut vars = Self::with_capacity(12);

        vars.request_time()
            .set_empty_document_root();

        vars
    }

    pub fn get_content_type(&self) -> Option<&str> {
        self.content_type.as_deref()
    }

    pub fn get_query_string(&self) -> Option<&str> {
        self.query_string.as_deref()
    }

    pub fn get_cookie(&self) -> Option<&str> {
        self.cookie.as_deref()
    }

    pub fn get_request_method(&self) -> Option<&str> {
        self.request_method.as_deref()
    }

    pub fn into_cstring_pairs(self) -> ServerVarsCString {
        let vars: Vec<(CString, CString)> = self
            .vars
            .into_iter()
            .filter_map(|(k, v)| {
                Some((CString::new(k).ok()?, CString::new(v).ok()?))
            })
            .collect();

        let content_type = self
            .content_type
            .and_then(|s| CString::new(s).ok());

        let query_string = self
            .query_string
            .and_then(|s| CString::new(s).ok());

        let cookie = self
            .cookie
            .and_then(|s| CString::new(s).ok());

        let request_method = self
            .request_method
            .and_then(|s| CString::new(s).ok());

        ServerVarsCString {
            vars,
            content_type,
            query_string,
            cookie,
            request_method,
        }
    }
}

pub struct ServerVarsCString {
    pub vars: Vec<(CString, CString)>,
    pub content_type: Option<CString>,
    pub query_string: Option<CString>,
    pub cookie: Option<CString>,
    pub request_method: Option<CString>,
}

impl ServerVarsCString {
    pub fn content_type_ptr(&self) -> *const std::ffi::c_char {
        self.content_type
            .as_ref()
            .map(|c| c.as_ptr())
            .unwrap_or(std::ptr::null())
    }

    pub fn query_string_ptr(&self) -> *mut std::ffi::c_char {
        self.query_string
            .as_ref()
            .map(|c| c.as_ptr() as *mut _)
            .unwrap_or(std::ptr::null_mut())
    }

    pub fn cookie_ptr(&self) -> *mut std::ffi::c_char {
        self.cookie
            .as_ref()
            .map(|c| c.as_ptr() as *mut _)
            .unwrap_or(std::ptr::null_mut())
    }

    pub fn request_method_ptr(&self) -> *const std::ffi::c_char {
        self.request_method
            .as_ref()
            .map(|c| c.as_ptr())
            .unwrap_or(c"GET".as_ptr())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_server_vars_builder_basic() {
        let mut vars = ServerVars::new();

        vars.request_method("POST")
            .request_uri("/test")
            .query_string("foo=bar");

        assert_eq!(vars.len(), 3);
        assert_eq!(vars.get_request_method(), Some("POST"));
        assert_eq!(vars.get_query_string(), Some("foo=bar"));
    }

    #[test]
    fn test_server_vars_tracks_special_vars() {
        let mut vars = ServerVars::new();

        vars.set("CONTENT_TYPE", "application/json")
            .set("HTTP_COOKIE", "session=abc123")
            .set("QUERY_STRING", "id=1")
            .set("REQUEST_METHOD", "GET");

        assert_eq!(vars.get_content_type(), Some("application/json"));
        assert_eq!(vars.get_cookie(), Some("session=abc123"));
        assert_eq!(vars.get_query_string(), Some("id=1"));
        assert_eq!(vars.get_request_method(), Some("GET"));
    }

    #[test]
    fn test_http_header_transformation() {
        let mut vars = ServerVars::new();
        vars.http_header("Content-Type", "text/html")
            .http_header("Content-Length", "100")
            .http_header("X-Custom-Header", "value")
            .http_header("Accept-Encoding", "gzip");

        let vec = vars.into_vec();
        let map: std::collections::HashMap<_, _> = vec.into_iter().collect();

        assert_eq!(map.get("CONTENT_TYPE"), Some(&"text/html".to_string()));
        assert_eq!(map.get("CONTENT_LENGTH"), Some(&"100".to_string()));
        assert_eq!(map.get("HTTP_X_CUSTOM_HEADER"), Some(&"value".to_string()));
        assert_eq!(map.get("HTTP_ACCEPT_ENCODING"), Some(&"gzip".to_string()));
    }

    #[test]
    fn test_web_defaults() {
        let vars = ServerVars::web_defaults();
        let vec = vars.into_vec();
        let map: std::collections::HashMap<_, _> = vec.into_iter().collect();

        assert!(map.contains_key("REQUEST_TIME"));
        assert!(map.contains_key("REQUEST_TIME_FLOAT"));
        assert_eq!(map.get("GATEWAY_INTERFACE"), Some(&"CGI/1.1".to_string()));
    }

    #[test]
    fn test_cli_defaults() {
        let vars = ServerVars::cli_defaults();
        let vec = vars.into_vec();
        let map: std::collections::HashMap<_, _> = vec.into_iter().collect();

        assert!(map.contains_key("REQUEST_TIME"));
        assert_eq!(map.get("DOCUMENT_ROOT"), Some(&"".to_string()));
        assert!(!map.contains_key("GATEWAY_INTERFACE"));
    }

    #[test]
    fn test_into_cstring_pairs() {
        let mut vars = ServerVars::new();

        vars.request_method("POST")
            .content_type("application/json")
            .query_string("test=1")
            .cookies("sid=xyz");

        let cstring_vars = vars.into_cstring_pairs();

        assert!(cstring_vars
            .request_method
            .is_some());

        assert!(cstring_vars
            .content_type
            .is_some());

        assert!(cstring_vars
            .query_string
            .is_some());

        assert!(cstring_vars.cookie.is_some());

        assert_eq!(cstring_vars.vars.len(), 4);
    }

    #[test]
    fn test_https_sets_scheme() {
        let mut vars_https = ServerVars::new();
        vars_https.https(true);

        let mut vars_http = ServerVars::new();
        vars_http.https(false);

        let https_vec = vars_https.into_vec();
        let http_vec = vars_http.into_vec();

        let https_map: std::collections::HashMap<_, _> = https_vec
            .into_iter()
            .collect();
        let http_map: std::collections::HashMap<_, _> =
            http_vec.into_iter().collect();

        assert_eq!(https_map.get("HTTPS"), Some(&"on".to_string()));
        assert_eq!(https_map.get("REQUEST_SCHEME"), Some(&"https".to_string()));
        assert_eq!(http_map.get("REQUEST_SCHEME"), Some(&"http".to_string()));
        assert!(!http_map.contains_key("HTTPS"));
    }

    #[test]
    fn test_path_info_sets_translated() {
        let mut vars = ServerVars::new();
        let doc_root = PathBuf::from("/var/www");
        vars.path_info("/extra/path", &doc_root);

        let vec = vars.into_vec();
        let map: std::collections::HashMap<_, _> = vec.into_iter().collect();

        assert_eq!(map.get("PATH_INFO"), Some(&"/extra/path".to_string()));
        assert_eq!(
            map.get("PATH_TRANSLATED"),
            Some(&"/var/www/extra/path".to_string())
        );
    }

    #[test]
    fn test_script_name_sets_php_self() {
        let mut vars = ServerVars::new();
        vars.script_name("/index.php");

        let vec = vars.into_vec();
        let map: std::collections::HashMap<_, _> = vec.into_iter().collect();

        assert_eq!(map.get("SCRIPT_NAME"), Some(&"/index.php".to_string()));
        assert_eq!(map.get("PHP_SELF"), Some(&"/index.php".to_string()));
    }
}
