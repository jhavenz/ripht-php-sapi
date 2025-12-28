pub mod cli;
pub mod web;

use std::path::Path;

pub use cli::{CliRequest, CliRequestError};
pub use web::{Method, WebRequest, WebRequestError};

#[cfg(feature = "http")]
pub use web::{from_http_parts, from_http_request};

use crate::ExecutionContext;

pub trait PhpSapiAdapter {
    fn build(
        self,
        script_path: impl AsRef<Path>,
    ) -> Result<ExecutionContext, WebRequestError> {
        
    }
}