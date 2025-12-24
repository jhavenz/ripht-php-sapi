use std::io::{BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use ripht_php_sapi::{RiphtSapi, WebRequest};

// Simple binary framing for bench-only IPC (no JSON, no text headers)
// Frames (little-endian):
// Request: 0x01 EXEC | 0x02 ECHO
//  EXEC: [0x01][u8 method][u32 path_len][path][u32 uri_len][uri][u32 ct_len][ct][u32 body_len][body]
//  ECHO: [0x02][u32 body_len]
// Response: 0x81 EXEC_RESP | 0x82 ECHO_RESP
//  EXEC_RESP: [0x81][u16 status][u32 body_len][body]
//  ECHO_RESP: [0x82][u32 body_len][body]

const OP_EXEC: u8 = 0x01;
const OP_ECHO: u8 = 0x02;
const OP_EXEC_RESP: u8 = 0x81;
const OP_ECHO_RESP: u8 = 0x82;

pub mod worker {
    use super::*;

    pub fn maybe_run_worker() {
        if std::env::var("SAPI_WORKER").is_ok() {
            run();
            std::process::exit(0);
        }
    }

    pub fn run() {
        let sapi = RiphtSapi::instance();
        let mut stdin = std::io::stdin().lock();
        let mut stdout = std::io::stdout().lock();

        let mut op = [0u8; 1];
        while let Ok(()) = stdin.read_exact(&mut op) {
            match op[0] {
                OP_EXEC => handle_exec(&sapi, &mut stdin, &mut stdout),
                OP_ECHO => handle_echo(&mut stdin, &mut stdout),
                _ => break,
            }
        }
    }

    fn handle_exec(
        sapi: &RiphtSapi,
        stdin: &mut dyn Read,
        stdout: &mut dyn Write,
    ) {
        let body = read_vec(stdin);
        let ct = read_string(stdin);
        let uri = read_string(stdin);
        let method_id = read_u8(stdin);
        let script = read_string(stdin);

        let script_path = PathBuf::from(script);

        let mut builder = match method_id {
            0 => WebRequest::get(),
            1 => WebRequest::post(),
            2 => WebRequest::put(),
            3 => WebRequest::delete(),
            4 => WebRequest::patch(),
            5 => WebRequest::head(),
            6 => WebRequest::options(),
            _ => WebRequest::get(),
        };

        if !uri.is_empty() {
            builder = builder.with_uri(uri);
        }

        if !ct.is_empty() {
            builder = builder.with_content_type(ct);
        }

        if !body.is_empty() {
            builder = builder.with_body(body);
        }

        if let Ok(ctx) = builder.build(&script_path) {
            match sapi.execute(ctx) {
                Ok(result) => {
                    let _ = stdout.write_all(&[OP_EXEC_RESP]);

                    write_u16(stdout, result.status_code());
                    write_u32(stdout, result.body().len() as u32);

                    let _ = stdout.write_all(&result.body());
                    let _ = stdout.flush();
                }
                Err(_) => {
                    let _ = stdout.write_all(&[OP_EXEC_RESP]);

                    write_u16(stdout, 500);
                    write_u32(stdout, 0);

                    let _ = stdout.flush();
                }
            }
        } else {
            let _ = stdout.write_all(&[OP_EXEC_RESP]);

            write_u16(stdout, 404);
            write_u32(stdout, 0);

            let _ = stdout.flush();
        }
    }

    fn handle_echo(stdin: &mut dyn Read, stdout: &mut dyn Write) {
        let len = read_u32(stdin) as usize;
        let _ = stdout.write_all(&[OP_ECHO_RESP]);
        write_u32(stdout, len as u32);

        // Stream zeroed bytes w/out heap alloc for large payloads
        let mut remaining = len;
        let buf = [0u8; 8192];

        while remaining > 0 {
            let chunk = remaining.min(buf.len());
            let _ = stdout.write_all(&buf[..chunk]);
            remaining -= chunk;
        }

        let _ = stdout.flush();
    }

    fn read_u8(r: &mut dyn Read) -> u8 {
        let mut b = [0u8; 1];
        let _ = r.read_exact(&mut b);

        b[0]
    }

    fn read_u32(r: &mut dyn Read) -> u32 {
        let mut b = [0u8; 4];
        let _ = r.read_exact(&mut b);

        u32::from_le_bytes(b)
    }

    fn read_string(r: &mut dyn Read) -> String {
        let len = read_u32(r) as usize;

        if len == 0 {
            return String::new();
        }

        let mut v = vec![0u8; len];
        let _ = r.read_exact(&mut v);

        String::from_utf8(v).unwrap_or_default()
    }

    fn read_vec(r: &mut dyn Read) -> Vec<u8> {
        let len = read_u32(r) as usize;
        let mut v = vec![0u8; len];

        if len > 0 {
            let _ = r.read_exact(&mut v);
        }

        v
    }
}

pub struct Pool {
    workers: Vec<Proc>,
    next: usize,
}

impl Pool {
    pub fn new(n: usize) -> Self {
        let mut workers = Vec::with_capacity(n);
        for _ in 0..n {
            workers.push(Proc::spawn());
        }

        Self { workers, next: 0 }
    }

    pub fn exec_request(&mut self, req: &Exec) -> ReplyMeta {
        let idx = self.next;
        let len = self.workers.len();

        self.next = (idx + 1) % len;

        let w = &mut self.workers[idx];
        w.send_exec(req);
        w.recv_exec()
    }

    pub fn echo(&mut self, body_len: usize) -> usize {
        let idx = self.next;
        let len = self.workers.len();

        self.next = (idx + 1) % len;

        let w = &mut self.workers[idx];
        w.send_echo(body_len);
        w.recv_echo()
    }
}

struct Proc {
    _child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Proc {
    fn spawn() -> Self {
        let exe = std::env::current_exe().expect("bench exe");

        let mut child = Command::new(exe)
            .env("SAPI_WORKER", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn worker");

        let stdin = child
            .stdin
            .take()
            .expect("worker stdin");

        let stdout = child
            .stdout
            .take()
            .expect("worker stdout");

        Self {
            _child: child,
            stdin,
            stdout: BufReader::new(stdout),
        }
    }

    fn send_exec(&mut self, req: &Exec) {
        let _ = self
            .stdin
            .write_all(&[OP_EXEC]);

        self.stdin
            .write_all(&[req.method])
            .ok();

        write_string(&mut self.stdin, &req.script);

        write_string(
            &mut self.stdin,
            req.uri
                .as_deref()
                .unwrap_or(""),
        );

        write_string(
            &mut self.stdin,
            req.content_type
                .as_deref()
                .unwrap_or(""),
        );

        write_vec(
            &mut self.stdin,
            req.body
                .as_deref()
                .unwrap_or(&[]),
        );

        let _ = self.stdin.flush();
    }

    fn recv_exec(&mut self) -> ReplyMeta {
        let mut op = [0u8; 1];

        let _ = self
            .stdout
            .read_exact(&mut op);

        debug_assert_eq!(op[0], OP_EXEC_RESP);

        let status = read_u16(&mut self.stdout);
        let body_len = read_u32(&mut self.stdout) as usize;

        drain_body(&mut self.stdout, body_len);

        ReplyMeta {
            _status: status,
            body_len,
        }
    }

    fn send_echo(&mut self, body_len: usize) {
        let _ = self
            .stdin
            .write_all(&[OP_ECHO]);

        write_u32(&mut self.stdin, body_len as u32);

        let _ = self.stdin.flush();
    }

    fn recv_echo(&mut self) -> usize {
        let mut op = [0u8; 1];

        let _ = self
            .stdout
            .read_exact(&mut op);

        debug_assert_eq!(op[0], OP_ECHO_RESP);

        let body_len = read_u32(&mut self.stdout) as usize;
        drain_body(&mut self.stdout, body_len);

        body_len
    }
}

pub struct Exec {
    pub method: u8, // 0=GET,1=POST,...
    pub script: String,
    pub uri: Option<String>,
    pub content_type: Option<String>,
    pub body: Option<Vec<u8>>,
}

pub struct ReplyMeta {
    pub _status: u16,
    pub body_len: usize,
}

fn write_u16(w: &mut dyn Write, v: u16) {
    let _ = w.write_all(&v.to_le_bytes());
}

fn write_u32(w: &mut dyn Write, v: u32) {
    let _ = w.write_all(&v.to_le_bytes());
}

fn write_string(w: &mut dyn Write, s: &str) {
    write_u32(w, s.len() as u32);
    let _ = w.write_all(s.as_bytes());
}

fn write_vec(w: &mut dyn Write, v: &[u8]) {
    write_u32(w, v.len() as u32);
    if !v.is_empty() {
        let _ = w.write_all(v);
    }
}

fn read_u16(r: &mut dyn Read) -> u16 {
    let mut b = [0u8; 2];
    let _ = r.read_exact(&mut b);
    u16::from_le_bytes(b)
}

fn read_u32(r: &mut dyn Read) -> u32 {
    let mut b = [0u8; 4];
    let _ = r.read_exact(&mut b);
    u32::from_le_bytes(b)
}

fn drain_body(r: &mut dyn Read, len: usize) {
    let mut remaining = len;

    let mut buf = [0u8; 8192];

    while remaining > 0 {
        let to = remaining.min(buf.len());

        let n = r
            .read(&mut buf[..to])
            .unwrap_or(0);

        if n == 0 {
            break;
        };

        remaining -= n;
    }
}

pub fn workers_from_env() -> usize {
    std::env::var("BENCH_WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or(4)
}
