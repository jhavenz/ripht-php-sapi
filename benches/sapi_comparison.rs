//! Benchmarks comparing ripht-php-sapi performance against php-fpm and FrankenPHP.
//! Installing gnuplot is optional.
//!
//! Run: `cargo bench --bench sapi_comparison`
//!
//! Env Var Options:
//! - `BENCH_COMPARE=1` - Enable php-fpm and FrankenPHP comparisons
//! - `BENCH_FPM_BIN=/path/to/php-fpm` - Path to php-fpm binary
//! - `BENCH_FRANKENPHP_BIN=/path/to/frankenphp` - Path to frankenphp binary
//! - `BENCH_FPM_ONLY=1` - Benchmark only php-fpm
//! - `BENCH_FRANKENPHP_ONLY=1` - Benchmark only FrankenPHP

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;

use criterion::{
    black_box, criterion_group, criterion_main, Criterion, Throughput,
};
use ripht_php_sapi::{RiphtSapi, WebRequest};
#[path = "support.rs"]
mod support;

static NEXT_PORT: AtomicU16 = AtomicU16::new(19000);

fn bench_hello_world(c: &mut Criterion) {
    // If running in worker mode, execute worker loop and exit this process.
    support::worker::maybe_run_worker();
    let compare = should_compare();
    let fpm_mode = fpm_only();
    let franken_mode = frankenphp_only();

    let mut group = c.benchmark_group("hello_world");
    group.throughput(Throughput::Elements(1));

    if !fpm_mode && !franken_mode {
        let sapi = RiphtSapi::instance();
        let script = php_script_path("hello.php");

        let exec = WebRequest::get()
            .build(&script)
            .expect("build failed");
        group.bench_function("rust_sapi", |b| {
            b.iter(|| {
                let ctx = black_box(exec.clone());
                black_box(
                    sapi.execute(ctx)
                        .expect("execution failed"),
                )
            })
        });
    }

    if compare || fpm_mode {
        if let Some(fpm) = FpmServer::start() {
            group.bench_function("php_fpm", |b| {
                b.iter(|| black_box(fpm.execute("hello.php", "GET", None)))
            });
        } else {
            eprintln!("Skipping php-fpm benchmark (server failed to start)");
        }
    }

    if compare || franken_mode {
        if let Some(franken) = FrankenPhpServer::start() {
            group.bench_function("frankenphp", |b| {
                b.iter(|| black_box(franken.execute("hello.php", "GET", None)))
            });
        } else {
            eprintln!("Skipping FrankenPHP benchmark (server failed to start)");
        }
    }

    group.finish();

    // Pooled (process) benchmark — minimal production-like worker pool
    let mut group_pool = c.benchmark_group("hello_world_pooled");
    group_pool.throughput(Throughput::Elements(1));
    {
        let workers = support::workers_from_env();
        let mut pool = support::Pool::new(workers);
        let req = support::Exec {
            method: 0, // GET
            script: php_script_path("hello.php")
                .to_string_lossy()
                .into_owned(),
            uri: None,
            content_type: None,
            body: None,
        };
        group_pool.bench_function("rust_sapi_pooled", |b| {
            b.iter(|| {
                let meta = pool.exec_request(&req);
                criterion::black_box(meta.body_len);
            })
        });
    }
    group_pool.finish();
}

fn bench_json_api(c: &mut Criterion) {
    support::worker::maybe_run_worker();
    let compare = should_compare();
    let fpm_mode = fpm_only();
    let franken_mode = frankenphp_only();

    let mut group = c.benchmark_group("json_api");
    group.throughput(Throughput::Elements(1));

    if !fpm_mode && !franken_mode {
        let sapi = RiphtSapi::instance();
        let script = php_script_path("api.php");

        let exec = WebRequest::get()
            .build(&script)
            .expect("build failed");
        group.bench_function("rust_sapi", |b| {
            b.iter(|| {
                let ctx = black_box(exec.clone());
                black_box(
                    sapi.execute(ctx)
                        .expect("execution failed"),
                )
            })
        });
    }

    if compare || fpm_mode {
        if let Some(fpm) = FpmServer::start() {
            group.bench_function("php_fpm", |b| {
                b.iter(|| black_box(fpm.execute("api.php", "GET", None)))
            });
        }
    }

    if compare || franken_mode {
        if let Some(franken) = FrankenPhpServer::start() {
            group.bench_function("frankenphp", |b| {
                b.iter(|| black_box(franken.execute("api.php", "GET", None)))
            });
        }
    }

    group.finish();

    let mut group_pool = c.benchmark_group("json_api_pooled");
    group_pool.throughput(Throughput::Elements(1));
    {
        let workers = support::workers_from_env();
        let mut pool = support::Pool::new(workers);
        let req = support::Exec {
            method: 0, // GET
            script: php_script_path("api.php")
                .to_string_lossy()
                .into_owned(),
            uri: None,
            content_type: None,
            body: None,
        };
        group_pool.bench_function("rust_sapi_pooled", |b| {
            b.iter(|| {
                let meta = pool.exec_request(&req);
                criterion::black_box(meta.body_len);
            })
        });
    }
    group_pool.finish();
}

fn bench_post_json(c: &mut Criterion) {
    support::worker::maybe_run_worker();
    let compare = should_compare();
    let fpm_mode = fpm_only();
    let franken_mode = frankenphp_only();

    let mut group = c.benchmark_group("post_json");
    group.throughput(Throughput::Elements(1));

    let body = br#"{"name":"test","value":42}"#;

    if !fpm_mode && !franken_mode {
        let sapi = RiphtSapi::instance();
        let script = php_script_path("post_json.php");

        let exec = WebRequest::post()
            .with_content_type("application/json")
            .with_body(body.to_vec())
            .build(&script)
            .expect("build failed");
        group.bench_function("rust_sapi", |b| {
            b.iter(|| {
                let ctx = black_box(exec.clone());
                black_box(
                    sapi.execute(ctx)
                        .expect("execution failed"),
                )
            })
        });
    }

    if compare || fpm_mode {
        if let Some(fpm) = FpmServer::start() {
            group.bench_function("php_fpm", |b| {
                b.iter(|| {
                    black_box(fpm.execute("post_json.php", "POST", Some(body)))
                })
            });
        }
    }

    if compare || franken_mode {
        if let Some(franken) = FrankenPhpServer::start() {
            group.bench_function("frankenphp", |b| {
                b.iter(|| {
                    black_box(franken.execute(
                        "post_json.php",
                        "POST",
                        Some(body),
                    ))
                })
            });
        }
    }

    group.finish();
}

fn bench_large_output(c: &mut Criterion) {
    support::worker::maybe_run_worker();
    let compare = should_compare();
    let fpm_mode = fpm_only();
    let franken_mode = frankenphp_only();

    let mut group = c.benchmark_group("large_output");
    group.throughput(Throughput::Elements(1));

    if !fpm_mode && !franken_mode {
        let sapi = RiphtSapi::instance();
        let script = php_script_path("large_output.php");

        let exec = WebRequest::get()
            .with_uri("/?size=10000")
            .build(&script)
            .expect("build failed");
        group.bench_function("rust_sapi", |b| {
            b.iter(|| {
                let ctx = black_box(exec.clone());
                black_box(
                    sapi.execute(ctx)
                        .expect("execution failed"),
                )
            })
        });
    }

    if compare || fpm_mode {
        if let Some(fpm) = FpmServer::start() {
            group.bench_function("php_fpm", |b| {
                b.iter(|| {
                    black_box(fpm.execute(
                        "large_output.php?size=10000",
                        "GET",
                        None,
                    ))
                })
            });
        }
    }

    if compare || franken_mode {
        if let Some(franken) = FrankenPhpServer::start() {
            group.bench_function("frankenphp", |b| {
                b.iter(|| {
                    black_box(franken.execute(
                        "large_output.php?size=10000",
                        "GET",
                        None,
                    ))
                })
            });
        }
    }

    group.finish();
}

fn bench_ipc_echo(c: &mut Criterion) {
    support::worker::maybe_run_worker();

    let workers = support::workers_from_env();
    let mut pool = support::Pool::new(workers);

    let mut small = c.benchmark_group("ipc_echo_small");
    small.throughput(Throughput::Bytes(32));
    small.bench_function("pipe_binary", |b| {
        b.iter(|| {
            let n = pool.echo(32);
            criterion::black_box(n);
        })
    });
    small.finish();

    let mut large = c.benchmark_group("ipc_echo_large");
    let size = 256 * 1024;
    large.throughput(Throughput::Bytes(size as u64));
    large.bench_function("pipe_binary", |b| {
        b.iter(|| {
            let n = pool.echo(size);
            criterion::black_box(n);
        })
    });
    large.finish();
}

criterion_group!(
    benches,
    bench_hello_world,
    bench_json_api,
    bench_post_json,
    bench_large_output,
    bench_ipc_echo,
);

criterion_main!(benches);

fn get_next_port() -> u16 {
    NEXT_PORT.fetch_add(1, Ordering::Relaxed)
}

fn php_script_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/php_scripts")
        .join(name)
}

fn scripts_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/php_scripts")
}

fn env_bin(var: &str) -> Option<PathBuf> {
    let value = std::env::var(var).ok()?;
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let p = PathBuf::from(value);
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

struct FpmServer {
    process: Child,
    #[allow(dead_code)]
    port: u16,
    socket_path: PathBuf,
}

impl FpmServer {
    fn start() -> Option<Self> {
        let fpm_bin = env_bin("BENCH_FPM_BIN")?;
        if !fpm_bin.exists() {
            eprintln!("php-fpm not found at {:?}", fpm_bin);
            return None;
        }

        let port = get_next_port();
        let socket_path =
            std::env::temp_dir().join(format!("php-fpm-bench-{}.sock", port));

        let _ = std::fs::remove_file(&socket_path);

        let config_path =
            std::env::temp_dir().join(format!("php-fpm-bench-{}.conf", port));
        let config = format!(
            r#"[global]
error_log = /dev/stderr
daemonize = no

[www]
listen = {}
listen.mode = 0666
pm = static
pm.max_children = 1
"#,
            socket_path.display()
        );
        std::fs::write(&config_path, config).ok()?;

        let process = Command::new(&fpm_bin)
            .arg("-y")
            .arg(&config_path)
            .arg("-c")
            .arg("/dev/null")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        for _ in 0..50 {
            if socket_path.exists() {
                std::thread::sleep(Duration::from_millis(50));
                return Some(Self {
                    process,
                    port,
                    socket_path,
                });
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        eprintln!("php-fpm socket never appeared");
        None
    }

    fn execute(
        &self,
        script: &str,
        method: &str,
        body: Option<&[u8]>,
    ) -> Option<Vec<u8>> {
        use std::os::unix::net::UnixStream;

        let mut stream = UnixStream::connect(&self.socket_path).ok()?;
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .ok()?;
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .ok()?;

        let script_path = scripts_dir().join(script);
        let content_length = body
            .map(|b| b.len())
            .unwrap_or(0);

        let params = build_fcgi_params(&[
            ("SCRIPT_FILENAME", script_path.to_str()?),
            ("REQUEST_METHOD", method),
            ("CONTENT_LENGTH", &content_length.to_string()),
            ("CONTENT_TYPE", "application/json"),
            ("QUERY_STRING", ""),
            ("REQUEST_URI", &format!("/{}", script)),
            ("DOCUMENT_ROOT", scripts_dir().to_str()?),
            ("SERVER_PROTOCOL", "HTTP/1.1"),
            ("GATEWAY_INTERFACE", "CGI/1.1"),
            ("SERVER_SOFTWARE", "bench"),
        ]);

        let request_id: u16 = 1;
        let mut request = Vec::new();

        request.extend_from_slice(&[
            1, 1, 0, 1, 0, 8, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0,
        ]);

        let params_len = params.len();
        request.push(1);
        request.push(4);
        request.push((request_id >> 8) as u8);
        request.push(request_id as u8);
        request.push((params_len >> 8) as u8);
        request.push(params_len as u8);
        request.push(0);
        request.push(0);
        request.extend_from_slice(&params);

        request.extend_from_slice(&[1, 4, 0, 1, 0, 0, 0, 0]);

        if let Some(body_data) = body {
            let body_len = body_data.len();
            request.push(1);
            request.push(5);
            request.push((request_id >> 8) as u8);
            request.push(request_id as u8);
            request.push((body_len >> 8) as u8);
            request.push(body_len as u8);
            request.push(0);
            request.push(0);
            request.extend_from_slice(body_data);
        }

        request.extend_from_slice(&[1, 5, 0, 1, 0, 0, 0, 0]);

        stream
            .write_all(&request)
            .ok()?;

        let mut response = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => response.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }

        extract_fcgi_stdout(&response)
    }
}

impl Drop for FpmServer {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

fn build_fcgi_params(params: &[(&str, &str)]) -> Vec<u8> {
    let mut result = Vec::new();
    for (name, value) in params {
        let name_len = name.len();
        let value_len = value.len();

        if name_len < 128 {
            result.push(name_len as u8);
        } else {
            result.push(((name_len >> 24) | 0x80) as u8);
            result.push((name_len >> 16) as u8);
            result.push((name_len >> 8) as u8);
            result.push(name_len as u8);
        }

        if value_len < 128 {
            result.push(value_len as u8);
        } else {
            result.push(((value_len >> 24) | 0x80) as u8);
            result.push((value_len >> 16) as u8);
            result.push((value_len >> 8) as u8);
            result.push(value_len as u8);
        }

        result.extend_from_slice(name.as_bytes());
        result.extend_from_slice(value.as_bytes());
    }
    result
}

fn extract_fcgi_stdout(data: &[u8]) -> Option<Vec<u8>> {
    let mut result = Vec::new();
    let mut pos = 0;

    while pos + 8 <= data.len() {
        let _version = data[pos];
        let record_type = data[pos + 1];
        let content_length =
            ((data[pos + 4] as usize) << 8) | (data[pos + 5] as usize);
        let padding_length = data[pos + 6] as usize;

        pos += 8;

        if record_type == 6 && pos + content_length <= data.len() {
            result.extend_from_slice(&data[pos..pos + content_length]);
        }

        pos += content_length + padding_length;
    }

    if let Some(header_end) = result
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
    {
        Some(result[header_end + 4..].to_vec())
    } else {
        Some(result)
    }
}

struct FrankenPhpServer {
    process: Child,
    port: u16,
}

impl FrankenPhpServer {
    fn start() -> Option<Self> {
        let frankenphp_bin = env_bin("BENCH_FRANKENPHP_BIN")?;
        if !frankenphp_bin.exists() {
            eprintln!("frankenphp not found at {:?}", frankenphp_bin);
            return None;
        }

        let port = get_next_port();

        let caddyfile_path =
            std::env::temp_dir().join(format!("Caddyfile-bench-{}", port));
        let caddyfile = format!(
            r#"{{
    admin off
    auto_https off
}}

:{}  {{
    root * {}
    php
}}
"#,
            port,
            scripts_dir().display()
        );
        std::fs::write(&caddyfile_path, caddyfile).ok()?;

        let process = Command::new(&frankenphp_bin)
            .arg("run")
            .arg("--config")
            .arg(&caddyfile_path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        for _ in 0..50 {
            if TcpStream::connect(format!("127.0.0.1:{}", port)).is_ok() {
                std::thread::sleep(Duration::from_millis(50));
                return Some(Self { process, port });
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        eprintln!("FrankenPHP never started on port {}", port);
        None
    }

    fn execute(
        &self,
        script: &str,
        method: &str,
        body: Option<&[u8]>,
    ) -> Option<Vec<u8>> {
        let mut stream =
            TcpStream::connect(format!("127.0.0.1:{}", self.port)).ok()?;
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .ok()?;
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .ok()?;

        let content_length = body
            .map(|b| b.len())
            .unwrap_or(0);

        let request = if body.is_some() {
            format!(
                "{} /{} HTTP/1.1\r\n\
                Host: localhost:{}\r\n\
                Content-Type: application/json\r\n\
                Content-Length: {}\r\n\
                Connection: close\r\n\
                \r\n",
                method, script, self.port, content_length
            )
        } else {
            format!(
                "{} /{} HTTP/1.1\r\n\
                Host: localhost:{}\r\n\
                Connection: close\r\n\
                \r\n",
                method, script, self.port
            )
        };

        stream
            .write_all(request.as_bytes())
            .ok()?;
        if let Some(body_data) = body {
            stream
                .write_all(body_data)
                .ok()?;
        }

        let mut response = Vec::new();
        stream
            .read_to_end(&mut response)
            .ok()?;

        if let Some(header_end) = response
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
        {
            Some(response[header_end + 4..].to_vec())
        } else {
            Some(response)
        }
    }
}

impl Drop for FrankenPhpServer {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

fn should_compare() -> bool {
    std::env::var("BENCH_COMPARE").is_ok()
}

fn fpm_only() -> bool {
    std::env::var("BENCH_FPM_ONLY").is_ok()
}

fn frankenphp_only() -> bool {
    std::env::var("BENCH_FRANKENPHP_ONLY").is_ok()
}
