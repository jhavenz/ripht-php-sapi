//! Throughput benchmarks for ripht-php-sapi.
//!
//! Measures raw throughput for various workloads.
//!
//! # Usage
//!
//! ```bash
//! cargo bench --bench throughput
//! ```

mod shared;

use criterion::{
    black_box, criterion_group, criterion_main, Criterion, Throughput,
};
use shared::{Backend, Method, SapiBackend};

fn bench_simple_request_throughput(c: &mut Criterion) {
    shared::worker::maybe_run_worker();

    let mut backend = SapiBackend::new();

    let mut group = c.benchmark_group("throughput");
    group.throughput(Throughput::Elements(1));

    group.bench_function("simple_request", |b| {
        b.iter(|| black_box(backend.execute("hello.php", Method::Get, None)))
    });

    group.finish();
}

fn bench_file_io_throughput(c: &mut Criterion) {
    shared::worker::maybe_run_worker();

    let mut backend = SapiBackend::new();

    let mut group = c.benchmark_group("file_io");
    group.throughput(Throughput::Elements(1));

    group.bench_function("read_write", |b| {
        b.iter(|| black_box(backend.execute("file_io.php", Method::Get, None)))
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_simple_request_throughput,
    bench_file_io_throughput
);

criterion_main!(benches);
