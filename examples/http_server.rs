//! Minimal HTTP server using PHP SAPI to handle requests.
//!
//! Run: `cargo run --example http_server`, then visit http://127.0.0.1:8001

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;

use ripht_php_sapi::{RiphtSapi, WebRequest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let php = RiphtSapi::instance();
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/php_scripts")
        .join("api.php");

    let listener = TcpListener::bind("127.0.0.1:8001")?;
    println!("Server running on http://127.0.0.1:8001");

    for stream in listener.incoming() {
        let mut stream = stream?;
        let mut reader = BufReader::new(&stream);

        let mut request_line = String::new();
        reader.read_line(&mut request_line)?;

        let mut line = String::new();
        loop {
            line.clear();
            reader.read_line(&mut line)?;
            if line.trim().is_empty() {
                break;
            }
        }

        let exec = WebRequest::get().build(&script)?;

        match php.execute(exec) {
            Ok(result) => {
                let body = result.body();
                write!(
                    stream,
                    "HTTP/1.1 {}\r\nContent-Length: {}\r\n\r\n",
                    result.status_code(),
                    body.len()
                )?;
                stream.write_all(&body)?;
            }
            Err(e) => {
                write!(stream, "HTTP/1.1 500\r\n\r\nError: {}", e)?;
            }
        }
    }

    Ok(())
}
