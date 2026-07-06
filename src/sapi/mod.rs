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
pub mod config;
mod executor;
pub(crate) mod ffi;
pub mod native;
pub(crate) mod native_functions;
pub(crate) mod response;
pub(crate) mod server_context;
pub(crate) mod server_vars;

pub use config::SapiConfig;
pub use executor::{ExecutionError, Executor};
pub(crate) use server_vars::{ServerVars, ServerVarsCString};

use crate::execution::{
    ExecutionContext, ExecutionHooks, ExecutionOptions, ExecutionReport,
    ExecutionResult, ResponseSink,
};
use config::ResolvedConfig;

static PHP_INIT_RESULT: OnceLock<Result<(), SapiError>> = OnceLock::new();
static SAPI_CONFIG: OnceLock<SapiConfig> = OnceLock::new();
pub(crate) static RESOLVED_CONFIG: OnceLock<ResolvedConfig> = OnceLock::new();

/// Errors from SAPI initialization and configuration.
#[derive(Debug, Clone, Error)]
#[non_exhaustive]
pub enum SapiError {
    #[error("PHP engine not initialized")]
    NotInitialized,

    #[error("PHP initialization failed: {0}")]
    InitializationFailed(String),

    #[error("PHP engine already initialized — configure() must be called before instance()")]
    AlreadyInitialized,

    #[error("SAPI already configured — configure() can only be called once")]
    AlreadyConfigured,

    #[error("SAPI name contains null byte: {0:?}")]
    InvalidSapiName(String),

    #[error("pretty name contains null byte: {0:?}")]
    InvalidPrettyName(String),

    #[error("server software string contains null byte: {0:?}")]
    InvalidServerSoftware(String),

    #[error("INI key contains null byte")]
    InvalidIniKey,

    #[error("INI value contains null byte")]
    InvalidIniValue,

    #[error("INI path contains null byte")]
    InvalidIniPath,

    #[error("Failed to set INI: {0}")]
    IniSetFailed(String),

    #[error(
        "PHP library not found. Build PHP with --enable-embed=static and set RIPHT_PHP_SAPI_PREFIX"
    )]
    LibraryNotFound,
}

/// Finalizes the active PHP request response, if one is currently executing.
///
/// This is intended for host native functions that provide compatibility with
/// PHP runtimes such as FastCGI. It is a no-op outside an active request and
/// never unwinds across the caller's native boundary.
pub fn finish_current_request() -> bool {
    callbacks::finish_current_request()
}

/// PHP SAPI instance. Initialize once, execute scripts repeatedly.
pub struct RiphtSapi {
    _marker: std::marker::PhantomData<*mut ()>,
}

impl RiphtSapi {
    /// Apply custom SAPI configuration before the first [`RiphtSapi::instance()`] call.
    ///
    /// Must be called before PHP is initialized. Returns
    /// [`SapiError::AlreadyInitialized`] if PHP is already running or
    /// [`SapiError::AlreadyConfigured`] if `configure` was already called.
    ///
    /// Call this from a single thread during startup. `configure` and
    /// `instance` are not internally synchronized against each other: a
    /// `configure` racing a concurrent `instance` may be silently ignored
    /// (the engine reads defaults before the config lands). This matches the
    /// non-ZTS, single-threaded-init contract of the crate.
    pub fn configure(config: SapiConfig) -> Result<(), SapiError> {
        if PHP_INIT_RESULT
            .get()
            .is_some()
        {
            return Err(SapiError::AlreadyInitialized);
        }
        SAPI_CONFIG
            .set(config)
            .map_err(|_| SapiError::AlreadyConfigured)
    }

    // Note: will panic if initialization fails
    #[must_use]
    pub fn instance() -> Self {
        Self::init().expect("SAPI initialization failure")
    }

    fn init() -> Result<Self, SapiError> {
        let init_result = PHP_INIT_RESULT.get_or_init(|| {
            #[cfg(feature = "tracing")]
            info!("Initializing RiphtSapi");

            let config = SAPI_CONFIG
                .get()
                .cloned()
                .unwrap_or_default();
            let resolved = config.resolve()?;
            let resolved = RESOLVED_CONFIG.get_or_init(|| resolved);

            // SAFETY: One-time PHP engine initialization via OnceLock.
            // All pointers/callbacks are static or 'static and remain valid.
            // ResolvedConfig fields are Box::leak'd to 'static.
            unsafe {
                ffi::sapi_module.name = resolved.sapi_name.as_ptr() as *mut _;
                ffi::sapi_module.pretty_name =
                    resolved.pretty_name.as_ptr() as *mut _;

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
                ffi::sapi_module.sapi_error = Some(ffi::ripht_sapi_error_shim);

                ffi::sapi_module.header_handler =
                    Some(callbacks::ripht_sapi_header_handler);

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

                ffi::sapi_module.php_ini_ignore =
                    i32::from(resolved.ignore_php_ini);
                ffi::sapi_module.php_ini_ignore_cwd =
                    i32::from(resolved.ignore_cwd_ini);

                if let Some(ini_path) = resolved.ini_path {
                    ffi::sapi_module.php_ini_path_override =
                        ini_path.as_ptr() as *mut _;
                }

                ffi::sapi_module.input_filter =
                    Some(callbacks::ripht_sapi_input_filter);
                ffi::sapi_module.default_post_reader =
                    Some(callbacks::ripht_sapi_default_post_reader);
                ffi::sapi_module.treat_data =
                    Some(callbacks::ripht_sapi_treat_data);

                ffi::sapi_module.ini_entries =
                    resolved.ini_entries.as_ptr() as *const _;

                ffi::sapi_module.additional_functions = resolved
                    .native_functions
                    .as_ptr();

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

    pub fn execute_with_sink<S>(
        &self,
        ctx: ExecutionContext,
        sink: S,
    ) -> Result<ExecutionReport, ExecutionError>
    where
        S: ResponseSink + 'static,
    {
        self.executor()
            .map_err(|_| ExecutionError::NotInitialized)?
            .execute_with_sink(ctx, sink)
    }

    pub fn execute_with_sink_and_options<S>(
        &self,
        ctx: ExecutionContext,
        sink: S,
        options: ExecutionOptions,
    ) -> Result<ExecutionReport, ExecutionError>
    where
        S: ResponseSink + 'static,
    {
        self.executor()
            .map_err(|_| ExecutionError::NotInitialized)?
            .execute_with_sink_and_options(ctx, sink, options)
    }

    pub fn is_initialized(&self) -> bool {
        PHP_INIT_RESULT
            .get()
            .map(|result| result.is_ok())
            .unwrap_or(false)
    }
}
