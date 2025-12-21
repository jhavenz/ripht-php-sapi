//! SAPI callback implementations for the Ripht PHP SAPI.
//!
//! # Safety
//!
//! All callbacks share these invariants:
//!
//! - **Threading**: NTS build only. One request executes at a time.
//! - **Context lifetime**: `sapi_globals.server_context` is valid only during
//!   request execution (between `php_request_startup` and `php_request_shutdown`).
//! - **Panic safety**: Callbacks wrap Rust code in `catch_unwind` to prevent
//!   unwinding across FFI.
//! - **Pointer validity**: PHP-provided pointers are valid for the callback duration.

#![allow(clippy::missing_safety_doc)]

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_double, c_int, c_uint, c_void};

#[cfg(feature = "tracing")]
use tracing::{debug, error, info, trace, warn};

use super::ffi;
use super::server_context::ServerContext;
use super::SERVER_SOFTWARE;
use crate::execution::{ExecutionMessage, ResponseHeader};

const HTTP_STATUS_MIN: i32 = 100;
const HTTP_STATUS_MAX: i32 = 599;
const HTTP_STATUS_FALLBACK: u16 = 500;

/// Returns the `ServerContext` pointer if valid. See module docs for safety.
#[inline]
pub(crate) unsafe fn get_context() -> Option<*mut ServerContext> {
    let ptr = ffi::sapi_globals.server_context as *mut ServerContext;
    if ptr.is_null() {
        return None;
    }

    // Alignment check catches corruption
    let align = std::mem::align_of::<ServerContext>();
    if !(ptr as usize).is_multiple_of(align) {
        return None;
    }

    Some(ptr)
}

#[no_mangle]
pub unsafe extern "C" fn ripht_sapi_startup(
    _module: *mut ffi::sapi_module_struct,
) -> c_int {
    ffi::SUCCESS
}

#[no_mangle]
pub unsafe extern "C" fn ripht_sapi_shutdown(
    _module: *mut ffi::sapi_module_struct,
) -> c_int {
    ffi::SUCCESS
}

#[no_mangle]
pub extern "C" fn ripht_sapi_activate() -> c_int {
    ffi::SUCCESS
}

#[no_mangle]
pub extern "C" fn ripht_sapi_deactivate() -> c_int {
    ffi::SUCCESS
}

/// Unbuffered write callback.
#[no_mangle]
pub unsafe extern "C" fn ripht_sapi_ub_write(
    str: *const c_char,
    str_length: usize,
) -> usize {
    if str.is_null() || str_length == 0 {
        return 0;
    }

    if ffi::sapi_globals.headers_sent == 0 {
        ffi::sapi_send_headers();
    }

    let Some(ctx_ptr) = get_context() else {
        return 0;
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let bytes = std::slice::from_raw_parts(str as *const u8, str_length);

        #[cfg(feature = "tracing")]
        trace!(bytes_written = str_length, "Output captured");

        (*ctx_ptr).write_output(bytes)
    }));

    result.unwrap_or(0)
}

/// Flush output callback.
#[no_mangle]
pub unsafe extern "C" fn ripht_sapi_flush(_server_context: *mut c_void) {
    #[cfg(feature = "tracing")]
    trace!("Flush called");

    if ffi::sapi_globals.headers_sent == 0 {
        ffi::sapi_send_headers();
    }

    let Some(ctx_ptr) = get_context() else {
        return;
    };

    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        (*ctx_ptr).flush();
    }));
}

/// Send all response headers callback.
#[no_mangle]
pub unsafe extern "C" fn ripht_sapi_send_headers(
    sapi_headers: *mut ffi::sapi_headers_struct,
) -> c_int {
    if sapi_headers.is_null() {
        return ffi::SAPI_HEADER_SEND_FAILED;
    }

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(ctx_ptr) = get_context() else {
            return ffi::SAPI_HEADER_SEND_FAILED;
        };

        let status = (*sapi_headers).http_response_code;
        let status_code: u16 =
            if !(HTTP_STATUS_MIN..=HTTP_STATUS_MAX).contains(&status) {
                HTTP_STATUS_FALLBACK
            } else {
                status as u16
            };

        (*ctx_ptr).set_status(status_code);
        (*ctx_ptr).response_headers.clear();

        // Iterate PHP's header list
        let mut elem = (*sapi_headers).headers.head;
        while !elem.is_null() {
            let header_ptr = (*elem).data.as_ptr() as *mut ffi::sapi_header_struct;
            ripht_sapi_send_header(header_ptr, std::ptr::null_mut());
            elem = (*elem).next;
        }

        ffi::SAPI_HEADER_SENT_SUCCESSFULLY
    }));

    result.unwrap_or(ffi::SAPI_HEADER_SEND_FAILED)
}

/// Send single response header callback.
#[no_mangle]
pub unsafe extern "C" fn ripht_sapi_send_header(
    sapi_header: *mut ffi::sapi_header_struct,
    _server_context: *mut c_void,
) {
    if sapi_header.is_null() {
        return;
    }

    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(ctx_ptr) = get_context() else {
            return;
        };

        let header_ptr = (*sapi_header).header;
        let header_len = (*sapi_header).header_len;

        if header_ptr.is_null() || header_len == 0 {
            return;
        }

        let bytes = std::slice::from_raw_parts(header_ptr as *const u8, header_len);

        if let Some(h) = ResponseHeader::parse(bytes) {
            (*ctx_ptr).add_header(h);
        }
    }));
}

/// Read POST body callback. May be called multiple times.
#[no_mangle]
pub unsafe extern "C" fn ripht_sapi_read_post(
    buffer: *mut c_char,
    count_bytes: usize,
) -> usize {
    if buffer.is_null() || count_bytes == 0 {
        return 0;
    }

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(ctx_ptr) = get_context() else {
            return 0;
        };

        let slice = std::slice::from_raw_parts_mut(buffer as *mut u8, count_bytes);
        (*ctx_ptr).read_post(slice)
    }));

    result.unwrap_or(0)
}

/// Read cookies callback. Returns pointer to Cookie header value.
#[no_mangle]
pub unsafe extern "C" fn ripht_sapi_read_cookies() -> *mut c_char {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(ctx_ptr) = get_context() else {
            return std::ptr::null_mut();
        };
        (*ctx_ptr).cookie_data_ptr()
    }));

    result.unwrap_or(std::ptr::null_mut())
}

/// Register $_SERVER variables callback.
#[no_mangle]
pub unsafe extern "C" fn ripht_sapi_register_server_variables(
    track_vars_array: *mut ffi::zval,
) {
    if track_vars_array.is_null() {
        return;
    }

    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        register_var_static(track_vars_array, c"SERVER_SOFTWARE", SERVER_SOFTWARE);

        let Some(ctx_ptr) = get_context() else {
            return;
        };

        let ctx = &*ctx_ptr;
        for (name, value) in ctx.server_vars() {
            ffi::php_register_variable_safe(
                name.as_ptr(),
                value.as_ptr(),
                value.as_bytes().len(),
                track_vars_array,
            );
        }
    }));
}

#[inline]
unsafe fn register_var_static(array: *mut ffi::zval, name: &CStr, value: &str) {
    if let Ok(value_cstr) = CString::new(value) {
        ffi::php_register_variable_safe(
            name.as_ptr(),
            value_cstr.as_ptr(),
            value.len(),
            array,
        );
    }
}

#[no_mangle]
pub extern "C" fn ripht_sapi_default_post_reader() {}

/// Parse GET/POST data callback. Delegates to PHP's default.
#[no_mangle]
pub unsafe extern "C" fn ripht_sapi_treat_data(
    arg: c_int,
    str: *mut c_char,
    dest_array: *mut ffi::zval,
) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ffi::php_default_treat_data(arg, str, dest_array);
    }));
}

#[no_mangle]
pub unsafe extern "C" fn ripht_sapi_input_filter(
    _arg: c_int,
    _var: *const c_char,
    _val: *mut *mut c_char,
    _val_len: usize,
    _new_val_len: *mut usize,
) -> c_uint {
    ffi::php_default_input_filter(_arg, _var, _val, _val_len, _new_val_len)
}

/// Log message callback (errors, warnings, notices).
#[no_mangle]
pub unsafe extern "C" fn ripht_sapi_log_message(
    message: *const c_char,
    syslog_type: c_int,
) {
    if message.is_null() {
        return;
    }

    let msg = CStr::from_ptr(message).to_string_lossy();

    let should_log_to_stderr = get_context()
        .map(|ctx_ptr| (*ctx_ptr).log_to_stderr)
        .unwrap_or(false);

    if should_log_to_stderr {
        eprintln!("{}", msg);
    }

    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        #[cfg(feature = "tracing")]
        match syslog_type {
            0..=3 => error!(message = %msg, "PHP error"),
            4 => warn!(message = %msg, "PHP warning"),
            5 => info!(message = %msg, "PHP notice"),
            6 => info!(message = %msg, "PHP info"),
            _ => debug!(message = %msg, "PHP debug"),
        }

        if let Some(ctx_ptr) = get_context() {
            (*ctx_ptr).add_message(ExecutionMessage::from_syslog(
                syslog_type,
                msg.to_string(),
            ));
        }
    }));
}

/// Get request time callback.
#[no_mangle]
pub unsafe extern "C" fn ripht_sapi_get_request_time(
    request_time: *mut c_double,
) -> c_int {
    if request_time.is_null() {
        return ffi::FAILURE;
    }

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        use std::time::{SystemTime, UNIX_EPOCH};

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);

        *request_time = now;
        ffi::SUCCESS
    }));

    result.unwrap_or(ffi::FAILURE)
}

/// PHP's asking for an environment variable
#[no_mangle]
pub unsafe extern "C" fn ripht_sapi_getenv(
    name: *const c_char,
    name_len: usize,
) -> *mut c_char {
    if name.is_null() || name_len == 0 {
        return std::ptr::null_mut();
    }

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let name_bytes = std::slice::from_raw_parts(name as *const u8, name_len);

        if let Some(ctx_ptr) = get_context() {
            if let Some(ptr) = (*ctx_ptr).get_env(name_bytes) {
                return ptr as *mut c_char;
            }
        }

        std::ptr::null_mut()
    }));

    result.unwrap_or(std::ptr::null_mut())
}

#[cfg(test)]
mod tests {
    use crate::{php_script_path, RiphtSapi, WebRequest};

    use super::*;

    unsafe fn get_context_for_test() -> Option<*mut ServerContext> {
        let ptr = super::ffi::sapi_globals.server_context as *mut ServerContext;

        if ptr.is_null() {
            return None;
        }
        let align = std::mem::align_of::<ServerContext>();
        if !(ptr as usize).is_multiple_of(align) {
            return None;
        }
        Some(ptr)
    }

    #[test]
    fn test_ub_write_null_buffer() {
        unsafe {
            let result = ripht_sapi_ub_write(std::ptr::null(), 10);
            assert_eq!(result, 0, "ub_write should return 0 for null buffer");
        }
    }

    #[test]
    fn test_ub_write_zero_length() {
        unsafe {
            let data = b"test";
            let result = ripht_sapi_ub_write(data.as_ptr() as *const c_char, 0);
            assert_eq!(result, 0, "ub_write should return 0 for zero length");
        }
    }

    #[test]
    fn test_log_message_all_levels() {
        unsafe {
            for level in 0..=7 {
                let msg = CString::new(format!("Test message level {}", level))
                    .unwrap();

                ripht_sapi_log_message(msg.as_ptr(), level);
            }
        }
    }

    #[test]
    fn test_get_request_time_null_pointer() {
        unsafe {
            let result = ripht_sapi_get_request_time(std::ptr::null_mut());
            assert_eq!(
                result,
                super::ffi::FAILURE,
                "get_request_time should return FAILURE for null pointer"
            );
        }
    }

    #[test]
    fn test_get_request_time_valid_pointer() {
        unsafe {
            let mut request_time: f64 = 0.0;
            let result = ripht_sapi_get_request_time(&mut request_time);

            assert_eq!(
                result,
                super::ffi::SUCCESS,
                "get_request_time should return SUCCESS for valid pointer"
            );

            assert!(
                request_time > 0.0,
                "request_time should be set to a positive value"
            );
        }
    }

    #[test]
    fn test_get_context_null_handling() {
        unsafe {
            super::ffi::sapi_globals.server_context = std::ptr::null_mut();

            let result = get_context_for_test();

            assert!(
                result.is_none(),
                "get_context should return None for null pointer"
            );
        }
    }

    #[test]
    fn test_get_context_alignment_check() {
        unsafe {
            let ctx = Box::new(ServerContext::new());
            let ctx_ptr = Box::into_raw(ctx);

            super::ffi::sapi_globals.server_context =
                ctx_ptr as *mut std::ffi::c_void;

            let result = get_context_for_test();

            assert!(
                result.is_some(),
                "get_context should return Some for valid aligned pointer"
            );

            let _ = Box::from_raw(ctx_ptr);
            super::ffi::sapi_globals.server_context = std::ptr::null_mut();
        }
    }

    #[test]
    fn test_read_post_bounds() {
        let php = RiphtSapi::instance();
        let script_path = php_script_path("hello.php");

        let post_data = b"test data".to_vec();

        let exec = WebRequest::post()
            .with_content_type("application/x-www-form-urlencoded")
            .with_body(post_data)
            .build(&script_path)
            .expect("failed to build POST WebRequest");

        let _result = php
            .execute(exec)
            .expect("POST request execution failed");

        unsafe {
            let mut buffer = vec![0u8; 100];
            let result = ripht_sapi_read_post(
                buffer.as_mut_ptr() as *mut c_char,
                buffer.len(),
            );
            assert!(
                result <= buffer.len(),
                "read_post should not exceed buffer size"
            );
        }
    }

    #[test]
    fn test_send_headers_null_pointer() {
        unsafe {
            let result = ripht_sapi_send_headers(std::ptr::null_mut());
            assert_eq!(
                result,
                super::ffi::SAPI_HEADER_SEND_FAILED,
                "send_headers should return SAPI_HEADER_SEND_FAILED for null pointer"
            );
        }
    }

    #[test]
    fn test_send_headers_invalid_status() {
        let php = RiphtSapi::instance();

        let script_path = php_script_path("hello.php");

        let exec = WebRequest::get()
            .build(&script_path)
            .expect("failed to build WebRequest");

        let _result = php
            .execute(exec)
            .expect("hello.php request execution failed");

        unsafe {
            let ctx = Box::new(ServerContext::new());
            let ctx_ptr = Box::into_raw(ctx);
            super::ffi::sapi_globals.server_context =
                ctx_ptr as *mut std::ffi::c_void;

            let mut headers = super::ffi::sapi_headers_struct {
                http_response_code: -1,
                ..Default::default()
            };
            let result = ripht_sapi_send_headers(&mut headers);
            assert_eq!(
                result,
                super::ffi::SAPI_HEADER_SENT_SUCCESSFULLY,
                "send_headers should handle invalid status codes gracefully"
            );

            headers.http_response_code = 99999;
            let result2 = ripht_sapi_send_headers(&mut headers);
            assert_eq!(
                result2,
                super::ffi::SAPI_HEADER_SENT_SUCCESSFULLY,
                "send_headers should handle out-of-range status codes gracefully"
            );

            let _ = Box::from_raw(ctx_ptr);
            super::ffi::sapi_globals.server_context = std::ptr::null_mut();
        }
    }

    #[test]
    fn test_header_parsing_whitespace() {
        let php = RiphtSapi::instance();
        let script_path = php_script_path("hello.php");

        let exec = WebRequest::get()
            .build(&script_path)
            .expect("failed to build WebRequest");
        let _result = php
            .execute(exec)
            .expect("hello.php request execution failed");

        unsafe {
            let mut header = super::ffi::sapi_header_struct::default();
            let header_str = b"Content-Type:   application/json\r\n";
            header.header = header_str.as_ptr() as *mut c_char;
            header.header_len = header_str.len();

            ripht_sapi_send_header(&mut header, std::ptr::null_mut());

            let header_str2 = b"X-Custom-Header:  value with spaces  \r\n";
            header.header = header_str2.as_ptr() as *mut c_char;
            header.header_len = header_str2.len();

            ripht_sapi_send_header(&mut header, std::ptr::null_mut());
        }
    }

    #[test]
    fn test_header_parsing_no_colon() {
        let php = RiphtSapi::instance();
        let script_path = php_script_path("hello.php");

        let exec = WebRequest::get()
            .build(&script_path)
            .expect("failed to build WebRequest");

        let _result = php
            .execute(exec)
            .expect("hello.php request execution failed");

        unsafe {
            let ctx = Box::new(ServerContext::new());
            let ctx_ptr = Box::into_raw(ctx);
            super::ffi::sapi_globals.server_context =
                ctx_ptr as *mut std::ffi::c_void;

            let mut header = super::ffi::sapi_header_struct::default();
            let header_str = b"InvalidHeaderNoColon\r\n";
            header.header = header_str.as_ptr() as *mut c_char;
            header.header_len = header_str.len();

            ripht_sapi_send_header(&mut header, std::ptr::null_mut());

            assert_eq!(
                (*ctx_ptr).response_headers.len(),
                0,
                "Header with no colon should not be added"
            );

            let _ = Box::from_raw(ctx_ptr);
            super::ffi::sapi_globals.server_context = std::ptr::null_mut();
        }
    }

    #[test]
    fn test_header_parsing_colon_at_start() {
        let php = RiphtSapi::instance();
        let script_path = php_script_path("hello.php");

        let exec = WebRequest::get()
            .build(&script_path)
            .expect("failed to build WebRequest");

        let _result = php
            .execute(exec)
            .expect("hello.php request execution failed");

        unsafe {
            let ctx = Box::new(ServerContext::new());
            let ctx_ptr = Box::into_raw(ctx);
            super::ffi::sapi_globals.server_context =
                ctx_ptr as *mut std::ffi::c_void;

            let mut header = super::ffi::sapi_header_struct::default();
            let header_str = b": value\r\n";
            header.header = header_str.as_ptr() as *mut c_char;
            header.header_len = header_str.len();

            ripht_sapi_send_header(&mut header, std::ptr::null_mut());

            assert_eq!(
                (*ctx_ptr).response_headers.len(),
                0,
                "Header with colon at start (empty name) should not be added"
            );

            let _ = Box::from_raw(ctx_ptr);
            super::ffi::sapi_globals.server_context = std::ptr::null_mut();
        }
    }

    #[test]
    fn test_header_parsing_empty_value() {
        let php = RiphtSapi::instance();
        let script_path = php_script_path("hello.php");

        let exec = WebRequest::get()
            .build(&script_path)
            .expect("failed to build WebRequest");
        let _result = php
            .execute(exec)
            .expect("hello.php request execution failed");

        unsafe {
            let ctx = Box::new(ServerContext::new());
            let ctx_ptr = Box::into_raw(ctx);
            super::ffi::sapi_globals.server_context =
                ctx_ptr as *mut std::ffi::c_void;

            let mut header = super::ffi::sapi_header_struct::default();
            let header_str = b"X-Empty-Value:\r\n";
            header.header = header_str.as_ptr() as *mut c_char;
            header.header_len = header_str.len();

            ripht_sapi_send_header(&mut header, std::ptr::null_mut());

            let headers_len = (*ctx_ptr).response_headers.len();

            assert_eq!(
                headers_len, 1,
                "Header with empty value should be added"
            );

            let headers = &(*ctx_ptr).response_headers;
            assert_eq!(headers[0].name(), "X-Empty-Value");
            assert_eq!(headers[0].value(), "");

            let _ = Box::from_raw(ctx_ptr);
            super::ffi::sapi_globals.server_context = std::ptr::null_mut();
        }
    }

    #[test]
    fn test_header_parsing_colon_at_end() {
        let php = RiphtSapi::instance();
        let script_path = php_script_path("hello.php");

        let exec = WebRequest::get()
            .build(&script_path)
            .expect("failed to build WebRequest");

        let _result = php
            .execute(exec)
            .expect("hello.php request execution failed");

        unsafe {
            let ctx = Box::new(ServerContext::new());
            let ctx_ptr = Box::into_raw(ctx);
            super::ffi::sapi_globals.server_context =
                ctx_ptr as *mut std::ffi::c_void;

            let mut header = super::ffi::sapi_header_struct::default();
            let header_str = b"X-Trailing-Colon:\r\n";
            header.header = header_str.as_ptr() as *mut c_char;
            header.header_len = header_str.len();

            ripht_sapi_send_header(&mut header, std::ptr::null_mut());

            assert_eq!(
                (*ctx_ptr).response_headers.len(),
                1,
                "Header with colon at end should be added with empty value"
            );

            let _ = Box::from_raw(ctx_ptr);
            super::ffi::sapi_globals.server_context = std::ptr::null_mut();
        }
    }
}
