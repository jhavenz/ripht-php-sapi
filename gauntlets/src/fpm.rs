use std::env;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::{
    artifact_path, artifact_report_path, case::scripts_dir,
    write_json_artifact, FpmParityReport, GauntletCase, HeaderValue,
    HttpMethod, ParityComparison, RiphtBufferedAdapter, RuntimeAdapter,
    RuntimeFailure, RuntimeFailureKind, RuntimeMessage, RuntimeMode,
    RuntimeResult,
};

pub const RIPHT_FPM_PARITY_ARTIFACT: &str = "ripht-fpm-parity.json";
pub const FPM_BIN_ENV: &str = "RIPHT_GAUNTLET_FPM_BIN";

#[derive(Debug)]
pub struct FpmParityRun {
    pub report: FpmParityReport,
    pub artifact_path: PathBuf,
}

pub fn run_fpm_parity() -> std::io::Result<FpmParityRun> {
    let artifact_path = artifact_path(RIPHT_FPM_PARITY_ARTIFACT);
    let mut report = build_fpm_parity_report(now_unix_epoch_secs()?);

    report.ripht.artifact_path =
        Some(artifact_report_path(RIPHT_FPM_PARITY_ARTIFACT));
    if let Some(result) = &mut report.php_fpm {
        result.artifact_path =
            Some(artifact_report_path(RIPHT_FPM_PARITY_ARTIFACT));
    }

    write_json_artifact(&artifact_path, &report)?;

    Ok(FpmParityRun {
        report,
        artifact_path,
    })
}

fn build_fpm_parity_report(generated_unix_epoch_secs: u64) -> FpmParityReport {
    let case = GauntletCase::get("fpm_parity_sink_events", "sink_events.php");
    let mut ripht = RiphtBufferedAdapter::new();
    let ripht_result = ripht.execute(&case);

    let mut fpm = match FpmAdapter::start() {
        Ok(adapter) => adapter,
        Err(err) => {
            let reason = err.to_string();

            return FpmParityReport {
                generated_unix_epoch_secs,
                passed: false,
                skipped: true,
                skip_reason: Some(reason.clone()),
                case: case.name.to_string(),
                fpm_binary: None,
                ripht: ripht_result,
                php_fpm: None,
                comparison: ParityComparison {
                    passed: false,
                    differences: vec![reason],
                },
            };
        }
    };

    let fpm_binary = Some(fpm.binary_label().to_string());
    let fpm_result = fpm.execute(&case);
    let comparison = compare_parity(&ripht_result, &fpm_result);
    let passed = comparison.passed;

    FpmParityReport {
        generated_unix_epoch_secs,
        passed,
        skipped: false,
        skip_reason: None,
        case: case.name.to_string(),
        fpm_binary,
        ripht: ripht_result,
        php_fpm: Some(fpm_result),
        comparison,
    }
}

struct FpmAdapter {
    process: Child,
    socket_path: PathBuf,
    config_path: PathBuf,
    error_log_path: PathBuf,
    binary: FpmBinary,
}

impl FpmAdapter {
    fn start() -> Result<Self, FpmStartError> {
        let binary = discover_fpm_binary()?;
        let temp_base = unique_temp_base();
        let socket_path = temp_base.with_extension("sock");
        let config_path = temp_base.with_extension("conf");
        let error_log_path = temp_base.with_extension("log");

        let _ = fs::remove_file(&socket_path);
        let _ = fs::remove_file(&config_path);
        let _ = fs::remove_file(&error_log_path);

        fs::write(&config_path, fpm_config(&socket_path, &error_log_path))
            .map_err(|err| FpmStartError::ConfigWrite(err.to_string()))?;

        let process = Command::new(&binary.path)
            .arg("-F")
            .arg("-y")
            .arg(&config_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| FpmStartError::Spawn(err.to_string()))?;

        let mut adapter = Self {
            process,
            socket_path,
            config_path,
            error_log_path,
            binary,
        };

        adapter.wait_for_socket()?;

        Ok(adapter)
    }

    fn execute(&mut self, case: &GauntletCase) -> RuntimeResult {
        let started_at = Instant::now();

        match self.execute_case(case) {
            Ok(response) => RuntimeResult {
                runtime: "php_fpm".to_string(),
                mode: RuntimeMode::PhpFpm,
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
                "php_fpm",
                RuntimeMode::PhpFpm,
                case.name,
                started_at.elapsed(),
                RuntimeFailure::new(RuntimeFailureKind::Execute, err),
            ),
        }
    }

    fn binary_label(&self) -> &str {
        &self.binary.label
    }

    fn wait_for_socket(&mut self) -> Result<(), FpmStartError> {
        for _ in 0..100 {
            if self.socket_path.exists() {
                std::thread::sleep(Duration::from_millis(50));
                return Ok(());
            }

            if let Some(status) = self
                .process
                .try_wait()
                .map_err(|err| FpmStartError::Spawn(err.to_string()))?
            {
                return Err(FpmStartError::Exited {
                    status: status.to_string(),
                    log: read_optional_string(&self.error_log_path),
                });
            }

            std::thread::sleep(Duration::from_millis(50));
        }

        Err(FpmStartError::Timeout(read_optional_string(
            &self.error_log_path,
        )))
    }

    fn execute_case(&self, case: &GauntletCase) -> Result<FpmResponse, String> {
        let script_path = case.script_path();
        let mut stream = UnixStream::connect(&self.socket_path)
            .map_err(|err| err.to_string())?;

        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|err| err.to_string())?;
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .map_err(|err| err.to_string())?;

        write_begin_request(&mut stream)?;
        write_params(&mut stream, case, &script_path)?;
        write_stdin(&mut stream, case.body.as_deref())?;
        stream
            .flush()
            .map_err(|err| err.to_string())?;

        let output = read_fcgi_response(&mut stream)?;
        if output.protocol_status != Some(0) {
            return Err(format!(
                "FastCGI protocol status should be 0, got {:?}",
                output.protocol_status
            ));
        }

        let parsed = parse_cgi_response(&output.stdout)?;
        let messages = if output.stderr.is_empty() {
            Vec::new()
        } else {
            vec![RuntimeMessage {
                level: "stderr".to_string(),
                message: String::from_utf8_lossy(&output.stderr).into_owned(),
            }]
        };

        Ok(FpmResponse {
            status_code: parsed.status_code,
            exit_status: output.app_status,
            headers: parsed.headers,
            body: parsed.body,
            messages,
        })
    }
}

impl Drop for FpmAdapter {
    fn drop(&mut self) {
        if matches!(self.process.try_wait(), Ok(None)) {
            let _ = self.process.kill();
        }

        let _ = self.process.wait();
        let _ = fs::remove_file(&self.socket_path);
        let _ = fs::remove_file(&self.config_path);
        let _ = fs::remove_file(&self.error_log_path);
    }
}

struct FpmBinary {
    path: PathBuf,
    label: String,
}

#[derive(Debug)]
enum FpmStartError {
    MissingBinary,
    InvalidEnvPath(String),
    ConfigWrite(String),
    Spawn(String),
    Exited { status: String, log: String },
    Timeout(String),
}

impl std::fmt::Display for FpmStartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingBinary => write!(
                f,
                "php-fpm binary not found; set {FPM_BIN_ENV} or add php-fpm to PATH"
            ),
            Self::InvalidEnvPath(path) => {
                write!(f, "{FPM_BIN_ENV} path does not exist: {path}")
            }
            Self::ConfigWrite(message) => {
                write!(f, "failed to write php-fpm config: {message}")
            }
            Self::Spawn(message) => write!(f, "failed to start php-fpm: {message}"),
            Self::Exited { status, log } => {
                write!(f, "php-fpm exited before socket readiness: {status}; {log}")
            }
            Self::Timeout(log) => {
                write!(f, "timed out waiting for php-fpm socket; {log}")
            }
        }
    }
}

fn discover_fpm_binary() -> Result<FpmBinary, FpmStartError> {
    if let Some(value) =
        env::var_os(FPM_BIN_ENV).filter(|value| !value.is_empty())
    {
        let path = PathBuf::from(&value);

        if path.exists() {
            let label = format!("{FPM_BIN_ENV}={}", path.display());
            return Ok(FpmBinary { path, label });
        }

        return Err(FpmStartError::InvalidEnvPath(
            path.to_string_lossy()
                .into_owned(),
        ));
    }

    find_on_path("php-fpm")
        .map(|path| FpmBinary {
            label: format!("PATH={}", path.display()),
            path,
        })
        .ok_or(FpmStartError::MissingBinary)
}

fn find_on_path(binary: &str) -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;

    env::split_paths(&paths)
        .map(|path| path.join(binary))
        .find(|candidate| candidate.is_file())
}

fn fpm_config(socket_path: &Path, error_log_path: &Path) -> String {
    format!(
        r#"[global]
error_log = {error_log}
daemonize = no

[www]
listen = {socket}
listen.mode = 0666
pm = static
pm.max_children = 1
clear_env = no
catch_workers_output = yes
decorate_workers_output = no
"#,
        socket = socket_path.display(),
        error_log = error_log_path.display()
    )
}

fn unique_temp_base() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();

    env::temp_dir()
        .join(format!("ripht-gauntlet-fpm-{}-{nanos}", std::process::id()))
}

fn read_optional_string(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

struct FpmResponse {
    status_code: u16,
    exit_status: Option<i32>,
    headers: Vec<HeaderValue>,
    body: Vec<u8>,
    messages: Vec<RuntimeMessage>,
}

struct FcgiOutput {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    app_status: Option<i32>,
    protocol_status: Option<u8>,
}

impl FcgiOutput {
    fn new() -> Self {
        Self {
            stdout: Vec::new(),
            stderr: Vec::new(),
            app_status: None,
            protocol_status: None,
        }
    }

    fn consume(
        &mut self,
        record_type: u8,
        content: &[u8],
    ) -> Result<bool, String> {
        match record_type {
            3 => {
                if content.len() < 5 {
                    return Err(
                        "FastCGI END_REQUEST body was too short".to_string()
                    );
                }

                let status = i32::from_be_bytes([
                    content[0], content[1], content[2], content[3],
                ]);
                self.app_status = Some(status);
                self.protocol_status = Some(content[4]);

                Ok(true)
            }
            6 => {
                self.stdout
                    .extend_from_slice(content);
                Ok(false)
            }
            7 => {
                self.stderr
                    .extend_from_slice(content);
                Ok(false)
            }
            _ => Ok(false),
        }
    }
}

fn write_begin_request(stream: &mut UnixStream) -> Result<(), String> {
    let body = [0, 1, 0, 0, 0, 0, 0, 0];

    write_fcgi_record(stream, 1, &body)
}

fn write_params(
    stream: &mut UnixStream,
    case: &GauntletCase,
    script_path: &Path,
) -> Result<(), String> {
    let content_length = case
        .body
        .as_ref()
        .map(Vec::len)
        .unwrap_or_default()
        .to_string();
    let request_uri = case
        .uri
        .clone()
        .unwrap_or_else(|| format!("/{}", case.script));
    let query_string = request_uri
        .split_once('?')
        .map(|(_, query)| query.to_string())
        .unwrap_or_default();
    let script_name = format!("/{}", case.script);
    let content_type = case
        .content_type
        .unwrap_or(if case.body.is_some() {
            "application/octet-stream"
        } else {
            ""
        });
    let script_filename = script_path
        .to_string_lossy()
        .into_owned();

    let mut params = vec![
        (
            "REQUEST_METHOD".to_string(),
            method_name(case.method).to_string(),
        ),
        ("SCRIPT_FILENAME".to_string(), script_filename),
        ("SCRIPT_NAME".to_string(), script_name),
        ("REQUEST_URI".to_string(), request_uri),
        ("QUERY_STRING".to_string(), query_string),
        ("CONTENT_TYPE".to_string(), content_type.to_string()),
        ("CONTENT_LENGTH".to_string(), content_length),
        ("SERVER_SOFTWARE".to_string(), "ripht-gauntlet".to_string()),
        ("SERVER_NAME".to_string(), "localhost".to_string()),
        ("SERVER_PORT".to_string(), "80".to_string()),
        ("SERVER_PROTOCOL".to_string(), "HTTP/1.1".to_string()),
        ("GATEWAY_INTERFACE".to_string(), "CGI/1.1".to_string()),
        (
            "DOCUMENT_ROOT".to_string(),
            scripts_dir()
                .to_string_lossy()
                .into_owned(),
        ),
    ];

    params.extend(case.env.iter().cloned());

    let params_data = build_fcgi_params(&params);
    write_fcgi_records(stream, 4, &params_data)?;
    write_fcgi_record(stream, 4, &[])
}

fn write_stdin(
    stream: &mut UnixStream,
    body: Option<&[u8]>,
) -> Result<(), String> {
    if let Some(body) = body {
        write_fcgi_records(stream, 5, body)?;
    }

    write_fcgi_record(stream, 5, &[])
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

fn build_fcgi_params(params: &[(String, String)]) -> Vec<u8> {
    let mut data = Vec::new();

    for (name, value) in params {
        write_name_value_len(&mut data, name.len());
        write_name_value_len(&mut data, value.len());
        data.extend_from_slice(name.as_bytes());
        data.extend_from_slice(value.as_bytes());
    }

    data
}

fn write_name_value_len(data: &mut Vec<u8>, len: usize) {
    if len < 128 {
        data.push(len as u8);
        return;
    }

    data.push(((len >> 24) as u8) | 0x80);
    data.push((len >> 16) as u8);
    data.push((len >> 8) as u8);
    data.push(len as u8);
}

fn write_fcgi_records(
    stream: &mut UnixStream,
    record_type: u8,
    bytes: &[u8],
) -> Result<(), String> {
    for chunk in bytes.chunks(u16::MAX as usize) {
        write_fcgi_record(stream, record_type, chunk)?;
    }

    Ok(())
}

fn write_fcgi_record(
    stream: &mut UnixStream,
    record_type: u8,
    content: &[u8],
) -> Result<(), String> {
    let content_len = content.len();
    let padding_len = (8 - (content_len % 8)) % 8;
    let header = [
        1,
        record_type,
        0,
        1,
        (content_len >> 8) as u8,
        content_len as u8,
        padding_len as u8,
        0,
    ];

    stream
        .write_all(&header)
        .map_err(|err| err.to_string())?;
    stream
        .write_all(content)
        .map_err(|err| err.to_string())?;

    if padding_len > 0 {
        stream
            .write_all(&vec![0u8; padding_len])
            .map_err(|err| err.to_string())?;
    }

    Ok(())
}

fn read_fcgi_response(stream: &mut UnixStream) -> Result<FcgiOutput, String> {
    let mut output = FcgiOutput::new();

    loop {
        let mut header = [0u8; 8];
        stream
            .read_exact(&mut header)
            .map_err(|err| format!("failed to read FastCGI header: {err}"))?;

        if header[0] != 1 {
            return Err(format!("unsupported FastCGI version {}", header[0]));
        }

        let request_id = u16::from_be_bytes([header[2], header[3]]);
        if request_id != 1 {
            return Err(format!("unexpected FastCGI request id {request_id}"));
        }

        let record_type = header[1];
        let content_len = u16::from_be_bytes([header[4], header[5]]) as usize;
        let padding_len = header[6] as usize;
        let mut content = vec![0u8; content_len];
        stream
            .read_exact(&mut content)
            .map_err(|err| format!("failed to read FastCGI content: {err}"))?;

        if padding_len > 0 {
            let mut padding = vec![0u8; padding_len];
            stream
                .read_exact(&mut padding)
                .map_err(|err| {
                    format!("failed to read FastCGI padding: {err}")
                })?;
        }

        if output.consume(record_type, &content)? {
            return Ok(output);
        }
    }
}

struct ParsedCgiResponse {
    status_code: u16,
    headers: Vec<HeaderValue>,
    body: Vec<u8>,
}

fn parse_cgi_response(stdout: &[u8]) -> Result<ParsedCgiResponse, String> {
    let Some((header_end, delimiter_len)) = header_delimiter(stdout) else {
        return Ok(ParsedCgiResponse {
            status_code: 200,
            headers: Vec::new(),
            body: stdout.to_vec(),
        });
    };

    let header_text = String::from_utf8_lossy(&stdout[..header_end]);
    let mut status_code = 200;
    let mut headers = Vec::new();

    for line in header_text.lines() {
        let line = line.trim_end_matches('\r');
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        let value = value.trim();

        if name.eq_ignore_ascii_case("Status") {
            if let Some(code) = value
                .split_whitespace()
                .next()
                .and_then(|code| code.parse::<u16>().ok())
            {
                status_code = code;
            }
            continue;
        }

        headers.push(HeaderValue::new(name, value));
    }

    Ok(ParsedCgiResponse {
        status_code,
        headers,
        body: stdout[header_end + delimiter_len..].to_vec(),
    })
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

fn compare_parity(
    ripht: &RuntimeResult,
    fpm: &RuntimeResult,
) -> ParityComparison {
    let mut differences = Vec::new();

    if let Some(failure) = &ripht.failure {
        differences.push(format!("ripht failed: {}", failure.message));
    }
    if let Some(failure) = &fpm.failure {
        differences.push(format!("php-fpm failed: {}", failure.message));
    }
    if ripht.status_code != fpm.status_code {
        differences.push(format!(
            "status mismatch: ripht={:?} php_fpm={:?}",
            ripht.status_code, fpm.status_code
        ));
    }
    if ripht.body != fpm.body {
        differences.push(format!(
            "body mismatch: ripht=`{}` php_fpm=`{}`",
            String::from_utf8_lossy(&ripht.body),
            String::from_utf8_lossy(&fpm.body)
        ));
    }
    if !contains_header(&ripht.headers, "X-Ripht-Sink", "yes") {
        differences.push("ripht missing X-Ripht-Sink: yes".to_string());
    }
    if !contains_header(&fpm.headers, "X-Ripht-Sink", "yes") {
        differences.push("php-fpm missing X-Ripht-Sink: yes".to_string());
    }
    if !fpm.messages.is_empty() {
        differences.push("php-fpm emitted FastCGI stderr".to_string());
    }

    ParityComparison {
        passed: differences.is_empty(),
        differences,
    }
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
            && header.value == expected_value
    })
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
        build_fcgi_params, compare_parity, parse_cgi_response, FcgiOutput,
    };
    use crate::{HeaderValue, RuntimeMode, RuntimeResult};

    #[test]
    fn cgi_response_parser_preserves_status_headers_and_body() {
        let parsed = parse_cgi_response(
            b"Status: 201 Created\r\nX-Test: one\r\nX-Test: two\r\n\r\nbody",
        )
        .expect("CGI response should parse");

        assert_eq!(parsed.status_code, 201);
        assert_eq!(parsed.headers.len(), 2);
        assert_eq!(parsed.headers[0].name, "X-Test");
        assert_eq!(parsed.headers[0].value, "one");
        assert_eq!(parsed.headers[1].value, "two");
        assert_eq!(parsed.body, b"body");
    }

    #[test]
    fn fcgi_output_separates_stdout_stderr_and_end_status() {
        let mut output = FcgiOutput::new();

        assert!(!output
            .consume(6, b"stdout")
            .unwrap());
        assert!(!output
            .consume(7, b"stderr")
            .unwrap());
        assert!(output
            .consume(3, &[0, 0, 0, 9, 0])
            .unwrap());

        assert_eq!(output.stdout, b"stdout");
        assert_eq!(output.stderr, b"stderr");
        assert_eq!(output.app_status, Some(9));
        assert_eq!(output.protocol_status, Some(0));
    }

    #[test]
    fn fcgi_params_encode_large_name_value_lengths() {
        let long_value = "x".repeat(130);
        let params = vec![("A".to_string(), long_value)];
        let encoded = build_fcgi_params(&params);

        assert_eq!(encoded[0], 1);
        assert_eq!(encoded[1], 0x80);
        assert_eq!(encoded[2], 0);
        assert_eq!(encoded[3], 0);
        assert_eq!(encoded[4], 130);
    }

    #[test]
    fn parity_comparison_requires_status_body_and_probe_header() {
        let ripht = runtime_result(
            "ripht",
            b"alphaomega",
            vec![HeaderValue::new("X-Ripht-Sink", "yes")],
        );
        let fpm = runtime_result(
            "php_fpm",
            b"alphaomega",
            vec![HeaderValue::new("X-Ripht-Sink", "yes")],
        );

        assert!(compare_parity(&ripht, &fpm).passed);
    }

    fn runtime_result(
        runtime: &str,
        body: &[u8],
        headers: Vec<HeaderValue>,
    ) -> RuntimeResult {
        RuntimeResult {
            runtime: runtime.to_string(),
            mode: RuntimeMode::PhpFpm,
            case: "fpm_parity_sink_events".to_string(),
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
