//! Sends JSON data in POST requests using serde for serialization.
//!
//! Run: `cargo run --example post_json --features serde`

#[cfg(feature = "serde")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::path::PathBuf;

    use ripht_php_sapi::{RiphtSapi, WebRequest};
    use serde_json::json;

    let sapi = RiphtSapi::instance();

    let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/php_scripts")
        .join("post_json.php");

    let data = json!({
        "user_id": 123,
        "action": "update_profile",
        "data": {
            "name": "Jane Smith",
            "email": "jane@example.com"
        }
    });

    let body = serde_json::to_vec(&data)?;

    let req = WebRequest::post()
        .with_content_type("application/json")
        .with_body(body)
        .build(&script_path)?;

    let result = sapi.execute(req)?;

    println!("Status: {}", result.status_code());

    if let Ok(json) =
        serde_json::from_str::<serde_json::Value>(&result.body_string())
    {
        println!("Response:\n{}", serde_json::to_string_pretty(&json)?);
    } else {
        println!("Body: {}", result.body_string());
    }

    Ok(())
}

#[cfg(not(feature = "serde"))]
fn main() {
    eprintln!("This example requires the 'serde' feature");
    eprintln!("Run with: cargo run --example post_json --features serde");
}
