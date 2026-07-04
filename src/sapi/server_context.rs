use std::cell::Cell;
use std::ffi::CString;
use std::sync::OnceLock;

use crate::execution::{
    ExecutionContext, ExecutionMessage, ExecutionResult, ResponseHeader,
};
use crate::sapi::response::{
    AbortReason, BufferedResponseSink, ResponseLifecycle, ResponseSink,
    SinkResult,
};
use crate::sapi::ServerVarsCString;

const MIN_BUFFER_SIZE: usize = 4096;
const DEFAULT_BUFFER_SIZE: usize = 65536;

/// Output buffer configuration. Set via `SAPI_INIT_BUF` and `SAPI_BUF_GROWTH` env vars.
#[derive(Clone, Copy)]
struct BufferPolicy {
    initial_cap: usize,
    strategy: Growth,
}

/// Buffer growth strategy when output exceeds capacity.
#[derive(Clone, Copy)]
enum Growth {
    X4,
    X2,
    Fixed(usize),
}

static BUFFER_POLICY: OnceLock<BufferPolicy> = OnceLock::new();

fn buffer_policy() -> &'static BufferPolicy {
    BUFFER_POLICY.get_or_init(|| {
        let initial_cap = std::env::var("SAPI_INIT_BUF")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|&n| n >= MIN_BUFFER_SIZE)
            .unwrap_or(DEFAULT_BUFFER_SIZE);

        let strategy = match std::env::var("SAPI_BUF_GROWTH")
            .ok()
            .as_deref()
        {
            Some("x2") | Some("X2") => Growth::X2,
            Some("fixed32k") => Growth::Fixed(32 * 1024),
            _ => Growth::X4,
        };

        BufferPolicy {
            initial_cap,
            strategy,
        }
    })
}

type FlushCallback = Box<dyn FnMut()>;
type OutputCallback = Box<dyn FnMut(&[u8])>;

/// Per-request state for the SAPI.
///
/// # Interior Mutability
///
/// `status_code` and `post_position` use `Cell` because they're mutated via raw
/// pointers from FFI callbacks. Cell provides interior mutability without runtime
/// overhead, making the aliasing pattern well-defined per Rust's memory model.
pub struct ServerContext {
    status_code: Cell<u16>,
    response: ResponseLifecycle,
    sink: BufferedResponseSink,
    pub post_data: Vec<u8>,
    post_position: Cell<usize>,
    pub messages: Vec<ExecutionMessage>,
    pub vars: Option<ServerVarsCString>,
    pub env_vars: Vec<(CString, CString)>,
    pub ini_overrides: Vec<(CString, CString)>,
    pub response_headers: Vec<ResponseHeader>,
    pub output_callback: Option<OutputCallback>,
    pub flush_callback: Option<FlushCallback>,
    pub log_to_stderr: bool,
}

impl Default for ServerContext {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerContext {
    pub fn new() -> Self {
        let policy = buffer_policy();

        Self {
            post_data: Vec::new(),
            post_position: Cell::new(0),
            status_code: Cell::new(200),
            response: ResponseLifecycle::default(),
            sink: BufferedResponseSink::with_capacity(policy.initial_cap),
            messages: Vec::with_capacity(8),
            vars: None,
            env_vars: Vec::new(),
            ini_overrides: Vec::new(),
            response_headers: Vec::with_capacity(16),
            output_callback: None,
            flush_callback: None,
            log_to_stderr: false,
        }
    }

    pub fn status_code(&self) -> u16 {
        self.status_code.get()
    }

    pub fn content_type_ptr(&self) -> *const std::ffi::c_char {
        self.vars
            .as_ref()
            .map(|v| v.content_type_ptr())
            .unwrap_or(std::ptr::null())
    }

    pub fn query_string_ptr(&self) -> *mut std::ffi::c_char {
        self.vars
            .as_ref()
            .map(|v| v.query_string_ptr())
            .unwrap_or(std::ptr::null_mut())
    }

    pub fn cookie_data_ptr(&self) -> *mut std::ffi::c_char {
        self.vars
            .as_ref()
            .map(|v| v.cookie_ptr())
            .unwrap_or(std::ptr::null_mut())
    }

    pub fn request_method_ptr(&self) -> *const std::ffi::c_char {
        self.vars
            .as_ref()
            .map(|v| v.request_method_ptr())
            .unwrap_or(c"GET".as_ptr())
    }

    pub fn server_vars(&self) -> &[(CString, CString)] {
        self.vars
            .as_ref()
            .map(|v| v.vars.as_slice())
            .unwrap_or(&[])
    }

    pub fn read_post(&self, buffer: &mut [u8]) -> usize {
        if buffer.is_empty() {
            return 0;
        }

        let pos = self.post_position.get();
        let remaining = self
            .post_data
            .len()
            .saturating_sub(pos);
        let to_copy = remaining.min(buffer.len());

        if to_copy > 0 {
            let end = pos + to_copy;
            buffer[..to_copy].copy_from_slice(&self.post_data[pos..end]);
            self.post_position.set(end);
        }

        to_copy
    }

    pub fn write_output(&mut self, data: &[u8]) -> usize {
        if !self.response.can_write() {
            return data.len();
        }

        if let Some(ref mut callback) = self.output_callback {
            callback(data);

            return data.len();
        }

        let actual_buffer_length = self.sink.capacity();
        let required_buffer_length = self.sink.len() + data.len();

        if required_buffer_length > actual_buffer_length {
            let policy = buffer_policy();

            let new_cap = match policy.strategy {
                Growth::X4 => actual_buffer_length
                    .saturating_mul(4)
                    .max(required_buffer_length + policy.initial_cap),
                Growth::X2 => actual_buffer_length
                    .saturating_mul(2)
                    .max(required_buffer_length + policy.initial_cap),
                Growth::Fixed(step) => {
                    let mut cap = actual_buffer_length;
                    while cap < required_buffer_length {
                        cap = cap.saturating_add(step);
                    }
                    cap
                }
            };

            self.sink
                .reserve(new_cap - self.sink.len());
        }

        let _ = self.sink.write(data);
        data.len()
    }

    pub fn add_header(&mut self, header: ResponseHeader) {
        if self
            .response
            .headers_finalized()
        {
            return;
        }

        self.response_headers
            .push(header);
    }

    pub fn start_headers(&mut self, code: u16) -> bool {
        if self
            .response
            .headers_finalized()
        {
            return false;
        }

        self.set_status(code);
        self.response_headers.clear();
        true
    }

    pub(crate) fn headers_finalized(&self) -> bool {
        self.response
            .headers_finalized()
    }

    pub fn finalize_headers(&mut self) -> bool {
        if !self
            .response
            .finalize_headers()
        {
            return false;
        }

        let result = self
            .sink
            .send_headers(self.status_code(), &self.response_headers);

        if matches!(result, SinkResult::Abort) {
            self.abort_response(AbortReason::SinkFailure);
        }

        true
    }

    pub fn set_status(&self, code: u16) {
        self.status_code.set(code);
    }

    pub fn add_message(&mut self, message: ExecutionMessage) {
        self.messages.push(message);
    }

    pub fn set_output_callback<F: FnMut(&[u8]) + 'static>(
        &mut self,
        callback: F,
    ) {
        self.output_callback = Some(Box::new(callback));
    }

    pub fn set_flush_callback<F: FnMut() + 'static>(&mut self, callback: F) {
        self.flush_callback = Some(Box::new(callback));
    }

    pub fn flush(&mut self) {
        if !self.response.can_flush() {
            return;
        }

        if matches!(self.sink.flush(), SinkResult::Abort) {
            self.abort_response(AbortReason::SinkFailure);
            return;
        }

        if let Some(ref mut callback) = self.flush_callback {
            callback();
        }
    }

    pub fn finalize_response(&mut self) -> bool {
        if !self.response.finish() {
            return false;
        }

        let result = self.sink.finish();
        if matches!(result, SinkResult::Abort) {
            self.abort_response(AbortReason::SinkFailure);
            return false;
        }

        debug_assert!(self.sink.is_finished());

        if let Some(ref mut callback) = self.flush_callback {
            callback();
        }

        true
    }

    pub fn take_response_output(&mut self) -> Vec<u8> {
        self.sink.take_output()
    }

    pub(crate) fn abort_response(&mut self, reason: AbortReason) -> bool {
        if !self.response.abort(reason) {
            return false;
        }

        self.sink.abort(reason);
        true
    }

    pub fn get_env(&self, key: &[u8]) -> Option<*const std::ffi::c_char> {
        self.env_vars
            .iter()
            .find(|(k, _)| k.as_bytes() == key)
            .map(|(_, v)| v.as_ptr())
    }

    pub fn into_result(self, body: Vec<u8>) -> ExecutionResult {
        ExecutionResult::new(
            self.status_code.get(),
            0,
            body,
            self.response_headers,
            self.messages,
        )
    }
}

impl From<ExecutionContext> for Box<ServerContext> {
    fn from(ctx: ExecutionContext) -> Self {
        let mut server_ctx = Box::new(ServerContext::new());

        server_ctx.post_data = ctx.input;
        server_ctx.log_to_stderr = ctx.log_to_stderr;

        server_ctx.vars = Some(
            ctx.server_vars
                .into_cstring_pairs(),
        );

        server_ctx.env_vars = ctx
            .env_vars
            .into_iter()
            .filter_map(|(k, v)| {
                Some((CString::new(k).ok()?, CString::new(v).ok()?))
            })
            .collect();

        server_ctx.ini_overrides = ctx
            .ini_overrides
            .into_iter()
            .filter_map(|(k, v)| {
                Some((CString::new(k).ok()?, CString::new(v).ok()?))
            })
            .collect();

        server_ctx
    }
}
