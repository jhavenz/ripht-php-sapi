//! Basic throughput benchmarks for ripht-php-sapi requests.
//!
//! Run: `cargo bench --bench throughput`

use std::path::PathBuf;

use criterion::{
    black_box, criterion_group, criterion_main, Criterion, Throughput,
};
use ripht_php_sapi::{RiphtSapi, WebRequest};

criterion_group!(
    benches,
    bench_simple_request,
    bench_json_api,
    bench_post_json,
    bench_file_io,
    bench_throughput,
);

criterion_main!(benches);

fn php_script_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/php_scripts")
        .join(name)
}

fn bench_simple_request(c: &mut Criterion) {
    let sapi = RiphtSapi::instance();
    let script = php_script_path("hello.php");

    c.bench_function("simple_get", |b| {
        b.iter(|| {
            let exec = WebRequest::get()
                .build(&script)
                .expect("build failed");

            black_box(
                sapi.execute(exec)
                    .expect("execution failed"),
            )
        })
    });
}

fn bench_json_api(c: &mut Criterion) {
    let sapi = RiphtSapi::instance();
    let script = php_script_path("api.php");

    c.bench_function("json_api_get", |b| {
        b.iter(|| {
            let exec = WebRequest::get()
                .with_uri("/?action=status")
                .build(&script)
                .expect("build failed");

            black_box(
                sapi.execute(exec)
                    .expect("execution failed"),
            )
        })
    });
}

fn bench_post_json(c: &mut Criterion) {
    let sapi = RiphtSapi::instance();
    let script = php_script_path("post_json.php");
    let body = r#"{"name":"test","value":42}"#;

    c.bench_function("post_json", |b| {
        b.iter(|| {
            let exec = WebRequest::post()
                .with_content_type("application/json")
                .with_body(body.as_bytes().to_vec())
                .build(&script)
                .expect("build failed");

            black_box(
                sapi.execute(exec)
                    .expect("execution failed"),
            )
        })
    });
}

fn bench_file_io(c: &mut Criterion) {
    let sapi = RiphtSapi::instance();
    let script = php_script_path("file_io.php");

    c.bench_function("file_io", |b| {
        b.iter(|| {
            let exec = WebRequest::get()
                .with_uri("/?action=readwrite")
                .build(&script)
                .expect("build failed");

            black_box(
                sapi.execute(exec)
                    .expect("execution failed"),
            )
        })
    });
}

fn bench_throughput(c: &mut Criterion) {
    let sapi = RiphtSapi::instance();
    let script = php_script_path("hello.php");

    let mut group = c.benchmark_group("throughput");
    group.throughput(Throughput::Elements(1));

    group.bench_function("requests_per_second", |b| {
        b.iter(|| {
            let exec = WebRequest::get()
                .build(&script)
                .expect("build failed");

            black_box(
                sapi.execute(exec)
                    .expect("execution failed"),
            )
        })
    });

    group.finish();
}
