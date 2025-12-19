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
pub(crate) static SERVER_SOFTWARE: &str = "Ripht/0.1.0";
static INI_ENTRIES: &[u8] = b"\
variables_order=EGPCS\n\
request_order=GP\n\
output_buffering=4096\n\
implicit_flush=0\n\
html_errors=0\n\
display_errors=1\n\
log_errors=1\n\
\0";

#[derive(Debug, Error)]
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

pub struct RiphtSapi {
    _marker: std::marker::PhantomData<*mut ()>,
}

impl RiphtSapi {
    #[must_use]
    pub fn instance() -> Self {
        Self::init().expect("RiphtSapi initialization failure")
    }

    fn init() -> Result<Self, SapiError> {
        let init_result = PHP_INIT_RESULT.get_or_init(|| {
            #[cfg(feature = "tracing")]
            info!("Initializing RiphtSapi");

            unsafe {
                ffi::sapi_module.name = SAPI_NAME.as_ptr() as *mut _;
                ffi::sapi_module.pretty_name =
                    SAPI_PRETTY_NAME.as_ptr() as *mut _;

                ffi::sapi_module.startup = Some(callbacks::php_sapi_startup);
                ffi::sapi_module.shutdown = Some(callbacks::php_sapi_shutdown);
                ffi::sapi_module.activate = Some(callbacks::php_sapi_activate);
                ffi::sapi_module.deactivate =
                    Some(callbacks::php_sapi_deactivate);

                ffi::sapi_module.ub_write = Some(callbacks::php_sapi_ub_write);
                ffi::sapi_module.flush = Some(callbacks::php_sapi_flush);

                ffi::sapi_module.send_headers =
                    Some(callbacks::php_sapi_send_headers);
                ffi::sapi_module.send_header =
                    Some(callbacks::php_sapi_send_header);

                ffi::sapi_module.read_post =
                    Some(callbacks::php_sapi_read_post);
                ffi::sapi_module.read_cookies =
                    Some(callbacks::php_sapi_read_cookies);

                ffi::sapi_module.register_server_variables =
                    Some(callbacks::php_sapi_register_server_variables);

                ffi::sapi_module.log_message =
                    Some(callbacks::php_sapi_log_message);
                ffi::sapi_module.get_request_time =
                    Some(callbacks::php_sapi_get_request_time);
                ffi::sapi_module.getenv = Some(callbacks::php_sapi_getenv);

                ffi::sapi_module.php_ini_ignore = 0;
                ffi::sapi_module.php_ini_ignore_cwd = 1;

                ffi::sapi_module.input_filter =
                    Some(callbacks::php_sapi_input_filter);
                ffi::sapi_module.default_post_reader =
                    Some(callbacks::php_sapi_default_post_reader);
                ffi::sapi_module.treat_data =
                    Some(callbacks::php_sapi_treat_data);

                ffi::sapi_module.ini_entries = INI_ENTRIES.as_ptr() as *const _;

                #[cfg(feature = "tracing")]
                trace!("Starting SAPI");
                ffi::sapi_startup(&mut ffi::sapi_module);

                #[cfg(feature = "tracing")]
                trace!("Starting PHP module");
                let result = ffi::php_module_startup(
                    &mut ffi::sapi_module,
                    std::ptr::null_mut(),
                );

                if result == ffi::FAILURE {
                    #[cfg(feature = "tracing")]
                    error!("PHP startup failed");
                    ffi::sapi_shutdown();
                    Err(SapiError::InitializationFailed(
                        "php_module_startup returned FAILURE".to_string(),
                    ))
                } else {
                    #[cfg(feature = "tracing")]
                    info!("PHP initialized");
                    Ok(())
                }
            }
        });

        match init_result {
            Ok(()) => Ok(Self {
                _marker: std::marker::PhantomData,
            }),
            Err(e) => Err(SapiError::InitializationFailed(format!(
                "Ripht PHP SAPI initialization failed: {}",
                e
            ))),
        }
    }

    pub fn shutdown() {
        unsafe {
            ffi::php_module_shutdown();
            ffi::sapi_shutdown();
        }
    }

    pub fn set_ini(&self, key: &str, value: &str) -> Result<(), SapiError> {
        let key_cstr =
            CString::new(key).map_err(|_| SapiError::InvalidIniKey)?;
        let value_cstr =
            CString::new(value).map_err(|_| SapiError::InvalidIniValue)?;

        unsafe {
            let init = ffi::zend_string_init_interned
                .expect("zend_string_init_interned function pointer is null - PHP may not be properly initialized");

            let name = init(key_cstr.as_ptr(), key.len(), true);

            if name.is_null() {
                return Err(SapiError::IniSetFailed(key.to_string()));
            }

            let result = ffi::zend_alter_ini_entry_chars(
                name,
                value_cstr.as_ptr(),
                value.len(),
                ffi::ZEND_INI_USER | ffi::ZEND_INI_SYSTEM,
                ffi::ZEND_INI_STAGE_RUNTIME,
            );

            if result != ffi::SUCCESS {
                return Err(SapiError::IniSetFailed(key.to_string()));
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
