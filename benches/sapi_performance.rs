//! Benchmark comparing ripht-php-sapi against php-fpm and FrankenPHP.
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
//! - `BENCH_COMPARE=1` — Enable php-fpm and FrankenPHP comparisons
//! - `BENCH_FPM_BIN=/path/to/php-fpm` — Path to php-fpm binary
//! - `BENCH_FRANKENPHP_BIN=/path/to/frankenphp` — Path to FrankenPHP binary
//! - `BENCH_FPM_ONLY=1` — Benchmark only php-fpm
//! - `BENCH_FRANKENPHP_ONLY=1` — Benchmark only FrankenPHP

mod shared;

use criterion::{
    black_box, criterion_group, criterion_main, Criterion, Throughput,
};
use shared::{
    Backend, BenchSuite, FpmBackend, FrankenPhpBackend, Method, Pool,
    PooledBackend, SapiBackend,
};

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

// Generic
fn run_suite(c: &mut Criterion, suite: &BenchSuite) {
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

fn bench_ipc_echo(c: &mut Criterion) {
    shared::worker::maybe_run_worker();

    let mut pool = Pool::from_env();

    for (name, size) in [("small", 32usize), ("large", 256 * 1024)] {
        let mut group = c.benchmark_group(&format!("ipc_echo_{}", name));
        group.throughput(Throughput::Bytes(size as u64));

        group.bench_function("msgpack_pipe", |b| {
            b.iter(|| black_box(pool.echo(size)))
        });

        group.finish();
    }
}

criterion_group!(
    benches,
    bench_simple_get,
    bench_json_api,
    bench_post_json,
    bench_large_output,
    bench_ipc_echo,
);

criterion_main!(benches);
