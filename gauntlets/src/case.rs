use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GauntletCase {
    pub name: &'static str,
    pub script: &'static str,
    pub method: HttpMethod,
    pub uri: Option<String>,
    pub body: Option<Vec<u8>>,
    pub content_type: Option<&'static str>,
    pub env: Vec<(String, String)>,
}

impl GauntletCase {
    pub fn get(name: &'static str, script: &'static str) -> Self {
        Self {
            name,
            script,
            method: HttpMethod::Get,
            uri: None,
            body: None,
            content_type: None,
            env: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_uri(mut self, uri: impl Into<String>) -> Self {
        self.uri = Some(uri.into());
        self
    }

    #[must_use]
    pub fn with_env(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.env
            .push((key.into(), value.into()));
        self
    }

    pub fn script_path(&self) -> PathBuf {
        scripts_dir().join(self.script)
    }
}

pub fn scripts_dir() -> PathBuf {
    repository_root().join("tests/php_scripts")
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("gauntlets manifest should live below repository root")
        .to_path_buf()
}
