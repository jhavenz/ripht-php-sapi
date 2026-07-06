use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

pub fn artifact_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("artifacts")
        .join(name)
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
