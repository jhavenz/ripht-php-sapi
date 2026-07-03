//! Benchmark comparing ripht-php-sapi against php-fpm and FrankenPHP.
//!
//! This benchmark automatically enables comparison with other SAPIs.
//! For performance-only testing, use `sapi_performance` instead.
//!
//! # Usage
//!
//! ```bash
//! cargo bench --bench sapi_comparison
//! ```
//!
//! # Environment Variables
//!
//! - `BENCH_WORKERS=N` — Number of pooled workers (default: 4)
//! - `BENCH_FPM_BIN=/path/to/php-fpm` — Path to php-fpm binary (auto-detected if not set)
//! - `BENCH_FRANKENPHP_BIN=/path/to/frankenphp` — Path to FrankenPHP binary (auto-detected if not set)
//! - `BENCH_FPM_ONLY=1` — Benchmark only php-fpm
//! - `BENCH_FRANKENPHP_ONLY=1` — Benchmark only FrankenPHP

mod shared;

use criterion::{
    black_box, criterion_group, criterion_main, Criterion, Throughput,
};
use shared::{
    Backend, BenchSuite, FpmBackend, FrankenPhpBackend, Method, PooledBackend,
    SapiBackend,
};

fn ensure_comparison_mode() {
    std::env::set_var("BENCH_COMPARE", "1");

    // Auto-detect binaries if needed
    if std::env::var("BENCH_FPM_BIN").is_err() {
        if let Ok(output) = std::process::Command::new("which")
            .arg("php-fpm")
            .output()
        {
            if output.status.success() {
                let path_str = String::from_utf8_lossy(&output.stdout);
                let path = path_str.trim();
                if !path.is_empty() {
                    std::env::set_var("BENCH_FPM_BIN", path);
                }
            }
        }
    }

    if std::env::var("BENCH_FRANKENPHP_BIN").is_err() {
        if let Ok(output) = std::process::Command::new("which")
            .arg("frankenphp")
            .output()
        {
            if output.status.success() {
                let path_str = String::from_utf8_lossy(&output.stdout);
                let path = path_str.trim();
                if !path.is_empty() {
                    std::env::set_var("BENCH_FRANKENPHP_BIN", path);
                }
            }
        }
    }
}

const SUITES: &[BenchSuite] = &[
    BenchSuite {
        name: "simple_get",
        script: "hello.php",
        method: Method::Get,
        body: None,
    },
    BenchSuite {
        name: "json_api",
        script: "api.php",
        method: Method::Get,
        body: None,
    },
    BenchSuite {
        name: "post_json",
        script: "post_json.php",
        method: Method::Post,
        body: Some(br#"{"name":"test","value":42}"#),
    },
    BenchSuite {
        name: "large_output",
        script: "large_output.php",
        method: Method::Get,
        body: None,
    },
];

fn run_suite(c: &mut Criterion, suite: &BenchSuite) {
    ensure_comparison_mode();
    shared::worker::maybe_run_worker();

    let mut group = c.benchmark_group(suite.name);
    group.throughput(Throughput::Elements(1));

    if shared::should_run_ripht_sapi() {
        let mut backend = SapiBackend::new();
        group.bench_function(backend.name(), |b| {
            b.iter(|| {
                black_box(backend.execute(
                    suite.script,
                    suite.method,
                    suite.body,
                ))
            })
        });
    }

    if shared::should_run_ripht_sapi() {
        let mut backend = PooledBackend::from_env();
        group.bench_function(backend.name(), |b| {
            b.iter(|| {
                black_box(backend.execute(
                    suite.script,
                    suite.method,
                    suite.body,
                ))
            })
        });
    }

    if shared::should_run_fpm_sapi() {
        if let Some(mut backend) = FpmBackend::start() {
            group.bench_function(backend.name(), |b| {
                b.iter(|| {
                    black_box(backend.execute(
                        suite.script,
                        suite.method,
                        suite.body,
                    ))
                })
            });
        } else {
            eprintln!("Warning: php-fpm benchmark skipped (binary not found or failed to start)");
        }
    }

    if shared::should_run_frankenphp_sapi() {
        if let Some(mut backend) = FrankenPhpBackend::start() {
            group.bench_function(backend.name(), |b| {
                b.iter(|| {
                    black_box(backend.execute(
                        suite.script,
                        suite.method,
                        suite.body,
                    ))
                })
            });
        } else {
            eprintln!("Warning: FrankenPHP benchmark skipped (binary not found or failed to start)");
        }
    }

    group.finish();
}

fn bench_simple_get(c: &mut Criterion) {
    run_suite(c, &SUITES[0]);
}

fn bench_json_api(c: &mut Criterion) {
    run_suite(c, &SUITES[1]);
}

fn bench_post_json(c: &mut Criterion) {
    run_suite(c, &SUITES[2]);
}

fn bench_large_output(c: &mut Criterion) {
    run_suite(c, &SUITES[3]);
}

criterion_group!(
    benches,
    bench_simple_get,
    bench_json_api,
    bench_post_json,
    bench_large_output,
);

criterion_main!(benches);
