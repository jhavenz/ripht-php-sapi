#![allow(static_mut_refs)]

pub mod adapters;
pub mod execution;

mod sapi;

pub use adapters::{
    CliRequest, CliRequestError, Method, WebRequest, WebRequestError,
};

pub use sapi::{ExecutionError, Executor, RiphtSapi, SapiError};

pub use execution::{
    ExecutionContext, ExecutionHooks, ExecutionMessage, ExecutionResult,
    NoOpHooks, OutputAction, StreamingCallback, SyslogLevel,
};

pub mod prelude {
    pub use crate::{
        CliRequest, CliRequestError, ExecutionContext, ExecutionHooks,
        ExecutionMessage, ExecutionResult, Executor, Method, NoOpHooks,
        OutputAction, RiphtSapi, SapiError, StreamingCallback, SyslogLevel,
        WebRequest, WebRequestError,
    };

    #[cfg(feature = "http")]
    pub use crate::{from_http_parts, from_http_request};
}

#[cfg(test)]
use std::path::PathBuf;

#[cfg(feature = "http")]
pub use adapters::{from_http_parts, from_http_request};

#[cfg(test)]
pub fn php_script_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/php_scripts")
        .join(name)
}
