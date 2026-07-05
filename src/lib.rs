//! Rust bindings to PHP's Server API (SAPI) for embedding PHP in Rust applications.
//!
//! This crate provides a safe interface to execute PHP scripts from Rust, handling
//! the request lifecycle, I/O callbacks, and superglobal population automatically.
//!
//! # Execution Model
//!
//! PHP runs in NTS (non-thread-safe) mode with one request executing at a time.
//! Each request gets isolated state that's cleaned up after execution.
//!
//! # Example
//!
//! ```no_run
//! use ripht_php_sapi::{RiphtSapi, WebRequest};
//!
//! let php = RiphtSapi::instance();
//! let script = std::path::Path::new("/var/www/index.php");
//!
//! let request = WebRequest::get()
//!     .with_uri("/api/users?id=42")
//!     .build(script)
//!     .expect("...");
//!
//! let result = php.execute(request).expect("execute");
//! println!("Status: {}, Body: {}", result.status_code(), result.body_string());
//! ```

#![allow(static_mut_refs)]

pub mod adapters;
pub mod execution;

mod sapi;

pub use adapters::{
    AdapterError, CliRequest, CliRequestError, Method, PhpSapiAdapter,
    WebRequest, WebRequestError,
};

pub use sapi::{
    finish_current_request, native, ExecutionError, Executor, RiphtSapi,
    SapiConfig, SapiError,
};

pub use execution::{
    AbortReason, ExecutionContext, ExecutionHooks, ExecutionMessage,
    ExecutionReport, ExecutionResult, NoOpHooks, OutputAction, ResponseHeader,
    ResponseSink, SinkResult, StreamingCallback, SyslogLevel,
};

pub mod prelude {
    pub use crate::{
        finish_current_request, AbortReason, AdapterError, CliRequest,
        CliRequestError, ExecutionContext, ExecutionHooks, ExecutionMessage,
        ExecutionReport, ExecutionResult, Executor, Method, NoOpHooks,
        OutputAction, PhpSapiAdapter, ResponseHeader, ResponseSink, RiphtSapi,
        SapiConfig, SapiError, SinkResult, StreamingCallback, SyslogLevel,
        WebRequest, WebRequestError,
    };

    #[cfg(feature = "http")]
    pub use crate::{from_http_parts, from_http_request};
}

#[cfg(feature = "http")]
pub use adapters::{from_http_parts, from_http_request};

#[cfg(test)]
use std::path::PathBuf;

#[cfg(test)]
pub fn php_script_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/php_scripts")
        .join(name)
}
