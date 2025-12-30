#![allow(dead_code)]

use super::backend::Backend;
use super::env::{frankenphp_bin, next_port, scripts_dir};
use super::protocol::Method;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

pub struct FrankenPhpBackend {
    process: Child,
    port: u16,
}

impl FrankenPhpBackend {
    pub fn start() -> Option<Self> {
        let bin = frankenphp_bin()?;
        let port = next_port();

        let process = Command::new(&bin)
            .arg("php-server")
            .arg("--listen")
            .arg(format!("127.0.0.1:{}", port))
            .arg("--root")
            .arg(scripts_dir())
            .stdin(Stdio::null())
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

        eprintln!(
            "FrankenPhpBackend: timed out waiting for server on port {}",
            port
        );
        None
    }

    fn http_execute(
        &self,
        script: &str,
        method: Method,
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

        let content_len = body
            .map(|b| b.len())
            .unwrap_or(0);

        let request = if body.is_some() {
            format!(
                "{} /{} HTTP/1.1\r\n\
                 Host: 127.0.0.1:{}\r\n\
                 Content-Type: application/json\r\n\
                 Content-Length: {}\r\n\
                 Connection: close\r\n\r\n",
                method.as_str(),
                script,
                self.port,
                content_len
            )
        } else {
            format!(
                "{} /{} HTTP/1.1\r\n\
                 Host: 127.0.0.1:{}\r\n\
                 Connection: close\r\n\r\n",
                method.as_str(),
                script,
                self.port
            )
        };

        stream
            .write_all(request.as_bytes())
            .ok()?;

        if let Some(b) = body {
            stream.write_all(b).ok()?;
        }

        stream.flush().ok()?;

        let mut response = Vec::new();
        stream
            .read_to_end(&mut response)
            .ok()?;

        if let Some(pos) = response
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
        {
            Some(response[pos + 4..].to_vec())
        } else {
            Some(response)
        }
    }
}

impl Backend for FrankenPhpBackend {
    fn name(&self) -> &'static str {
        "frankenphp"
    }

    fn execute(
        &mut self,
        script: &str,
        method: Method,
        body: Option<&[u8]>,
    ) -> Option<Vec<u8>> {
        self.http_execute(script, method, body)
    }
}

impl Drop for FrankenPhpBackend {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}
