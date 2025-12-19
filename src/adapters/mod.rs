pub mod cli;
pub mod web;

pub use cli::{CliRequest, CliRequestError};
pub use web::{Method, WebRequest, WebRequestError};

#[cfg(feature = "http")]
pub use web::{from_http_parts, from_http_request};
