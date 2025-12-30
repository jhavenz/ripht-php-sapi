use super::env::scripts_dir;
use super::protocol::{read_message, write_message, Method, Request, Response};
use ripht_php_sapi::{RiphtSapi, WebRequest};
use serde_bytes::ByteBuf;
use std::io::{self, BufReader, BufWriter};

pub fn maybe_run_worker() {
    if std::env::var("SAPI_WORKER").is_ok() {
        run_worker_loop();
        std::process::exit(0);
    }
}

fn run_worker_loop() {
    let sapi = RiphtSapi::instance();

    let stdin = io::stdin().lock();
    let stdout = io::stdout().lock();

    let mut reader = BufReader::new(stdin);
    let mut writer = BufWriter::new(stdout);

    loop {
        let req: Request = match read_message(&mut reader) {
            Ok(r) => r,
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => {
                let resp = Response::Error {
                    message: format!("Failed to read request: {}", e),
                };
                let _ = write_message(&mut writer, &resp);
                continue;
            }
        };

        let resp = handle_request(&sapi, req);

        if let Err(e) = write_message(&mut writer, &resp) {
            eprintln!("Worker: failed to write response: {}", e);
            break;
        }
    }
}

fn handle_request(sapi: &RiphtSapi, req: Request) -> Response {
    match req {
        Request::Exec {
            method,
            script,
            uri,
            content_type,
            body,
        } => execute_php(sapi, &script, method, uri, content_type, body),

        Request::Echo { size } => Response::Echo { size },

        Request::Shutdown => {
            std::process::exit(0);
        }
    }
}

fn execute_php(
    sapi: &RiphtSapi,
    script: &str,
    method: Method,
    uri: Option<String>,
    content_type: Option<String>,
    body: Option<ByteBuf>,
) -> Response {
    let script_path = scripts_dir().join(script);

    if !script_path.exists() {
        return Response::Error {
            message: format!("Script not found: {}", script_path.display()),
        };
    }

    let mut builder = match method {
        Method::Get => WebRequest::get(),
        Method::Post => WebRequest::post(),
        Method::Put => WebRequest::put(),
        Method::Delete => WebRequest::delete(),
        Method::Patch => WebRequest::patch(),
        Method::Head => WebRequest::head(),
        Method::Options => WebRequest::options(),
    };

    if let Some(ref u) = uri {
        builder = builder.with_uri(u);
    }

    if let Some(ref ct) = content_type {
        builder = builder.with_content_type(ct);
    }

    if let Some(ref b) = body {
        builder = builder.with_body(b.to_vec());
        if content_type.is_none() {
            builder = builder.with_content_type("application/octet-stream");
        }
    }

    let ctx = match builder.build(&script_path) {
        Ok(c) => c,
        Err(e) => {
            return Response::Error {
                message: format!("Failed to build context: {}", e),
            };
        }
    };

    match sapi.execute(ctx) {
        Ok(result) => Response::Exec {
            status: result.status_code(),
            body: result.body().to_vec(),
        },
        Err(e) => Response::Error {
            message: format!("Execution failed: {}", e),
        },
    }
}
