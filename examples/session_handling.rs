//! Demonstrates PHP session management with cookie tracking across requests.
//!
//! Run: `cargo run --example session_handling`

use std::path::PathBuf;

use ripht_php_sapi::{RiphtSapi, WebRequest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sapi = RiphtSapi::instance();

    let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/php_scripts")
        .join("session.php");

    let exec1 = WebRequest::get().build(&script_path)?;
    let result1 = sapi.execute(exec1)?;

    println!("First request - Status: {}", result1.status_code());
    println!("Response: {}", result1.body_string());

    let session_cookie = result1
        .all_headers()
        .find(|h| h.name().eq_ignore_ascii_case("Set-Cookie"))
        .and_then(|h| {
            if h.value().starts_with("PHPSESSID=") {
                h.value().split(';').next().map(|s| s.to_string())
            } else {
                None
            }
        });

    if let Some(cookie_val) = session_cookie {
        println!("Session cookie: {}", cookie_val);

        let exec2 = WebRequest::get()
            .with_raw_cookie_header(&cookie_val)
            .build(&script_path)?;

        let result2 = sapi.execute(exec2)?;
        println!("Second request - Status: {}", result2.status_code());
        println!("Response: {}", result2.body_string());
    } else {
        eprintln!("No session cookie found in response");
    }

    Ok(())
}
