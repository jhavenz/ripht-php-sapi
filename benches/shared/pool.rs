#![allow(dead_code)]

use super::backend::Backend;
use super::env::workers_from_env;
use super::protocol::{read_message, write_message, Method, Request, Response};
use serde_bytes::ByteBuf;
use std::io::{BufReader, BufWriter};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

struct Worker {
    #[allow(dead_code)]
    child: Child,
    writer: BufWriter<ChildStdin>,
    reader: BufReader<ChildStdout>,
}

impl Worker {
    fn spawn() -> Self {
        let exe = std::env::current_exe().expect("failed to get current exe");

        let mut child = Command::new(exe)
            .env("SAPI_WORKER", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to spawn worker");

        let stdin = child
            .stdin
            .take()
            .expect("failed to get stdin");
        let stdout = child
            .stdout
            .take()
            .expect("failed to get stdout");

        Worker {
            child,
            writer: BufWriter::new(stdin),
            reader: BufReader::new(stdout),
        }
    }

    fn send(&mut self, req: &Request) -> Response {
        write_message(&mut self.writer, req).expect("failed to write request");

        read_message(&mut self.reader).expect("failed to read response")
    }
}

pub struct Pool {
    workers: Vec<Worker>,
    next: usize,
}

impl Pool {
    pub fn new(count: usize) -> Self {
        let workers = (0..count)
            .map(|_| Worker::spawn())
            .collect();

        Pool { workers, next: 0 }
    }

    pub fn from_env() -> Self {
        Self::new(workers_from_env())
    }

    pub fn send(&mut self, req: &Request) -> Response {
        let idx = self.next;
        self.next = (self.next + 1) % self.workers.len();

        self.workers[idx].send(req)
    }

    pub fn exec(
        &mut self,
        script: &str,
        method: Method,
        uri: Option<&str>,
        content_type: Option<&str>,
        body: Option<&[u8]>,
    ) -> Response {
        let req = Request::Exec {
            method,
            script: script.to_string(),
            uri: uri.map(String::from),
            content_type: content_type.map(String::from),
            body: body.map(|b| ByteBuf::from(b.to_vec())),
        };

        self.send(&req)
    }

    pub fn echo(&mut self, size: usize) -> usize {
        let req = Request::Echo { size };

        match self.send(&req) {
            Response::Echo { size } => size,
            _ => 0,
        }
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        for worker in &mut self.workers {
            let _ = write_message(&mut worker.writer, &Request::Shutdown);
        }
    }
}

pub struct PooledBackend {
    pool: Pool,
}

impl PooledBackend {
    pub fn new(workers: usize) -> Self {
        Self {
            pool: Pool::new(workers),
        }
    }

    pub fn from_env() -> Self {
        Self {
            pool: Pool::from_env(),
        }
    }
}

impl Backend for PooledBackend {
    fn name(&self) -> &'static str {
        "rust_sapi_pooled"
    }

    fn execute(
        &mut self,
        script: &str,
        method: Method,
        body: Option<&[u8]>,
    ) -> Option<Vec<u8>> {
        let content_type = body.map(|_| "application/json");

        match self
            .pool
            .exec(script, method, None, content_type, body)
        {
            Response::Exec { body, .. } => Some(body),
            Response::Error { message } => {
                eprintln!("PooledBackend error: {}", message);
                None
            }
            _ => None,
        }
    }
}
