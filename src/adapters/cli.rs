use std::path::{Path, PathBuf};

use crate::execution::ExecutionContext;
use crate::sapi::ServerVars;

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

#[derive(Debug, Clone, Default)]
pub struct CliRequest {
    stdin: Vec<u8>,
    argv: Vec<String>,
    working_dir: Option<PathBuf>,
    env_vars: Vec<(String, String)>,
    ini_overrides: Vec<(String, String)>,
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
        })
    }
}
