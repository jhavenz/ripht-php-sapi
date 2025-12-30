#![allow(dead_code)]

use super::backend::Backend;
use super::env::{fpm_bin, next_port, scripts_dir};
use super::protocol::Method;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

pub struct FpmBackend {
    process: Child,
    socket_path: PathBuf,
    #[allow(dead_code)]
    config_path: PathBuf,
}

impl FpmBackend {
    pub fn start() -> Option<Self> {
        let bin = fpm_bin()?;
        let id = next_port();

        let socket_path =
            std::env::temp_dir().join(format!("bench-fpm-{}.sock", id));
        let config_path =
            std::env::temp_dir().join(format!("bench-fpm-{}.conf", id));

        // Clean up stale socket
        let _ = std::fs::remove_file(&socket_path);

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

        std::fs::write(&config_path, &config).ok()?;

        let process = Command::new(&bin)
            .arg("-F")
            .arg("-y")
            .arg(&config_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        for _ in 0..50 {
            if socket_path.exists() {
                // Pause a sec...
                std::thread::sleep(Duration::from_millis(100));

                return Some(Self {
                    process,
                    socket_path,
                    config_path,
                });
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        eprintln!("FpmBackend: timed out waiting for socket");
        None
    }

    fn fcgi_execute(
        &self,
        script: &str,
        method: Method,
        body: Option<&[u8]>,
    ) -> Option<Vec<u8>> {
        let script_path = scripts_dir().join(script);

        let mut stream = UnixStream::connect(&self.socket_path).ok()?;
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .ok()?;
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .ok()?;

        let content_len = body
            .map(|b| b.len())
            .unwrap_or(0);

        let params = [
            ("REQUEST_METHOD", method.as_str()),
            ("SCRIPT_FILENAME", &script_path.to_string_lossy()),
            ("SCRIPT_NAME", script),
            ("REQUEST_URI", &format!("/{}", script)),
            ("QUERY_STRING", ""),
            (
                "CONTENT_TYPE",
                if body.is_some() {
                    "application/json"
                } else {
                    ""
                },
            ),
            ("CONTENT_LENGTH", &content_len.to_string()),
            ("SERVER_SOFTWARE", "bench"),
            ("SERVER_NAME", "localhost"),
            ("SERVER_PORT", "80"),
            ("SERVER_PROTOCOL", "HTTP/1.1"),
            ("GATEWAY_INTERFACE", "CGI/1.1"),
        ];

        let params_data = build_fcgi_params(&params);

        // FCGI_BEGIN_REQUEST
        let begin = [
            0, 1, // version, FCGI_BEGIN_REQUEST
            0, 1, // request id
            0, 8, // content length
            0, 0, // padding, reserved
            0, 1, // FCGI_RESPONDER
            0, 0, 0, 0, 0, 0, // flags + reserved
        ];
        stream
            .write_all(&begin)
            .ok()?;

        write_fcgi_record(&mut stream, 4, 1, &params_data)?;
        write_fcgi_record(&mut stream, 4, 1, &[])?;

        if let Some(b) = body {
            write_fcgi_record(&mut stream, 5, 1, b)?;
        }

        write_fcgi_record(&mut stream, 5, 1, &[])?;

        stream.flush().ok()?;

        let mut response = Vec::new();
        let mut buf = [0u8; 8192];

        loop {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => response.extend_from_slice(&buf[..n]),
                Err(_) => break,
            }
        }

        extract_fcgi_stdout(&response)
    }
}

impl Backend for FpmBackend {
    fn name(&self) -> &'static str {
        "php_fpm"
    }

    fn execute(
        &mut self,
        script: &str,
        method: Method,
        body: Option<&[u8]>,
    ) -> Option<Vec<u8>> {
        self.fcgi_execute(script, method, body)
    }
}

impl Drop for FpmBackend {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
        let _ = std::fs::remove_file(&self.socket_path);
        let _ = std::fs::remove_file(&self.config_path);
    }
}

fn build_fcgi_params(params: &[(&str, &str)]) -> Vec<u8> {
    let mut data = Vec::new();

    for (name, value) in params {
        let name_len = name.len();
        let value_len = value.len();

        if name_len < 128 {
            data.push(name_len as u8);
        } else {
            data.push(((name_len >> 24) as u8) | 0x80);
            data.push((name_len >> 16) as u8);
            data.push((name_len >> 8) as u8);
            data.push(name_len as u8);
        }

        if value_len < 128 {
            data.push(value_len as u8);
        } else {
            data.push(((value_len >> 24) as u8) | 0x80);
            data.push((value_len >> 16) as u8);
            data.push((value_len >> 8) as u8);
            data.push(value_len as u8);
        }

        data.extend_from_slice(name.as_bytes());
        data.extend_from_slice(value.as_bytes());
    }

    data
}

fn write_fcgi_record(
    w: &mut dyn Write,
    record_type: u8,
    request_id: u16,
    content: &[u8],
) -> Option<()> {
    let content_len = content.len();
    let padding_len = (8 - (content_len % 8)) % 8;

    let header = [
        1,
        record_type,
        (request_id >> 8) as u8,
        request_id as u8,
        (content_len >> 8) as u8,
        content_len as u8,
        padding_len as u8,
        0,
    ];

    w.write_all(&header).ok()?;
    w.write_all(content).ok()?;

    if padding_len > 0 {
        w.write_all(&vec![0u8; padding_len])
            .ok()?;
    }

    Some(())
}

fn extract_fcgi_stdout(data: &[u8]) -> Option<Vec<u8>> {
    let mut result = Vec::new();
    let mut pos = 0;

    while pos + 8 <= data.len() {
        let record_type = data[pos + 1];
        let content_len =
            ((data[pos + 4] as usize) << 8) | (data[pos + 5] as usize);
        let padding_len = data[pos + 6] as usize;

        pos += 8; // Skip header

        if pos + content_len > data.len() {
            break;
        }

        if record_type == 6 && content_len > 0 {
            result.extend_from_slice(&data[pos..pos + content_len]);
        }

        pos += content_len + padding_len;
    }

    // Strip HTTP headers if present
    if let Some(header_end) = result
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
    {
        Some(result[header_end + 4..].to_vec())
    } else {
        Some(result)
    }
}
