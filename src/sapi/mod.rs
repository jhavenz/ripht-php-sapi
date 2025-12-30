//! Core SAPI implementation and PHP lifecycle management.
//!
//! Handles module startup/shutdown (MINIT/MSHUTDOWN), callback registration,
//! and provides the primary `RiphtSapi` interface for script execution.
//!
//! Adheres to the Common Gateway Interface (CGI) Version 1.1 specification for environment variable semantics.
//!
//! ## Specification Compliance
//!
//! This SAPI implements CGI/1.1 meta-variable conventions as defined in:
//! - [RFC 3875 - The Common Gateway Interface (CGI) Version 1.1](https://datatracker.ietf.org/doc/html/rfc3875)
//!
//! Specifically:
//! - Section 4.1: Request Meta-Variables
//! - Section 4.1.4: `GATEWAY_INTERFACE` set to `CGI/1.1`

use std::ffi::CString;
use std::sync::OnceLock;

use thiserror::Error;

#[cfg(feature = "tracing")]
use tracing::{error, info, trace};

pub(crate) mod callbacks;
mod executor;
pub(crate) mod ffi;
pub(crate) mod server_context;
pub(crate) mod server_vars;

pub use executor::{ExecutionError, Executor};
pub(crate) use server_vars::{ServerVars, ServerVarsCString};

use crate::execution::{ExecutionContext, ExecutionHooks, ExecutionResult};

static PHP_INIT_RESULT: OnceLock<Result<(), SapiError>> = OnceLock::new();

pub(crate) static SAPI_NAME: &[u8] = b"ripht\0";
pub(crate) static SAPI_PRETTY_NAME: &[u8] = b"Ripht PHP SAPI\0";
pub(crate) static SERVER_SOFTWARE: &str =
    concat!("Ripht/", env!("CARGO_PKG_VERSION"));
static INI_ENTRIES: &[u8] = b"\
variables_order=EGPCS\n\
request_order=GP\n\
output_buffering=4096\n\
implicit_flush=0\n\
html_errors=0\n\
display_errors=1\n\
log_errors=1\n\
\0";

/// Errors from SAPI initialization and configuration.
#[derive(Debug, Clone, Error)]
#[non_exhaustive]
pub enum SapiError {
    #[error("PHP engine not initialized")]
    NotInitialized,

    #[error("PHP initialization failed: {0}")]
    InitializationFailed(String),

    #[error("INI key contains null byte")]
    InvalidIniKey,

    #[error("INI value contains null byte")]
    InvalidIniValue,

    #[error("Failed to set INI: {0}")]
    IniSetFailed(String),

    #[error(
        "PHP library not found. Build PHP with --enable-embed=static and set RIPHT_PHP_SAPI_PREFIX"
    )]
    LibraryNotFound,
}

/// PHP SAPI instance. Initialize once, execute scripts repeatedly.
pub struct RiphtSapi {
    _marker: std::marker::PhantomData<*mut ()>,
}

impl RiphtSapi {
    // Note: will panic if initialization fails
    #[must_use]
    pub fn instance() -> Self {
        Self::init().expect("SAPI initialization failure")
    }

    fn init() -> Result<Self, SapiError> {
        let init_result = PHP_INIT_RESULT.get_or_init(|| {
            #[cfg(feature = "tracing")]
            info!("Initializing RiphtSapi");

            // SAFETY: One-time PHP engine initialization via OnceLock.
            // All pointers/callbacks are static or 'static and remain valid.
            unsafe {
                ffi::sapi_module.name = SAPI_NAME.as_ptr() as *mut _;
                ffi::sapi_module.pretty_name =
                    SAPI_PRETTY_NAME.as_ptr() as *mut _;

                // Register callbacks
                ffi::sapi_module.startup = Some(callbacks::ripht_sapi_startup);
                ffi::sapi_module.shutdown =
                    Some(callbacks::ripht_sapi_shutdown);
                ffi::sapi_module.activate =
                    Some(callbacks::ripht_sapi_activate);
                ffi::sapi_module.deactivate =
                    Some(callbacks::ripht_sapi_deactivate);

                ffi::sapi_module.ub_write =
                    Some(callbacks::ripht_sapi_ub_write);
                ffi::sapi_module.flush = Some(callbacks::ripht_sapi_flush);

                ffi::sapi_module.send_headers =
                    Some(callbacks::ripht_sapi_send_headers);
                ffi::sapi_module.send_header =
                    Some(callbacks::ripht_sapi_send_header);

                ffi::sapi_module.read_post =
                    Some(callbacks::ripht_sapi_read_post);
                ffi::sapi_module.read_cookies =
                    Some(callbacks::ripht_sapi_read_cookies);

                ffi::sapi_module.register_server_variables =
                    Some(callbacks::ripht_sapi_register_server_variables);

                ffi::sapi_module.log_message =
                    Some(callbacks::ripht_sapi_log_message);
                ffi::sapi_module.get_request_time =
                    Some(callbacks::ripht_sapi_get_request_time);
                ffi::sapi_module.getenv = Some(callbacks::ripht_sapi_getenv);

                ffi::sapi_module.php_ini_ignore = 0;
                ffi::sapi_module.php_ini_ignore_cwd = 1;

                ffi::sapi_module.input_filter =
                    Some(callbacks::ripht_sapi_input_filter);
                ffi::sapi_module.default_post_reader =
                    Some(callbacks::ripht_sapi_default_post_reader);
                ffi::sapi_module.treat_data =
                    Some(callbacks::ripht_sapi_treat_data);

                ffi::sapi_module.ini_entries = INI_ENTRIES.as_ptr() as *const _;

                #[cfg(feature = "tracing")]
                trace!("Starting SAPI");

                ffi::sapi_startup(&mut ffi::sapi_module);

                #[cfg(feature = "tracing")]
                trace!("Initializing SAPI module");

                let result = ffi::php_module_startup(
                    &mut ffi::sapi_module,
                    std::ptr::null_mut(),
                );

                if result == ffi::FAILURE {
                    #[cfg(feature = "tracing")]
                    error!("SAPI module startup failed");

                    ffi::sapi_shutdown();

                    Err(SapiError::InitializationFailed(
                        "SAPI module initialization failed".to_string(),
                    ))
                } else {
                    #[cfg(feature = "tracing")]
                    info!("SAPI module initialized");
                    Ok(())
                }
            }
        });

        match init_result {
            Ok(()) => Ok(Self {
                _marker: std::marker::PhantomData,
            }),
            // Clone the original error instead of wrapping it redundantly.
            // The error already contains descriptive context.
            Err(e) => Err(e.clone()),
        }
    }

    /// Shuts down the PHP engine. Calling `execute()` after this is undefined behavior.
    pub fn shutdown() {
        unsafe {
            ffi::php_module_shutdown();
            ffi::sapi_shutdown();
        }
    }

    pub fn set_ini(
        &self,
        key: impl Into<Vec<u8>>,
        value: impl Into<Vec<u8>>,
    ) -> Result<(), SapiError> {
        let k_str = key.into();
        let v_str = value.into();

        let key_cstr = CString::new(k_str.clone())
            .map_err(|_| SapiError::InvalidIniKey)?;
        let value_cstr = CString::new(v_str.clone())
            .map_err(|_| SapiError::InvalidIniValue)?;

        unsafe {
            let init = ffi::zend_string_init_interned
                .expect("zend_string_init_interned is null");

            let name = init(key_cstr.as_ptr(), k_str.len(), true);

            if name.is_null() {
                return Err(SapiError::IniSetFailed(
                    String::from_utf8(k_str).unwrap_or_default(),
                ));
            }

            let result = ffi::zend_alter_ini_entry_chars(
                name,
                value_cstr.as_ptr(),
                v_str.len(),
                ffi::ZEND_INI_USER | ffi::ZEND_INI_SYSTEM,
                ffi::ZEND_INI_STAGE_RUNTIME,
            );

            if result != ffi::SUCCESS {
                return Err(SapiError::IniSetFailed(
                    String::from_utf8(k_str).unwrap_or_default(),
                ));
            }

            Ok(())
        }
    }

    pub fn get_ini(&self, key: &str) -> Option<String> {
        #[cfg(feature = "tracing")]
        trace!(ini_key = key, "Getting INI value");

        let key_cstr = CString::new(key).ok()?;

        unsafe {
            let ptr = ffi::zend_ini_string(key_cstr.as_ptr(), key.len(), 0);
            if ptr.is_null() {
                None
            } else {
                Some(
                    std::ffi::CStr::from_ptr(ptr)
                        .to_string_lossy()
                        .into_owned(),
                )
            }
        }
    }

    pub fn executor(&self) -> Result<Executor<'_>, SapiError> {
        Executor::new(self)
    }

    pub fn execute(
        &self,
        ctx: ExecutionContext,
    ) -> Result<ExecutionResult, ExecutionError> {
        self.executor()
            .map_err(|_| ExecutionError::NotInitialized)?
            .execute(ctx)
    }

    pub fn execute_streaming<F>(
        &self,
        ctx: ExecutionContext,
        on_output: F,
    ) -> Result<ExecutionResult, ExecutionError>
    where
        F: FnMut(&[u8]) + 'static,
    {
        self.executor()
            .map_err(|_| ExecutionError::NotInitialized)?
            .execute_streaming(ctx, on_output)
    }

    pub fn execute_with_hooks<H: ExecutionHooks + 'static>(
        &self,
        ctx: ExecutionContext,
        hooks: H,
    ) -> Result<ExecutionResult, ExecutionError> {
        self.executor()
            .map_err(|_| ExecutionError::NotInitialized)?
            .execute_with_hooks(ctx, hooks)
    }

    pub fn is_initialized(&self) -> bool {
        PHP_INIT_RESULT
            .get()
            .map(|result| result.is_ok())
            .unwrap_or(false)
    }
}
