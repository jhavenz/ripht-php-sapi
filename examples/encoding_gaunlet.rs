//! Tests UTF-8, multi-byte character encodings, and emoji handling in query params and POST bodies.
//!
//! Run: `cargo run --example encoding_gaunlet`

use std::path::PathBuf;

use ripht_php_sapi::{RiphtSapi, WebRequest};

fn percent_encode(s: &str) -> String {
    let mut result = String::new();
    for byte in s.bytes() {
        if byte.is_ascii_alphanumeric()
            || byte == b'-'
            || byte == b'_'
            || byte == b'.'
            || byte == b'~'
        {
            result.push(byte as char);
        } else {
            result.push_str(&format!("%{:02X}", byte));
        }
    }
    result
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sapi = RiphtSapi::instance();
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/php_scripts")
        .join("encoding_gaunlet.php");

    println!("=== PHP Encoding Test ===\n");

    let test_strings = [
        ("UTF-8 basics", "Hello, 世界! 🌍"),
        ("Cyrillic", "Привет мир"),
        ("Arabic", "مرحبا"),
        ("Mixed scripts", "Привет مرحبا 你好"),
        ("Emoji sequence", "👨‍👩‍👧‍👦 🤞"),
        ("Special chars", "Tab:\tQuote:\"Backslash:\\"),
    ];

    for (name, data) in test_strings {
        let encoded = percent_encode(data);
        
        let req = WebRequest::get()
            .with_uri(format!("/?data={}", encoded))
            .build(&script)?;
        
        let mut result = sapi.execute(req)?;

        if let Ok(json) =
            serde_json::from_slice::<serde_json::Value>(&result.take_body())
        {
            let received = json["received_query"]
                .as_str()
                .unwrap_or("?");
            
            let ok = received == data;
            
            println!("{:.<25} {}", name, if ok { "OK" } else { "MISMATCH" });
        } else {
            println!("{:.<25} {}", name, result.status_code());
        }
    }

    println!("\nPOST with UTF-8 JSON:");

    let json_body = r#"{"message":"Привет, мир! 🌍","chinese":"你好世界"}"#;

    let req = WebRequest::post()
        .with_content_type("application/json")
        .with_body(json_body.as_bytes().to_vec())
        .build(&script)?;

    let result = sapi.execute(req)?;

    let body_len = result.body().len();
    println!(
        "  Status: {} (body: {} bytes)",
        result.status_code(),
        body_len
    );

    Ok(())
}
