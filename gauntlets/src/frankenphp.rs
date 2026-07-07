use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::{
    artifact_path, artifact_report_path, case::scripts_dir,
    compare_runtime_parity, write_json_artifact, FrankenPhpParityReport,
    GauntletCase, HeaderValue, HttpMethod, ParityComparison,
    RiphtBufferedAdapter, RuntimeAdapter, RuntimeFailure, RuntimeFailureKind,
    RuntimeMessage, RuntimeMode, RuntimeResult,
};

pub const RIPHT_FRANKENPHP_PARITY_ARTIFACT: &str =
    "ripht-frankenphp-parity.json";
pub const FRANKENPHP_BIN_ENV: &str = "RIPHT_GAUNTLET_FRANKENPHP_BIN";

#[derive(Debug)]
pub struct FrankenPhpParityRun {
    pub report: FrankenPhpParityReport,
    pub artifact_path: PathBuf,
}

pub fn run_frankenphp_parity() -> std::io::Result<FrankenPhpParityRun> {
    let artifact_path = artifact_path(RIPHT_FRANKENPHP_PARITY_ARTIFACT);
    let mut report = build_frankenphp_parity_report(now_unix_epoch_secs()?);

    report.ripht.artifact_path =
        Some(artifact_report_path(RIPHT_FRANKENPHP_PARITY_ARTIFACT));
    if let Some(result) = &mut report.frankenphp {
        result.artifact_path =
            Some(artifact_report_path(RIPHT_FRANKENPHP_PARITY_ARTIFACT));
    }

    write_json_artifact(&artifact_path, &report)?;

    Ok(FrankenPhpParityRun {
        report,
        artifact_path,
    })
}

fn build_frankenphp_parity_report(
    generated_unix_epoch_secs: u64,
) -> FrankenPhpParityReport {
    let case =
        GauntletCase::get("frankenphp_parity_sink_events", "sink_events.php");
    let mut ripht = RiphtBufferedAdapter::new();
    let ripht_result = ripht.execute(&case);

    let mut frankenphp = match FrankenPhpAdapter::start() {
        Ok(adapter) => adapter,
        Err(err) => {
            let reason = err.to_string();
            let skipped = err.is_skip();
            let frankenphp = (!skipped).then(|| {
                RuntimeResult::failure(
                    "frankenphp",
                    RuntimeMode::FrankenPhp,
                    case.name,
                    Duration::ZERO,
                    RuntimeFailure::new(
                        RuntimeFailureKind::Execute,
                        reason.clone(),
                    ),
                )
            });

            return FrankenPhpParityReport {
                generated_unix_epoch_secs,
                passed: false,
                skipped,
                skip_reason: skipped.then(|| reason.clone()),
                case: case.name.to_string(),
                frankenphp_binary: None,
                ripht: ripht_result,
                frankenphp,
                comparison: ParityComparison {
                    passed: false,
                    differences: vec![reason],
                },
            };
        }
    };

    let frankenphp_binary = Some(
        frankenphp
            .binary_label()
            .to_string(),
    );
    let frankenphp_result = frankenphp.execute(&case);
    let comparison = compare_runtime_parity(
        "ripht",
        "frankenphp",
        &ripht_result,
        &frankenphp_result,
    )
    .parity_comparison();
    let passed = comparison.passed;

    FrankenPhpParityReport {
        generated_unix_epoch_secs,
        passed,
        skipped: false,
        skip_reason: None,
        case: case.name.to_string(),
        frankenphp_binary,
        ripht: ripht_result,
        frankenphp: Some(frankenphp_result),
        comparison,
    }
}

struct FrankenPhpAdapter {
    process: Child,
    port: u16,
    error_log_path: PathBuf,
    binary: FrankenPhpBinary,
}

impl FrankenPhpAdapter {
    fn start() -> Result<Self, FrankenPhpStartError> {
        let binary = discover_frankenphp_binary()?;
        let port = free_local_port()?;
        let error_log_path = unique_temp_base().with_extension("log");

        let _ = fs::remove_file(&error_log_path);

        let log = fs::File::create(&error_log_path)
            .map_err(|err| FrankenPhpStartError::LogWrite(err.to_string()))?;
        let stderr = log
            .try_clone()
            .map_err(|err| FrankenPhpStartError::LogWrite(err.to_string()))?;

        let process = Command::new(&binary.path)
            .arg("php-server")
            .arg("--listen")
            .arg(format!("127.0.0.1:{port}"))
            .arg("--root")
            .arg(scripts_dir())
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(stderr))
            .spawn()
            .map_err(|err| FrankenPhpStartError::Spawn(err.to_string()))?;

        let mut adapter = Self {
            process,
            port,
            error_log_path,
            binary,
        };

        adapter.wait_for_server()?;

        Ok(adapter)
    }

    fn execute(&mut self, case: &GauntletCase) -> RuntimeResult {
        let started_at = Instant::now();

        match self.execute_case(case) {
            Ok(response) => RuntimeResult {
                runtime: "frankenphp".to_string(),
                mode: RuntimeMode::FrankenPhp,
                case: case.name.to_string(),
                status_code: Some(response.status_code),
                exit_status: response.exit_status,
                headers: response.headers,
                body: response.body,
                messages: response.messages,
                report: None,
                events: Vec::new(),
                duration_ms: started_at
                    .elapsed()
                    .as_millis(),
                artifact_path: None,
                failure: None,
            },
            Err(err) => RuntimeResult::failure(
                "frankenphp",
                RuntimeMode::FrankenPhp,
                case.name,
                started_at.elapsed(),
                RuntimeFailure::new(RuntimeFailureKind::Execute, err),
            ),
        }
    }

    fn binary_label(&self) -> &str {
        &self.binary.label
    }

    fn wait_for_server(&mut self) -> Result<(), FrankenPhpStartError> {
        for _ in 0..100 {
            if TcpStream::connect(("127.0.0.1", self.port)).is_ok() {
                std::thread::sleep(Duration::from_millis(50));
                return Ok(());
            }

            if let Some(status) = self
                .process
                .try_wait()
                .map_err(|err| FrankenPhpStartError::Spawn(err.to_string()))?
            {
                return Err(FrankenPhpStartError::Exited {
                    status: status.to_string(),
                    log: read_optional_string(&self.error_log_path),
                });
            }

            std::thread::sleep(Duration::from_millis(50));
        }

        Err(FrankenPhpStartError::Timeout(read_optional_string(
            &self.error_log_path,
        )))
    }

    fn execute_case(
        &self,
        case: &GauntletCase,
    ) -> Result<FrankenPhpResponse, String> {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port))
            .map_err(|err| err.to_string())?;

        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|err| err.to_string())?;
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .map_err(|err| err.to_string())?;

        write_http_request(&mut stream, self.port, case)?;

        let mut response = Vec::new();
        stream
            .read_to_end(&mut response)
            .map_err(|err| err.to_string())?;

        parse_http_response(&response)
    }
}

impl Drop for FrankenPhpAdapter {
    fn drop(&mut self) {
        if matches!(self.process.try_wait(), Ok(None)) {
            let _ = self.process.kill();
        }

        let _ = self.process.wait();
        let _ = fs::remove_file(&self.error_log_path);
    }
}

struct FrankenPhpBinary {
    path: PathBuf,
    label: String,
}

#[derive(Debug)]
enum FrankenPhpStartError {
    MissingBinary,
    InvalidEnvPath(String),
    PortBind(String),
    LogWrite(String),
    Spawn(String),
    Exited { status: String, log: String },
    Timeout(String),
}

impl std::fmt::Display for FrankenPhpStartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingBinary => write!(
                f,
                "frankenphp binary not found; set {FRANKENPHP_BIN_ENV} or add frankenphp to PATH"
            ),
            Self::InvalidEnvPath(path) => {
                write!(f, "{FRANKENPHP_BIN_ENV} path does not exist: {path}")
            }
            Self::PortBind(message) => {
                write!(f, "failed to allocate local FrankenPHP port: {message}")
            }
            Self::LogWrite(message) => {
                write!(f, "failed to create FrankenPHP log file: {message}")
            }
            Self::Spawn(message) => {
                write!(f, "failed to start FrankenPHP: {message}")
            }
            Self::Exited { status, log } => {
                write!(f, "FrankenPHP exited before readiness: {status}; {log}")
            }
            Self::Timeout(log) => {
                write!(f, "timed out waiting for FrankenPHP server; {log}")
            }
        }
    }
}

impl FrankenPhpStartError {
    fn is_skip(&self) -> bool {
        matches!(self, Self::MissingBinary)
    }
}

fn discover_frankenphp_binary() -> Result<FrankenPhpBinary, FrankenPhpStartError>
{
    if let Some(value) =
        env::var_os(FRANKENPHP_BIN_ENV).filter(|value| !value.is_empty())
    {
        let path = PathBuf::from(&value);

        if path.exists() {
            let label = format!("{FRANKENPHP_BIN_ENV}={}", path.display());
            return Ok(FrankenPhpBinary { path, label });
        }

        return Err(FrankenPhpStartError::InvalidEnvPath(
            path.to_string_lossy()
                .into_owned(),
        ));
    }

    find_on_path("frankenphp")
        .map(|path| FrankenPhpBinary {
            label: format!("PATH={}", path.display()),
            path,
        })
        .ok_or(FrankenPhpStartError::MissingBinary)
}

fn find_on_path(binary: &str) -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;

    env::split_paths(&paths)
        .map(|path| path.join(binary))
        .find(|candidate| candidate.is_file())
}

fn free_local_port() -> Result<u16, FrankenPhpStartError> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|err| FrankenPhpStartError::PortBind(err.to_string()))?;
    let port = listener
        .local_addr()
        .map_err(|err| FrankenPhpStartError::PortBind(err.to_string()))?
        .port();

    drop(listener);

    Ok(port)
}

fn unique_temp_base() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();

    env::temp_dir().join(format!(
        "ripht-gauntlet-frankenphp-{}-{nanos}",
        std::process::id()
    ))
}

fn read_optional_string(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

struct FrankenPhpResponse {
    status_code: u16,
    exit_status: Option<i32>,
    headers: Vec<HeaderValue>,
    body: Vec<u8>,
    messages: Vec<RuntimeMessage>,
}

fn write_http_request<W: Write>(
    stream: &mut W,
    port: u16,
    case: &GauntletCase,
) -> Result<(), String> {
    let request_uri = case
        .uri
        .clone()
        .unwrap_or_else(|| format!("/{}", case.script));
    let body = case
        .body
        .as_deref()
        .unwrap_or_default();
    let method = method_name(case.method);

    write!(
        stream,
        "{method} {request_uri} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n"
    )
    .map_err(|err| err.to_string())?;

    if !body.is_empty() {
        let content_type = case
            .content_type
            .unwrap_or("application/octet-stream");

        write!(
            stream,
            "Content-Type: {content_type}\r\nContent-Length: {}\r\n",
            body.len()
        )
        .map_err(|err| err.to_string())?;
    }

    stream
        .write_all(b"\r\n")
        .map_err(|err| err.to_string())?;
    stream
        .write_all(body)
        .map_err(|err| err.to_string())?;
    stream
        .flush()
        .map_err(|err| err.to_string())
}

fn method_name(method: HttpMethod) -> &'static str {
    match method {
        HttpMethod::Get => "GET",
        HttpMethod::Post => "POST",
        HttpMethod::Put => "PUT",
        HttpMethod::Delete => "DELETE",
        HttpMethod::Patch => "PATCH",
        HttpMethod::Head => "HEAD",
        HttpMethod::Options => "OPTIONS",
    }
}

fn parse_http_response(bytes: &[u8]) -> Result<FrankenPhpResponse, String> {
    let Some((header_end, delimiter_len)) = header_delimiter(bytes) else {
        return Err(
            "HTTP response did not include a header delimiter".to_string()
        );
    };

    let header_text = String::from_utf8_lossy(&bytes[..header_end]);
    let mut lines = header_text.lines();
    let status_line = lines
        .next()
        .ok_or_else(|| {
            "HTTP response did not include a status line".to_string()
        })?
        .trim_end_matches('\r');
    let status_code = parse_status_code(status_line)?;
    let mut headers = Vec::new();

    for line in lines {
        let line = line.trim_end_matches('\r');
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };

        headers.push(HeaderValue::new(name.trim(), value.trim()));
    }

    let body = bytes[header_end + delimiter_len..].to_vec();
    let body = if contains_header(&headers, "Transfer-Encoding", "chunked") {
        decode_chunked_body(&body)?
    } else {
        truncate_to_content_length(&headers, body)
    };

    Ok(FrankenPhpResponse {
        status_code,
        exit_status: None,
        headers,
        body,
        messages: Vec::new(),
    })
}

fn parse_status_code(status_line: &str) -> Result<u16, String> {
    status_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| format!("HTTP status line missing code: {status_line}"))?
        .parse::<u16>()
        .map_err(|err| format!("HTTP status code was invalid: {err}"))
}

fn header_delimiter(bytes: &[u8]) -> Option<(usize, usize)> {
    bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| (position, 4))
        .or_else(|| {
            bytes
                .windows(2)
                .position(|window| window == b"\n\n")
                .map(|position| (position, 2))
        })
}

fn contains_header(
    headers: &[HeaderValue],
    expected_name: &str,
    expected_value: &str,
) -> bool {
    headers.iter().any(|header| {
        header
            .name
            .eq_ignore_ascii_case(expected_name)
            && header
                .value
                .split(',')
                .any(|value| {
                    value
                        .trim()
                        .eq_ignore_ascii_case(expected_value)
                })
    })
}

fn truncate_to_content_length(
    headers: &[HeaderValue],
    body: Vec<u8>,
) -> Vec<u8> {
    let Some(length) = headers
        .iter()
        .find_map(|header| {
            if header
                .name
                .eq_ignore_ascii_case("Content-Length")
            {
                header
                    .value
                    .parse::<usize>()
                    .ok()
            } else {
                None
            }
        })
    else {
        return body;
    };

    body.into_iter()
        .take(length)
        .collect()
}

fn decode_chunked_body(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut decoded = Vec::new();
    let mut position = 0;

    loop {
        let line_end = bytes[position..]
            .windows(2)
            .position(|window| window == b"\r\n")
            .map(|offset| position + offset)
            .ok_or_else(|| {
                "chunked body was missing a chunk size terminator".to_string()
            })?;
        let size_text = String::from_utf8_lossy(&bytes[position..line_end]);
        let size_hex = size_text
            .split(';')
            .next()
            .unwrap_or_default()
            .trim();
        let size = usize::from_str_radix(size_hex, 16)
            .map_err(|err| format!("chunked body size was invalid: {err}"))?;

        position = line_end + 2;

        if size == 0 {
            return Ok(decoded);
        }

        let chunk_end = position + size;
        if chunk_end + 2 > bytes.len() {
            return Err(
                "chunked body ended before declared chunk size".to_string()
            );
        }

        decoded.extend_from_slice(&bytes[position..chunk_end]);

        if &bytes[chunk_end..chunk_end + 2] != b"\r\n" {
            return Err(
                "chunked body chunk was missing trailing CRLF".to_string()
            );
        }

        position = chunk_end + 2;
    }
}

fn now_unix_epoch_secs() -> std::io::Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(std::io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::{
        decode_chunked_body, parse_http_response, truncate_to_content_length,
        write_http_request, FrankenPhpStartError,
    };
    use crate::{
        compare_runtime_parity, HeaderValue, RuntimeMode, RuntimeResult,
    };

    #[test]
    fn http_response_parser_preserves_status_headers_and_body() {
        let parsed = parse_http_response(
            b"HTTP/1.1 201 Created\r\nX-Test: one\r\nX-Test: two\r\nContent-Length: 4\r\n\r\nbodyextra",
        )
        .expect("HTTP response should parse");

        assert_eq!(parsed.status_code, 201);
        assert_eq!(parsed.exit_status, None);
        assert_eq!(parsed.headers.len(), 3);
        assert_eq!(parsed.headers[0].name, "X-Test");
        assert_eq!(parsed.headers[0].value, "one");
        assert_eq!(parsed.headers[1].value, "two");
        assert_eq!(parsed.body, b"body");
    }

    #[test]
    fn http_request_does_not_emit_env_as_headers() {
        let case = crate::GauntletCase::get(
            "frankenphp_env_boundary",
            "sink_events.php",
        )
        .with_env("FOO", "bar");
        let mut request = Vec::new();

        write_http_request(&mut request, 8080, &case)
            .expect("HTTP request should write");
        let request_text =
            String::from_utf8(request).expect("HTTP request should be UTF-8");

        assert!(!request_text.contains("\r\nFOO: bar\r\n"));
    }

    #[test]
    fn http_response_parser_decodes_chunked_body() {
        let parsed = parse_http_response(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nalpha\r\n5\r\nomega\r\n0\r\n\r\n",
        )
        .expect("chunked HTTP response should parse");

        assert_eq!(parsed.status_code, 200);
        assert_eq!(parsed.body, b"alphaomega");
    }

    #[test]
    fn chunked_decoder_rejects_incomplete_chunks() {
        let err = decode_chunked_body(b"5\r\nalph")
            .expect_err("incomplete chunk should fail");

        assert!(err.contains("ended before declared chunk size"));
    }

    #[test]
    fn content_length_truncates_response_body() {
        let headers = vec![HeaderValue::new("Content-Length", "3")];
        let body = truncate_to_content_length(&headers, b"abcdef".to_vec());

        assert_eq!(body, b"abc");
    }

    #[test]
    fn parity_comparison_requires_status_body_and_probe_header() {
        let ripht = runtime_result(
            "ripht",
            RuntimeMode::RiphtBuffered,
            b"alphaomega",
            vec![HeaderValue::new("X-Ripht-Sink", "yes")],
        );
        let frankenphp = runtime_result(
            "frankenphp",
            RuntimeMode::FrankenPhp,
            b"alphaomega",
            vec![HeaderValue::new("X-Ripht-Sink", "yes")],
        );

        assert!(
            compare_runtime_parity("ripht", "frankenphp", &ripht, &frankenphp)
                .passed
        );
    }

    #[test]
    fn frankenphp_start_skip_is_limited_to_missing_binary_cases() {
        assert!(FrankenPhpStartError::MissingBinary.is_skip());
        assert!(!FrankenPhpStartError::InvalidEnvPath("missing".to_string())
            .is_skip());
        assert!(!FrankenPhpStartError::Spawn("boom".to_string()).is_skip());
        assert!(!FrankenPhpStartError::Timeout("log".to_string()).is_skip());
    }

    fn runtime_result(
        runtime: &str,
        mode: RuntimeMode,
        body: &[u8],
        headers: Vec<HeaderValue>,
    ) -> RuntimeResult {
        RuntimeResult {
            runtime: runtime.to_string(),
            mode,
            case: "frankenphp_parity_sink_events".to_string(),
            status_code: Some(200),
            exit_status: Some(0),
            headers,
            body: body.to_vec(),
            messages: Vec::new(),
            report: None,
            events: Vec::new(),
            duration_ms: 0,
            artifact_path: None,
            failure: None,
        }
    }
}
