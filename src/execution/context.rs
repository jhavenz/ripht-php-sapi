use std::ffi::CString;
use std::fmt;
use std::path::PathBuf;

use crate::sapi::ServerVars;
use crate::ExecutionError;

#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub input: Vec<u8>,
    pub script_path: PathBuf,
    pub server_vars: ServerVars,
    pub env_vars: Vec<(String, String)>,
    pub ini_overrides: Vec<(String, String)>,
}

impl ExecutionContext {
    pub fn script(path: impl Into<PathBuf>) -> Self {
        Self {
            input: Vec::new(),
            script_path: path.into(),
            server_vars: ServerVars::new(),
            env_vars: Vec::new(),
            ini_overrides: Vec::new(),
        }
    }

    pub fn var(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.server_vars.set(key, value);
        self
    }

    pub fn vars<I, K, V>(mut self, iter: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        self.server_vars.extend(iter);
        self
    }

    pub fn input(mut self, bytes: impl Into<Vec<u8>>) -> Self {
        self.input = bytes.into();
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.push((key.into(), value.into()));
        self
    }

    pub fn envs<I, K, V>(mut self, iter: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        self.env_vars
            .extend(iter.into_iter().map(|(k, v)| (k.into(), v.into())));
        self
    }

    pub fn ini(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.ini_overrides.push((key.into(), value.into()));
        self
    }

    pub fn path_as_cstring(&self) -> Result<CString, ExecutionError> {
        let path_str = self.script_path.to_string_lossy();
        CString::new(path_str.as_bytes())
            .map_err(|_| ExecutionError::InvalidPath("Path contains null byte".to_string()))
    }
}

impl fmt::Display for ExecutionContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "ExecutionContext {{")?;
        writeln!(f, "  script: {}", self.script_path.display())?;

        let var_count = self.server_vars.len();
        if var_count == 0 {
            writeln!(f, "  server_vars: []")?;
        } else {
            writeln!(f, "  server_vars: [")?;

            let display_count = var_count.min(15);
            for (key, value) in self.server_vars.iter().take(display_count) {
                let escaped_value = escape_non_utf8(value);
                let truncated = if escaped_value.len() > 60 {
                    format!("{}...", &escaped_value[..57])
                } else {
                    escaped_value
                };
                writeln!(f, "    {} = \"{}\"", key, truncated)?;
            }

            if var_count > display_count {
                writeln!(f, "    ... ({} more)", var_count - display_count)?;
            }
            writeln!(f, "  ]")?;
        }

        writeln!(f, "  input: {} bytes", self.input.len())?;
        write!(f, "}}")
    }
}

fn escape_non_utf8(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_control() && c != '\t' && c != '\n' {
            result.push_str(&format!("\\x{:02x}", c as u32));
        } else {
            result.push(c);
        }
    }
    result
}
