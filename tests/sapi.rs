use std::path::PathBuf;
use std::sync::Arc;

use ripht_php_sapi::{
    ExecutionContext, ExecutionHooks, OutputAction, RiphtSapi, WebRequest,
};

fn php_script_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/php_scripts")
        .join(name)
}

#[test]
fn execute_hello_php() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("hello.php");

    let exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build WebRequest");

    let result = php.execute(exec);

    match result {
        Ok(resp) => {
            assert!(resp
                .body_string()
                .contains("Hello"));
        }
        Err(e) => {
            panic!("Failed to execute script: {}", e);
        }
    }
}

#[test]
fn post_request_works() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("post_form.php");

    let exec = WebRequest::post()
        .with_content_type("application/x-www-form-urlencoded")
        .with_body(b"name=Jane%20Doe&email=jane%40example.com".to_vec())
        .build(&script_path)
        .expect("failed to build WebRequest");

    let result = php
        .execute(exec)
        .expect("POST request execution failed");

    assert_eq!(result.status_code(), 200);

    let json: serde_json::Value = serde_json::from_str(&result.body_string())
        .expect("failed to parse response body as JSON");

    assert_eq!(json["method"], "POST");
}

#[test]
fn stress_sequential_requests() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("hello.php");

    for i in 0..1000 {
        let exec = WebRequest::get()
            .with_uri(format!("/?i={}", i))
            .build(&script_path)
            .expect("failed to build WebRequest");

        let result = php
            .execute(exec)
            .unwrap_or_else(|_| panic!("request {} execution failed", i));

        assert_eq!(result.status_code(), 200, "Request {} had non-200 status", i);
    }
}

#[test]
fn stress_large_output() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("large_output.php");

    let exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build WebRequest");

    let result = php
        .execute(exec)
        .expect("large output request execution failed");

    assert!(
        result.body().len() >= 1024 * 1024,
        "Expected 1MB+ output, got {} bytes",
        result.body().len()
    );
}

#[test]
fn stress_mixed_methods() {
    let php = RiphtSapi::instance();

    let get_script = php_script_path("get_params.php");
    let post_script = php_script_path("post_form.php");

    for i in 0..500 {
        if i % 2 == 0 {
            let exec = WebRequest::get()
                .with_uri(format!("/?i={}", i))
                .build(&get_script)
                .expect("failed to build GET WebRequest");

            let result = php
                .execute(exec)
                .unwrap_or_else(|_| {
                    panic!("GET request {} execution failed", i)
                });

            assert_eq!(result.status_code(), 200);
        } else {
            let exec = WebRequest::post()
                .with_uri(format!("/post?i={}", i))
                .with_content_type("application/x-www-form-urlencoded")
                .with_body(b"name=test&value=123".to_vec())
                .build(&post_script)
                .expect("failed to build POST WebRequest");

            let result = php
                .execute(exec)
                .unwrap_or_else(|_| {
                    panic!("POST request {} execution failed", i)
                });

            assert_eq!(result.status_code(), 200);
        }
    }
}

#[test]
fn test_context_isolation_between_requests() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("server_vars.php");

    for i in 0..5 {
        let exec = WebRequest::get()
            .with_uri(format!("/test?request={}", i))
            .with_server_name(format!("server{}", i))
            .with_remote_addr(format!("127.0.0.{}", i))
            .build(&script_path)
            .expect("failed to build WebRequest");

        let result = php
            .execute(exec)
            .unwrap_or_else(|_| panic!("request {} execution failed", i));

        assert_eq!(result.status_code(), 200);

        let json: serde_json::Value =
            serde_json::from_str(&result.body_string())
                .expect("failed to parse response body as JSON");
        assert_eq!(
            json["server_name"],
            format!("server{}", i),
            "Request {} should have correct server_name",
            i
        );
    }
}

#[test]
fn test_cstring_pointer_validity_during_execution() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("post_form.php");

    let query_string = "foo=bar&baz=qux";
    let exec = WebRequest::post()
        .with_uri(format!("/test?{}", query_string))
        .with_content_type("application/x-www-form-urlencoded")
        .with_body(b"name=test".to_vec())
        .with_raw_cookie_header("session=abc123")
        .build(&script_path)
        .expect("failed to build WebRequest");

    let result = php
        .execute(exec)
        .expect("POST request execution failed");

    assert_eq!(result.status_code(), 200);
}

#[test]
fn test_post_data_bounds_with_real_script() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("large_input.php");

    let test_sizes = vec![0, 1, 100, 1024, 10 * 1024, 100 * 1024, 1024 * 1024];

    for size in test_sizes {
        let post_data = vec![b'x'; size];
        let exec = WebRequest::post()
            .with_content_type("application/octet-stream")
            .with_body(post_data)
            .build(&script_path)
            .expect("failed to build WebRequest");

        let result = php
            .execute(exec)
            .unwrap_or_else(|_| {
                panic!("request with {} bytes execution failed", size)
            });

        assert_eq!(result.status_code(), 200);

        let json: serde_json::Value =
            serde_json::from_str(&result.body_string())
                .expect("failed to parse response body as JSON");

        assert_eq!(
            json["input_length"]
                .as_u64()
                .unwrap() as usize,
            size,
            "Input length should match for size {}",
            size
        );
    }
}

#[test]
fn test_header_parsing_with_real_php_headers() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("headers.php");

    let exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build WebRequest");

    let result = php
        .execute(exec)
        .expect("headers.php request execution failed");

    assert_eq!(result.status_code(), 200);

    let content_type = result.header_val("Content-Type");
    if content_type.is_some() {
        assert_eq!(
            content_type,
            Some("application/json"),
            "Content-Type should be application/json if present"
        );
    }

    let has_custom_header = result
        .header_val("X-Custom-Header")
        .is_some()
        || result
            .header_val("x-custom-header")
            .is_some();

    if has_custom_header {
        assert_eq!(
            result
                .header_val("X-Custom-Header")
                .or_else(|| result.header_val("x-custom-header")),
            Some("test-value"),
            "X-Custom-Header should be set if headers are captured"
        );
    }
}

#[test]
fn test_error_handling_with_errors_script() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("errors.php");

    let exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build WebRequest");

    let result = php
        .execute(exec)
        .expect("errors.php request execution failed");

    assert_eq!(result.status_code(), 200);

    assert!(
        result.all_messages().any(|_| true),
        "Response should contain messages from error_log() and trigger_error()"
    );

    let has_error = result
        .all_messages()
        .any(|e| {
            e.message
                .contains("Sending an error log")
        });

    assert!(
        has_error,
        "Response should contain error message from error_log()"
    );
}

#[test]
fn test_state_isolation_after_errors() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("hello.php");

    let bad_exec = WebRequest::get().build("/nonexistent/path.php");

    assert!(bad_exec.is_err());

    let good_exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build WebRequest");

    let good_result = php
        .execute(good_exec)
        .expect("request after error path should succeed");

    assert_eq!(good_result.status_code(), 200);
}

#[test]
fn test_sapi_initializes() {
    let php = RiphtSapi::instance();

    assert!(php.is_initialized());
}

#[test]
fn test_file_not_found() {
    let req = WebRequest::get().build("/nonexistent/path.php");

    assert!(req.is_err());
}

#[test]
fn test_get_ini_display_errors() {
    let php = RiphtSapi::instance();
    let _ = php.set_ini("display_errors", "0");

    assert_eq!(php.get_ini("display_errors"), Some("0".into()));
}

#[test]
fn test_get_ini_nonexistent() {
    let php = RiphtSapi::instance();

    let value = php.get_ini("this_ini_key_does_not_exist_12345");
    assert!(value.is_none(), "Non-existent INI should return None");
}

#[test]
fn test_set_ini_and_get_ini() {
    let php = RiphtSapi::instance();

    let result = php.set_ini("memory_limit", "256M");
    assert!(result.is_ok(), "set_ini should succeed for valid INI key");

    let value = php.get_ini("memory_limit");
    assert!(value.is_some(), "memory_limit should be readable after set");
    assert_eq!(
        value.unwrap(),
        "256M",
        "memory_limit should reflect the set value"
    );
}

#[test]
fn test_set_ini_invalid_key() {
    let php = RiphtSapi::instance();

    let result = php.set_ini("key\0with\0nulls", "value");
    assert!(
        result.is_err(),
        "set_ini should fail for key with null bytes"
    );
}

#[test]
fn test_set_ini_invalid_value() {
    let php = RiphtSapi::instance();

    let result = php.set_ini("memory_limit", "value\0with\0nulls");
    assert!(
        result.is_err(),
        "set_ini should fail for value with null bytes"
    );
}

#[test]
fn test_execution_error_script_not_found() {
    let php = RiphtSapi::instance();

    let ctx = ExecutionContext::script("/nonexistent/path/to/script.php");
    let result = php.execute(ctx);

    assert!(
        result.is_err(),
        "execute should fail for nonexistent script"
    );
    let err = result.unwrap_err();
    assert!(
        err.to_string()
            .contains("not found"),
        "Error should mention script not found"
    );
}

#[test]
fn test_multipart_upload_basic() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("file_upload.php");

    let boundary = "boundary123";
    let body = format!(
        "--{boundary}\r\n\
Content-Disposition: form-data; name=\"field\"\r\n\r\n\
test_value\r\n\
--{boundary}--\r\n",
        boundary = boundary
    );

    let exec = WebRequest::post()
        .with_content_type(format!(
            "multipart/form-data; boundary={}",
            boundary
        ))
        .with_body(body.into_bytes())
        .build(&script_path)
        .expect("failed to build multipart WebRequest");

    let result = php
        .execute(exec)
        .expect("multipart POST request execution failed");
    assert_eq!(result.status_code(), 200);

    let body_str = result.body_string();
    let json: serde_json::Value = serde_json::from_str(&body_str)
        .unwrap_or_else(|_| {
            panic!("failed to parse JSON response: {}", body_str)
        });

    assert_eq!(
        json["post_data"]["field"], "test_value",
        "POST field should be 'test_value': {}",
        body_str
    );
}

#[test]
fn test_multipart_upload_with_file() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("file_upload.php");

    let boundary = {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("----multipart-form-boundary-{:x}", timestamp)
    };
    let file_content = "Hello, this is test file content!";
    let body = format!(
        "--{boundary}\r\n\
Content-Disposition: form-data; name=\"myfile\"; filename=\"test.txt\"\r\n\
Content-Type: text/plain\r\n\
\r\n\
{file_content}\r\n\
--{boundary}\r\n\
Content-Disposition: form-data; name=\"description\"\r\n\
\r\n\
A test file\r\n\
--{boundary}--\r\n",
        boundary = boundary,
        file_content = file_content
    );

    let exec = WebRequest::post()
        .with_content_type(format!(
            "multipart/form-data; boundary={}",
            boundary
        ))
        .with_body(body.into_bytes())
        .build(&script_path)
        .expect("failed to build file upload WebRequest");

    let result = php
        .execute(exec)
        .expect("file upload request execution failed");
    assert_eq!(result.status_code(), 200);

    let json: serde_json::Value = serde_json::from_slice(&result.body())
        .expect("failed to parse response body as JSON");

    assert_eq!(
        json["post_data"]["description"], "A test file",
        "POST field 'description' should be set"
    );

    assert!(
        json["files"]["myfile"].is_object(),
        "FILES should contain 'myfile' entry"
    );

    let file_entry = &json["files"]["myfile"];
    assert_eq!(file_entry["name"], "test.txt");
    assert_eq!(file_entry["error"], 0);

    assert_eq!(file_entry["tmp_exists"], true, "Temp file should exist");
    assert_eq!(
        file_entry["tmp_readable"], true,
        "Temp file should be readable"
    );

    assert_eq!(
        file_entry["tmp_content"], file_content,
        "Temp file content should match uploaded content"
    );
    assert_eq!(
        file_entry["tmp_content_length"],
        file_content.len(),
        "Temp file size should match uploaded content length"
    );
}

#[test]
fn test_multipart_upload_temp_file_creation() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("file_upload.php");

    let boundary = "boundary456";
    let file_content = "Test file content for temp file verification";
    let body = format!(
        "--{boundary}\r\n\
Content-Disposition: form-data; name=\"upload\"; filename=\"verify.txt\"\r\n\
Content-Type: text/plain\r\n\
\r\n\
{file_content}\r\n\
--{boundary}--\r\n",
        boundary = boundary,
        file_content = file_content
    );

    let exec = WebRequest::post()
        .with_content_type(format!(
            "multipart/form-data; boundary={}",
            boundary
        ))
        .with_body(body.into_bytes())
        .build(&script_path)
        .expect("failed to build temp file upload WebRequest");

    let result = php
        .execute(exec)
        .expect("temp file upload request execution failed");
    assert_eq!(result.status_code(), 200);

    let json: serde_json::Value = serde_json::from_slice(&result.body())
        .expect("failed to parse response body as JSON");

    let upload_tmp_dir = json["upload_tmp_dir"]
        .as_str()
        .unwrap_or("");

    let file_entry = &json["files"]["upload"];
    assert!(
        file_entry["tmp_name"].is_string(),
        "Temp file name should be set in $_FILES"
    );

    let tmp_name = file_entry["tmp_name"]
        .as_str()
        .expect("tmp_name field should be a string");

    if !upload_tmp_dir.is_empty() {
        let normalized_tmp_dir = upload_tmp_dir.trim_end_matches('/');
        let normalized_tmp_name = tmp_name.trim_end_matches('/');
        let normalized_tmp_dir_alt = normalized_tmp_dir.replace("/private", "");

        assert!(
            normalized_tmp_name.starts_with(normalized_tmp_dir)
                || normalized_tmp_name.starts_with(&normalized_tmp_dir_alt)
                || normalized_tmp_name
                    .replace("/private", "")
                    .starts_with(&normalized_tmp_dir_alt),
            "Temp file should be in upload_tmp_dir: {} (upload_tmp_dir: {})",
            tmp_name,
            upload_tmp_dir
        );
    } else {
        assert!(
            tmp_name.contains("/tmp/")
                || tmp_name.contains("\\tmp\\")
                || tmp_name.contains("/var/folders"),
            "Temp file should be in a temp directory: {}",
            tmp_name
        );
    }

    assert_eq!(
        file_entry["tmp_content"], file_content,
        "Temp file content should match uploaded content"
    );
}

#[test]
fn test_session_basic() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("session.php");

    let exec1 = WebRequest::get()
        .build(&script_path)
        .expect("failed to build first session WebRequest");
    let result1 = php
        .execute(exec1)
        .expect("first session request execution failed");
    assert_eq!(result1.status_code(), 200);

    let body1 = result1.body_string();
    assert!(
        body1.contains("session_id"),
        "Response should contain session_id: {}",
        body1
    );
    assert!(
        body1.contains("\"visit_count\":1")
            || body1.contains("\"visit_count\": 1"),
        "First request should have visit_count=1: {}",
        body1
    );

    let session_cookie = result1
        .all_headers()
        .find(|h| {
            h.name()
                .eq_ignore_ascii_case("Set-Cookie")
        })
        .and_then(|h| {
            if h.value()
                .starts_with("PHPSESSID=")
            {
                h.value()
                    .split(';')
                    .next()
                    .map(|s| s.to_string())
            } else {
                None
            }
        });

    if let Some(cookie_val) = session_cookie {
        let exec2 = WebRequest::get()
            .with_raw_cookie_header(&cookie_val)
            .build(&script_path)
            .expect("failed to build second session WebRequest");
        let result2 = php
            .execute(exec2)
            .expect("second session request execution failed");

        let body2 = result2.body_string();
        assert!(
            body2.contains("\"visit_count\":2")
                || body2.contains("\"visit_count\": 2"),
            "Second request should have visit_count=2: {}",
            body2
        );
    }
}

#[test]
fn test_head_request_method() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("hello.php");

    let exec = WebRequest::head()
        .build(&script_path)
        .expect("failed to build HEAD WebRequest");
    let result = php
        .execute(exec)
        .expect("HEAD request execution failed");

    assert_eq!(result.status_code(), 200);
}

#[test]
fn test_options_request_method() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("hello.php");

    let exec = WebRequest::options()
        .build(&script_path)
        .expect("failed to build OPTIONS WebRequest");
    let result = php
        .execute(exec)
        .expect("OPTIONS request execution failed");
    assert_eq!(result.status_code(), 200);
}

#[test]
fn test_streaming_sse_output() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("streaming.php");

    let chunks = Arc::new(std::sync::Mutex::new(Vec::<Vec<u8>>::new()));
    let chunks_clone = Arc::clone(&chunks);

    let exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build streaming WebRequest");
    let result = php
        .execute_streaming(exec, move |chunk| {
            chunks_clone
                .lock()
                .unwrap()
                .push(chunk.to_vec());
        })
        .expect("SSE streaming request execution failed");

    assert_eq!(result.status_code(), 200);

    let received_chunks = chunks.lock().unwrap();
    assert!(
        received_chunks.len() > 1,
        "Should receive multiple chunks, got {}",
        received_chunks.len()
    );

    assert!(
        result.body().is_empty(),
        "Response body should be empty when streaming (data sent to callback)"
    );

    let combined: Vec<u8> = received_chunks
        .iter()
        .flat_map(|c| c.iter().copied())
        .collect();
    let combined_str = String::from_utf8_lossy(&combined);
    assert!(
        combined_str.contains("Chunk 1"),
        "Streamed output should contain Chunk 1"
    );
    assert!(
        combined_str.contains("Chunk 5"),
        "Streamed output should contain Chunk 5"
    );
    assert!(
        combined_str.contains("[DONE]"),
        "Streamed output should contain [DONE]"
    );
}

#[test]
fn test_streaming_large_output() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("large_output.php");

    let chunks = Arc::new(std::sync::Mutex::new(Vec::<Vec<u8>>::new()));
    let chunks_clone = Arc::clone(&chunks);

    let exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build large output streaming WebRequest");
    let result = php
        .execute_streaming(exec, move |chunk| {
            chunks_clone
                .lock()
                .unwrap()
                .push(chunk.to_vec());
        })
        .expect("large output streaming request execution failed");

    assert_eq!(result.status_code(), 200);

    assert!(
        result.body().is_empty(),
        "Response body should be empty when streaming"
    );

    let received_chunks = chunks.lock().unwrap();
    assert!(
        !received_chunks.is_empty(),
        "Should receive at least one chunk"
    );

    let callback_total: usize = received_chunks
        .iter()
        .map(|c| c.len())
        .sum();
    assert!(
        callback_total >= 1024 * 1024,
        "Should receive 1MB+ via callback, got {} bytes",
        callback_total
    );

    drop(received_chunks);

    let exec2 = WebRequest::get()
        .build(&script_path)
        .expect("failed to build non-streaming WebRequest");
    let result2 = php
        .execute(exec2)
        .expect("non-streaming large output request execution failed");
    assert!(
        result2.body().len() >= 1024 * 1024,
        "Non-streaming should buffer the full output: {} bytes",
        result2.body().len()
    );
}

// Tests for internal SAPI state (post_read flag, server_context cleanup)
// have been moved to src/sapi/callbacks.rs unit tests since they require
// access to internal FFI types.

#[test]
fn test_header_edge_cases_duplicate_set_cookie_headers() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("header_edge_cases.php");

    let exec = WebRequest::get()
        .with_uri("/header_edge_cases.php?test=duplicate")
        .build(&script_path)
        .expect("failed to build header edge cases WebRequest");

    let result = php
        .execute(exec)
        .expect("header edge cases (duplicate) request execution failed");

    let set_cookies = result.header_vals("Set-Cookie");
    assert_eq!(
        set_cookies.len(),
        3,
        "Expected 3 Set-Cookie headers, got {:?}",
        set_cookies
    );

    assert!(set_cookies
        .iter()
        .any(|v| v.contains("a=1")));
    assert!(set_cookies
        .iter()
        .any(|v| v.contains("b=2")));
    assert!(set_cookies
        .iter()
        .any(|v| v.contains("c=3")));
}

#[test]
fn test_header_edge_cases_header_remove() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("header_edge_cases.php");

    let exec = WebRequest::get()
        .with_uri("/header_edge_cases.php?test=remove")
        .build(&script_path)
        .expect("failed to build header remove WebRequest");

    let result = php
        .execute(exec)
        .expect("header edge cases (remove) request execution failed");

    assert!(
        result
            .header_val("X-To-Remove")
            .is_none(),
        "X-To-Remove should not be present after header_remove()"
    );

    let kept = result
        .header_val("X-Kept")
        .map(|v| v.contains("still here"))
        .unwrap_or(false);
    assert!(kept, "X-Kept should be present");
}

#[test]
fn test_status_codes_and_redirect_location_header() {
    let php = RiphtSapi::instance();

    let status_script = php_script_path("status_codes.php");
    let exec_201 = WebRequest::get()
        .with_uri("/status_codes.php?code=201&method=code")
        .build(&status_script)
        .expect("failed to build status 201 WebRequest");

    let result_201 = php
        .execute(exec_201)
        .expect("status_codes.php (201) request execution failed");
    assert_eq!(result_201.status_code(), 201);

    let exec_307 = WebRequest::get()
        .with_uri("/status_codes.php?code=307&method=header")
        .build(&status_script)
        .expect("failed to build status 307 WebRequest");

    let result_307 = php
        .execute(exec_307)
        .expect("status_codes.php (307) request execution failed");
    assert_eq!(result_307.status_code(), 307);

    let redirect_script = php_script_path("redirect_handling.php");
    let exec_redirect = WebRequest::get()
        .with_uri("/redirect_handling.php?type=301")
        .build(&redirect_script)
        .expect("failed to build redirect WebRequest");

    let result_redirect = php
        .execute(exec_redirect)
        .expect("redirect_handling.php request execution failed");

    assert_eq!(result_redirect.status_code(), 301);

    let location = result_redirect
        .header_val("Location")
        .expect("redirect response missing Location header");
    assert!(
        location.contains("/redirected.php"),
        "Expected Location to contain /redirected.php, got: {}",
        location
    );
}

#[test]
fn test_binary_output_byte_integrity() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("binary_output.php");

    let exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build binary output WebRequest");

    let result = php
        .execute(exec)
        .expect("binary output request execution failed");

    assert_eq!(result.body().len(), 256, "Expected 256 bytes");
    assert_eq!(result.body()[0], 0);
    assert_eq!(result.body()[1], 1);
    assert_eq!(result.body()[255], 255);
}

#[test]
fn test_webrequest_shaping_via_superglobals() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("superglobals.php");

    let body = b"raw-body-123".to_vec();
    let path_info = "/extra/path";
    let document_root = std::env::temp_dir().join("ripht_sapi_docroot");

    let exec = WebRequest::post()
        .with_uri("/superglobals.php?alpha=1")
        .with_header("X-Foo-Bar", "baz")
        .with_https(true)
        .with_document_root(&document_root)
        .with_path_info(path_info)
        // Intentionally omit Content-Type + Content-Length to exercise defaults
        .with_body(body.clone())
        .build(&script_path)
        .expect("failed to build superglobals WebRequest");

    let result = php
        .execute(exec)
        .expect("superglobals.php request execution failed");

    let json: serde_json::Value = serde_json::from_slice(&result.body())
        .expect("failed to parse superglobals response as JSON");

    // Header mapping: X-Foo-Bar => HTTP_X_FOO_BAR => appears as X_FOO_BAR in HTTP_HEADERS
    assert_eq!(json["HTTP_HEADERS"]["X_FOO_BAR"], "baz");

    // HTTPS shaping
    assert_eq!(json["SERVER"]["REQUEST_SCHEME"], "https");
    assert_eq!(json["SERVER"]["HTTPS"], "on");

    // PATH_INFO / PATH_TRANSLATED shaping
    assert_eq!(json["SERVER"]["PATH_INFO"], path_info);
    let expected_translated =
        format!("{}{}", document_root.to_string_lossy(), path_info);
    assert_eq!(json["SERVER"]["PATH_TRANSLATED"], expected_translated);

    // Default Content-Type / Content-Length behavior when body is present
    assert_eq!(json["SERVER"]["CONTENT_TYPE"], "application/octet-stream");
    assert_eq!(json["SERVER"]["CONTENT_LENGTH"], body.len().to_string());
}

#[test]
fn test_execute_with_hooks_can_filter_headers_and_handle_output() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("headers.php");

    struct FilterAndCaptureHooks {
        captured: Arc<std::sync::Mutex<Vec<u8>>>,
    }

    impl ExecutionHooks for FilterAndCaptureHooks {
        fn on_header(&mut self, name: &str, _value: &str) -> bool {
            !name.eq_ignore_ascii_case("X-Another-Header")
        }

        fn on_output(&mut self, data: &[u8]) -> OutputAction {
            self.captured
                .lock()
                .unwrap()
                .extend_from_slice(data);
            OutputAction::Done
        }
    }

    let captured = Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));

    let exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build hooks test WebRequest");

    let result = php
        .execute_with_hooks(
            exec,
            FilterAndCaptureHooks {
                captured: Arc::clone(&captured),
            },
        )
        .expect("execute_with_hooks() failed");

    assert!(
        result.body().is_empty(),
        "Body should be empty when hooks handle output"
    );

    assert!(result
        .header_val("X-Custom-Header")
        .is_some());
    assert!(
        result
            .header_val("X-Another-Header")
            .is_none(),
        "X-Another-Header should be filtered out by hooks"
    );

    let captured_bytes = captured
        .lock()
        .unwrap()
        .clone();
    assert!(!captured_bytes.is_empty(), "Expected captured output");

    let captured_json: serde_json::Value =
        serde_json::from_slice(&captured_bytes)
            .expect("captured output should be valid JSON");
    assert_eq!(captured_json["method"], "GET");
}

#[test]
fn test_env_vars_visible_via_getenv() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("env_vars.php");

    let exec = WebRequest::get()
        .with_env("TEST_ENV_KEY", "hello-env")
        .build(&script_path)
        .expect("failed to build env vars WebRequest");

    let result = php
        .execute(exec)
        .expect("env_vars.php request execution failed");

    let json: serde_json::Value = serde_json::from_slice(&result.body())
        .expect("failed to parse env vars response as JSON");

    assert_eq!(json["TEST_ENV_KEY"], "hello-env");
    assert!(json["MISSING_ENV_KEY"].is_null());
}

#[test]
fn test_request_scoped_ini_overrides_apply_and_do_not_leak() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("ini_overrides.php");

    let base_exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build baseline INI WebRequest");
    let base_result = php
        .execute(base_exec)
        .expect("ini_overrides.php baseline request execution failed");

    let base_json: serde_json::Value =
        serde_json::from_slice(&base_result.body())
            .expect("failed to parse baseline INI response as JSON");

    let base_display_errors = base_json["display_errors"]
        .as_str()
        .unwrap_or("")
        .to_string();

    // Flip the value to exercise request-scoped ini overrides.
    let override_value = if base_display_errors.is_empty() {
        "1"
    } else {
        "0"
    };

    let exec = WebRequest::get()
        .with_ini("display_errors", override_value)
        .build(&script_path)
        .expect("failed to build INI override WebRequest");

    let result = php
        .execute(exec)
        .expect("ini_overrides.php override request execution failed");

    let json: serde_json::Value = serde_json::from_slice(&result.body())
        .expect("failed to parse INI override response as JSON");

    let got = json["display_errors"]
        .as_str()
        .unwrap_or("");
    if override_value == "0" {
        // For boolean directives PHP may report off as an empty string.
        assert!(
            got.is_empty() || got == "0",
            "Expected display_errors to be off, got: {:?}",
            got
        );
    } else {
        assert_eq!(got, "1");
    }

    // Verify no leak into subsequent requests.
    let after_exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build follow-up INI WebRequest");
    let after_result = php
        .execute(after_exec)
        .expect("ini_overrides.php follow-up request execution failed");

    let after_json: serde_json::Value =
        serde_json::from_slice(&after_result.body())
            .expect("failed to parse follow-up INI response as JSON");

    let after_display_errors = after_json["display_errors"]
        .as_str()
        .unwrap_or("");
    assert_eq!(
        after_display_errors, base_display_errors,
        "display_errors should not leak across requests"
    );
}
