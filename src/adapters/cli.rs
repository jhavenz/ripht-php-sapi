//! CLI request builder for command-line style execution.
//!
//! Provides minimal `$_SERVER` variables (no HTTP context). Use for
//! scripts that expect CLI-style invocation.

use std::path::{Path, PathBuf};

use crate::execution::ExecutionContext;
use crate::sapi::ServerVars;

#[cfg(feature = "tracing")]
use tracing::debug;

/// Errors from building a CLI request.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum CliRequestError {
    ScriptNotFound(PathBuf),
}

impl std::error::Error for CliRequestError {}

impl std::fmt::Display for CliRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ScriptNotFound(path) => {
                write!(f, "Script not found: {}", path.display())
            }
        }
    }
}

/// Builder for CLI-style PHP requests.
///
/// Configure arguments, stdin, environment, and INI overrides.
#[derive(Debug, Clone)]
pub struct CliRequest {
    stdin: Vec<u8>,
    argv: Vec<String>,
    working_dir: Option<PathBuf>,
    env_vars: Vec<(String, String)>,
    ini_overrides: Vec<(String, String)>,
}

impl Default for CliRequest {
    fn default() -> Self {
        Self {
            stdin: Default::default(),
            argv: Default::default(),
            working_dir: Default::default(),
            env_vars: Default::default(),
            ini_overrides: vec![
                ("html_errors".to_string(), "0".to_string()),
                ("display_errors".to_string(), "1".to_string()),
                ("implicit_flush".to_string(), "1".to_string()),
                ("max_input_time".to_string(), "-1".to_string()),
                ("output_buffering".to_string(), "0".to_string()),
                ("max_execution_time".to_string(), "0".to_string()),
            ],
        }
    }
}

impl CliRequest {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_arg(mut self, s: impl Into<String>) -> Self {
        self.argv.push(s.into());
        self
    }

    #[must_use]
    pub fn with_args<I, S>(mut self, iter: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.argv.extend(
            iter.into_iter()
                .map(|s| s.into()),
        );

        self
    }

    #[must_use]
    pub fn with_stdin(mut self, bytes: impl Into<Vec<u8>>) -> Self {
        self.stdin = bytes.into();
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

    #[must_use]
    pub fn with_working_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.working_dir = Some(path.into());

        self
    }

    pub fn build(
        self,
        script_path: impl AsRef<Path>,
    ) -> Result<ExecutionContext, CliRequestError> {
        let script_path = script_path
            .as_ref()
            .to_path_buf();

        #[cfg(feature = "tracing")]
        debug!(
            script_path = %script_path.display(),
            args = ?self.argv,
            "Building CLI request"
        );

        if !script_path.exists() {
            return Err(CliRequestError::ScriptNotFound(script_path));
        }

        let script_filename = std::fs::canonicalize(&script_path)
            .unwrap_or_else(|_| script_path.clone());

        let script_name = script_filename
            .file_name()
            .map(|s| {
                s.to_string_lossy()
                    .into_owned()
            })
            .unwrap_or_default();

        let argc = self.argv.len() + 1;

        let argv_str = std::iter::once(script_name.clone())
            .chain(self.argv.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ");

        let mut vars = ServerVars::cli_defaults();

        vars.script_filename(&script_filename)
            .script_name(&script_name)
            .path_translated(&script_filename)
            .argc(argc)
            .argv(&argv_str);

        if let Some(ref wd) = self.working_dir {
            vars.pwd(wd);
        }

        Ok(ExecutionContext {
            script_path,
            server_vars: vars,
            input: self.stdin,
            env_vars: self.env_vars,
            ini_overrides: self.ini_overrides,
            log_to_stderr: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn php_script_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/php_scripts")
            .join(name)
    }

    #[test]
    fn test_cli_request_sets_log_to_stderr() {
        let script_path = php_script_path("hello.php");

        let ctx = CliRequest::new()
            .build(&script_path)
            .expect("failed to build CLI request");

        assert!(
            ctx.log_to_stderr,
            "CLI request should set log_to_stderr to true"
        );
    }

    #[test]
    fn test_cli_execution_captures_messages() {
        use crate::RiphtSapi;

        let sapi = RiphtSapi::instance();
        let script_path = php_script_path("error_log_test.php");

        let ctx = CliRequest::new()
            .build(&script_path)
            .expect("failed to build CLI request");

        let result = sapi
            .execute(ctx)
            .expect("execution should succeed");

        assert!(
            !result.messages.is_empty(),
            "CLI execution should capture error_log messages"
        );

        assert!(
            result
                .messages
                .iter()
                .any(|m| m
                    .message
                    .contains("Test error log message")),
            "Should contain the error_log message"
        );
    }
}
