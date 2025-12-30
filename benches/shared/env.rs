#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU16, Ordering};

static NEXT_PORT: AtomicU16 = AtomicU16::new(19000);

pub fn next_port() -> u16 {
    NEXT_PORT.fetch_add(1, Ordering::Relaxed)
}

pub fn scripts_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/php_scripts")
}

pub fn workers_from_env() -> usize {
    std::env::var("BENCH_WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4)
}

pub fn should_run_ripht_sapi() -> bool {
    std::env::var("BENCH_FPM_ONLY").is_err()
        && std::env::var("BENCH_FRANKENPHP_ONLY").is_err()
}

pub fn should_run_fpm_sapi() -> bool {
    std::env::var("BENCH_COMPARE").is_ok()
        || std::env::var("BENCH_FPM_ONLY").is_ok()
}

pub fn should_run_frankenphp_sapi() -> bool {
    std::env::var("BENCH_COMPARE").is_ok()
        || std::env::var("BENCH_FRANKENPHP_ONLY").is_ok()
}

pub fn fpm_bin() -> Option<PathBuf> {
    let value = std::env::var("BENCH_FPM_BIN").ok()?;
    let path = PathBuf::from(value);

    if path.exists() {
        Some(path)
    } else {
        eprintln!(
            "Warning: BENCH_FPM_BIN path does not exist: {}",
            path.display()
        );
        None
    }
}

pub fn frankenphp_bin() -> Option<PathBuf> {
    let value = std::env::var("BENCH_FRANKENPHP_BIN").ok()?;
    let path = PathBuf::from(value);

    if path.exists() {
        Some(path)
    } else {
        eprintln!(
            "Warning: BENCH_FRANKENPHP_BIN path does not exist: {}",
            path.display()
        );
        None
    }
}
