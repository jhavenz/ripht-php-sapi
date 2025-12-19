use std::ffi::CString;

use std::any::TypeId;
use thiserror::Error;

#[cfg(feature = "tracing")]
use tracing::{debug, error, trace, warn};

use super::ffi;
use super::server_context::ServerContext;
use super::SapiError;
use crate::execution::{
    ExecutionContext, ExecutionHooks, ExecutionResult, NoOpHooks, OutputAction,
};

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ExecutionError {
    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("Script not found: {0}")]
    ScriptNotFound(std::path::PathBuf),

    #[error("PHP engine not initialized")]
    NotInitialized,

    #[error("Request startup failed")]
    StartupFailed,
}

pub struct Executor<'sapi> {
    sapi: &'sapi super::RiphtSapi,
}

impl<'sapi> Executor<'sapi> {
    pub(super) fn new(
        sapi: &'sapi super::RiphtSapi,
    ) -> Result<Self, SapiError> {
        if !sapi.is_initialized() {
            return Err(SapiError::NotInitialized);
        }

        Ok(Self { sapi })
    }

    pub fn execute(
        &self,
        ctx: ExecutionContext,
    ) -> Result<ExecutionResult, ExecutionError> {
        self.execute_with_hooks(ctx, NoOpHooks)
    }

    pub fn execute_streaming<F>(
        &self,
        ctx: ExecutionContext,
        on_output: F,
    ) -> Result<ExecutionResult, ExecutionError>
    where
        F: FnMut(&[u8]) + 'static,
    {
        #[cfg(feature = "tracing")]
        debug!(
            script_path = %ctx.script_path.display(),
            "Executing PHP (streaming)"
        );

        if !self.sapi.is_initialized() {
            return Err(ExecutionError::NotInitialized);
        }

        if !ctx.script_path.exists() {
            return Err(ExecutionError::ScriptNotFound(
                ctx.script_path.clone(),
            ));
        }

        let script_cstr = ctx.path_as_cstring()?;

        let mut server_ctx = Box::<ServerContext>::from(ctx);
        server_ctx.set_output_callback(on_output);

        unsafe {
            let ctx_ptr = Box::into_raw(server_ctx);
            ffi::sapi_globals.server_context = ctx_ptr as *mut std::ffi::c_void;

            Self::setup_globals(&*ctx_ptr);

            if ffi::php_request_startup() == ffi::FAILURE {
                let _ = Box::from_raw(ctx_ptr);
                ffi::php_request_shutdown(std::ptr::null_mut());
                ffi::sapi_globals.server_context = std::ptr::null_mut();
                return Err(ExecutionError::StartupFailed);
            }

            Self::apply_ini_overrides(&*ctx_ptr);

            Self::run_script(&script_cstr);

            ffi::sapi_globals.post_read = 1;

            ffi::php_request_shutdown(std::ptr::null_mut());

            ffi::sapi_globals.server_context = std::ptr::null_mut();

            let server_ctx = Box::from_raw(ctx_ptr);

            Self::cleanup_globals();

            let result = ExecutionResult {
                status: server_ctx.status_code,
                headers: server_ctx
                    .response_headers
                    .clone(),
                body: Vec::new(),
                messages: server_ctx.messages,
            };

            Ok(result)
        }
    }

    pub fn execute_with_hooks<H: ExecutionHooks + 'static>(
        &self,
        ctx: ExecutionContext,
        mut hooks: H,
    ) -> Result<ExecutionResult, ExecutionError> {
        #[cfg(feature = "tracing")]
        debug!(
            script_path = %ctx.script_path.display(),
            "Executing PHP"
        );

        if !self.sapi.is_initialized() {
            #[cfg(feature = "tracing")]
            error!("Execute before init");
            return Err(ExecutionError::NotInitialized);
        }

        if !ctx.script_path.exists() {
            return Err(ExecutionError::ScriptNotFound(
                ctx.script_path.clone(),
            ));
        }

        let script_cstr = ctx.path_as_cstring()?;
        let script_path = ctx.script_path.clone();

        hooks.on_context_created();

        let server_ctx = Box::<ServerContext>::from(ctx);

        unsafe {
            let ctx_ptr = Box::into_raw(server_ctx);
            ffi::sapi_globals.server_context = ctx_ptr as *mut std::ffi::c_void;

            Self::setup_globals(&*ctx_ptr);

            hooks.on_request_starting();

            #[cfg(feature = "tracing")]
            trace!("Starting PHP request");

            let startup_result = ffi::php_request_startup();

            if startup_result == ffi::FAILURE {
                #[cfg(feature = "tracing")]
                error!("Request startup failed");
                let _ = Box::from_raw(ctx_ptr);
                ffi::php_request_shutdown(std::ptr::null_mut());
                ffi::sapi_globals.server_context = std::ptr::null_mut();
                return Err(ExecutionError::StartupFailed);
            }

            Self::apply_ini_overrides(&*ctx_ptr);

            hooks.on_request_started();
            hooks.on_script_executing(&script_path);

            #[cfg(feature = "tracing")]
            trace!("Executing script");

            let exec_result = Self::run_script(&script_cstr);

            let success = exec_result != ffi::FAILURE;
            hooks.on_script_executed(success);

            hooks.on_request_finishing();

            #[cfg(feature = "tracing")]
            trace!("Shutting down request");

            ffi::sapi_globals.post_read = 1;
            ffi::php_request_shutdown(std::ptr::null_mut());

            ffi::sapi_globals.server_context = std::ptr::null_mut();

            let mut server_ctx = Box::from_raw(ctx_ptr);

            Self::cleanup_globals();

            let headers: Vec<(String, String)> =
                if TypeId::of::<H>() == TypeId::of::<NoOpHooks>() {
                    // Fast path: no filtering; move headers out without cloning
                    std::mem::take(&mut server_ctx.response_headers)
                } else {
                    server_ctx
                        .response_headers
                        .iter()
                        .filter(|(name, value)| hooks.on_header(name, value))
                        .cloned()
                        .collect()
                };

            hooks.on_status(server_ctx.status_code);

            for message in &server_ctx.messages {
                hooks.on_php_message(message);
            }

            let body = match hooks.on_output(&server_ctx.output_buffer) {
                OutputAction::Buffer => server_ctx.output_buffer,
                OutputAction::Handled => Vec::new(),
            };

            #[cfg(feature = "tracing")]
            debug!(
                status = server_ctx.status_code,
                body_len = body.len(),
                headers_count = headers.len(),
                messages_count = server_ctx.messages.len(),
                format!("Execution {}", if success { "succeeded" } else { "failed" })
            );

            let result = ExecutionResult {
                status: server_ctx.status_code,
                headers,
                body,
                messages: server_ctx.messages,
            };

            hooks.on_request_finished(&result);

            Ok(result)
        }
    }

    unsafe fn setup_globals(ctx: &ServerContext) {
        ffi::sapi_globals
            .request_info
            .request_method = ctx.request_method_ptr();

        ffi::sapi_globals
            .request_info
            .content_type = ctx.content_type_ptr();

        ffi::sapi_globals
            .request_info
            .content_length = ctx.post_data.len() as i64;

        ffi::sapi_globals
            .request_info
            .query_string = ctx.query_string_ptr();

        ffi::sapi_globals
            .sapi_headers
            .http_response_code = 200;
    }

    unsafe fn run_script(script_cstr: &CString) -> i32 {
        let mut file_handle = ffi::zend_file_handle::default();
        ffi::zend_stream_init_filename(&mut file_handle, script_cstr.as_ptr());
        file_handle.primary_script = 1;

        let exec_result = ffi::php_execute_script(&mut file_handle);
        ffi::zend_destroy_file_handle(&mut file_handle);
        exec_result
    }

    unsafe fn cleanup_globals() {
        ffi::sapi_globals.server_context = std::ptr::null_mut();

        ffi::sapi_globals
            .request_info
            .content_type = std::ptr::null();

        ffi::sapi_globals
            .request_info
            .query_string = std::ptr::null_mut();

        ffi::sapi_globals
            .request_info
            .cookie_data = std::ptr::null_mut();
    }

    unsafe fn apply_ini_overrides(ctx: &ServerContext) {
        if ctx.ini_overrides.is_empty() {
            return;
        }

        let init = ffi::zend_string_init_interned
            .expect("null pointer - PHP may not be properly initialized");

        for (key, value) in &ctx.ini_overrides {
            let name = init(key.as_ptr(), key.as_bytes().len(), true);
            if name.is_null() {
                continue;
            }

            ffi::zend_alter_ini_entry_chars(
                name,
                value.as_ptr(),
                value.as_bytes().len(),
                ffi::ZEND_INI_USER | ffi::ZEND_INI_SYSTEM,
                ffi::ZEND_INI_STAGE_RUNTIME,
            );
        }
    }
}
