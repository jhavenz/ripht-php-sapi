use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

pub const ARTIFACT_DIR_ENV: &str = "RIPHT_GAUNTLET_ARTIFACT_DIR";

pub fn artifact_path(name: &str) -> PathBuf {
    artifact_dir().join(name)
}

pub fn artifact_report_path(name: &str) -> PathBuf {
    if artifact_dir_override().is_some() {
        return Path::new(ARTIFACT_DIR_ENV).join(name);
    }

    Path::new("gauntlets/artifacts").join(name)
}

fn artifact_dir() -> PathBuf {
    artifact_dir_override()
        .map(PathBuf::from)
        .unwrap_or_else(default_artifact_dir)
}

fn artifact_dir_override() -> Option<OsString> {
    std::env::var_os(ARTIFACT_DIR_ENV).filter(|dir| !dir.is_empty())
}

fn default_artifact_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("artifacts")
}

pub fn write_json_artifact<T: Serialize>(
    path: &Path,
    value: &T,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_vec_pretty(value)?;

    fs::write(path, json)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::artifact_report_path;

    #[test]
    fn default_artifact_report_path_is_repo_relative() {
        assert_eq!(
            artifact_report_path("ripht-smoke.json"),
            PathBuf::from("gauntlets/artifacts/ripht-smoke.json")
        );
    }
}
