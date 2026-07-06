use std::cell::Cell;
use std::ffi::CString;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use crate::execution::{
    AbortReason, ExecutionContext, ExecutionControl, ExecutionMessage,
    ExecutionReport, ExecutionReportParts, ExecutionResult, ResponseHeader,
    ResponseSink, SinkResult,
};
use crate::sapi::response::{BufferedResponseSink, ResponseLifecycle};
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

enum ResponseTarget {
    Buffered(BufferedResponseSink),
    Host(Box<dyn ResponseSink>),
}

impl ResponseTarget {
    fn buffered(capacity: usize) -> Self {
        Self::Buffered(BufferedResponseSink::with_capacity(capacity))
    }

    fn host(sink: Box<dyn ResponseSink>) -> Self {
        Self::Host(sink)
    }

    fn capacity(&self) -> Option<usize> {
        match self {
            Self::Buffered(sink) => Some(sink.capacity()),
            Self::Host(_) => None,
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::Buffered(sink) => sink.len(),
            Self::Host(_) => 0,
        }
    }

    fn reserve(&mut self, additional: usize) {
        if let Self::Buffered(sink) = self {
            sink.reserve(additional);
        }
    }

    fn take_output(&mut self) -> Vec<u8> {
        match self {
            Self::Buffered(sink) => sink.take_output(),
            Self::Host(_) => Vec::new(),
        }
    }
}

impl ResponseSink for ResponseTarget {
    fn send_headers(
        &mut self,
        status: u16,
        headers: &[ResponseHeader],
    ) -> SinkResult {
        match self {
            Self::Buffered(sink) => sink.send_headers(status, headers),
            Self::Host(sink) => sink.send_headers(status, headers),
        }
    }

    fn write(&mut self, bytes: &[u8]) -> SinkResult {
        match self {
            Self::Buffered(sink) => sink.write(bytes),
            Self::Host(sink) => sink.write(bytes),
        }
    }

    fn flush(&mut self) -> SinkResult {
        match self {
            Self::Buffered(sink) => sink.flush(),
            Self::Host(sink) => sink.flush(),
        }
    }

    fn finish(&mut self) -> SinkResult {
        match self {
            Self::Buffered(sink) => sink.finish(),
            Self::Host(sink) => sink.finish(),
        }
    }

    fn abort(&mut self, reason: AbortReason) {
        match self {
            Self::Buffered(sink) => sink.abort(reason),
            Self::Host(sink) => sink.abort(reason),
        }
    }

    fn is_finished(&self) -> bool {
        match self {
            Self::Buffered(sink) => sink.is_finished(),
            Self::Host(sink) => sink.is_finished(),
        }
    }
}

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
    sink: ResponseTarget,
    control: Arc<ExecutionControl>,
    post_finish_started_at: Option<Instant>,
    pub post_data: Vec<u8>,
    post_position: Cell<usize>,
    pub messages: Vec<ExecutionMessage>,
    pub vars: Option<ServerVarsCString>,
    pub env_vars: Vec<(CString, CString)>,
    pub ini_overrides: Vec<(CString, CString)>,
    pub response_headers: Vec<ResponseHeader>,
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
        Self::with_response_target(
            ResponseTarget::buffered(buffer_policy().initial_cap),
            Arc::new(ExecutionControl::default()),
        )
    }

    pub(crate) fn with_response_sink(sink: Box<dyn ResponseSink>) -> Self {
        Self::with_response_sink_and_options(
            sink,
            Arc::new(ExecutionControl::default()),
        )
    }

    pub(crate) fn with_response_sink_and_options(
        sink: Box<dyn ResponseSink>,
        control: Arc<ExecutionControl>,
    ) -> Self {
        Self::with_response_target(ResponseTarget::host(sink), control)
    }

    fn with_response_target(
        sink: ResponseTarget,
        control: Arc<ExecutionControl>,
    ) -> Self {
        Self {
            post_data: Vec::new(),
            post_position: Cell::new(0),
            status_code: Cell::new(200),
            response: ResponseLifecycle::default(),
            sink,
            control,
            post_finish_started_at: None,
            messages: Vec::with_capacity(8),
            vars: None,
            env_vars: Vec::new(),
            ini_overrides: Vec::new(),
            response_headers: Vec::with_capacity(16),
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
        if self.stop_delivery_if_controlled() {
            return data.len();
        }

        if !self.response.can_write() {
            return data.len();
        }

        let Some(actual_buffer_length) = self.sink.capacity() else {
            return self.write_to_sink(data);
        };

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

        self.write_to_sink(data)
    }

    fn write_to_sink(&mut self, data: &[u8]) -> usize {
        let result = self.sink.write(data);

        self.apply_sink_result(result);

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
        if self.stop_delivery_if_controlled() {
            return false;
        }

        if self
            .response
            .headers_finalized()
        {
            return false;
        }

        let result = self
            .sink
            .send_headers(self.status_code(), &self.response_headers);

        if !self.apply_sink_result(result) {
            return false;
        }

        self.response
            .finalize_headers()
    }

    pub fn set_status(&self, code: u16) {
        self.status_code.set(code);
    }

    pub fn add_message(&mut self, message: ExecutionMessage) {
        self.messages.push(message);
    }

    pub fn set_flush_callback<F: FnMut() + 'static>(&mut self, callback: F) {
        self.flush_callback = Some(Box::new(callback));
    }

    pub fn flush(&mut self) {
        if self.stop_delivery_if_controlled() {
            return;
        }

        if !self.response.can_flush() {
            return;
        }

        let result = self.sink.flush();

        if !self.apply_sink_result(result) {
            return;
        }

        if let Some(ref mut callback) = self.flush_callback {
            callback();
        }
    }

    pub fn finalize_response(&mut self) -> bool {
        self.finish_response(false)
    }

    pub fn finalize_response_early(&mut self) -> bool {
        self.finish_response(true)
    }

    fn finish_response(&mut self, finalized_early: bool) -> bool {
        if self.stop_delivery_if_controlled() {
            return false;
        }

        if !self.response.can_finish() {
            return false;
        }

        let result = self.sink.finish();

        if !self.apply_sink_result(result) {
            return false;
        }

        let finished = if finalized_early {
            self.response.finish_early()
        } else {
            self.response.finish()
        };

        if !finished {
            return false;
        }

        if finalized_early {
            self.post_finish_started_at = Some(Instant::now());
        }

        debug_assert!(self.sink.is_finished());

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

    pub(crate) fn mark_client_closed(&mut self) -> bool {
        self.control
            .mark_client_closed();

        if !self
            .response
            .mark_client_closed()
        {
            return false;
        }

        self.sink
            .abort(AbortReason::ClientClosed);
        true
    }

    pub(crate) fn can_finish_response(&self) -> bool {
        self.response.can_finish()
    }

    pub(crate) fn observe_control_state(&mut self) -> bool {
        self.stop_delivery_if_controlled()
    }

    pub(crate) fn finalized_early(&self) -> bool {
        self.response
            .finalized_early()
    }

    pub(crate) fn aborted(&self) -> bool {
        self.response.aborted()
    }

    pub(crate) fn abort_reason(&self) -> Option<AbortReason> {
        self.response.abort_reason()
    }

    pub(crate) fn report_abort_reason(&self) -> Option<AbortReason> {
        self.abort_reason()
            .or_else(|| self.control.abort_reason())
    }

    pub(crate) fn report_aborted(&self) -> bool {
        self.aborted()
            || self
                .control
                .abort_reason()
                .is_some()
    }

    pub(crate) fn client_closed(&self) -> bool {
        self.response.client_closed()
            || self
                .control
                .is_client_closed()
    }

    pub(crate) fn timed_out(&self) -> bool {
        self.control
            .is_deadline_exceeded()
    }

    pub(crate) fn post_finish_duration(&self) -> Option<Duration> {
        if !self.finalized_early() {
            return None;
        }

        self.post_finish_started_at
            .map(|started_at| started_at.elapsed())
    }

    fn stop_delivery_if_controlled(&mut self) -> bool {
        if self
            .control
            .deadline_exceeded(Instant::now())
        {
            self.abort_response(AbortReason::DeadlineExceeded);
            return true;
        }

        if self.control.is_cancelled() {
            self.abort_response(AbortReason::HostAbort);
            return true;
        }

        if self
            .control
            .is_client_closed()
        {
            self.mark_client_closed();
            return true;
        }

        false
    }

    fn apply_sink_result(&mut self, result: SinkResult) -> bool {
        match result {
            SinkResult::Continue => true,
            SinkResult::Closed => {
                self.mark_client_closed();
                false
            }
            SinkResult::Abort => {
                self.abort_response(AbortReason::SinkFailure);
                false
            }
        }
    }

    pub fn get_env(&self, key: &[u8]) -> Option<*const std::ffi::c_char> {
        self.env_vars
            .iter()
            .find(|(k, _)| k.as_bytes() == key)
            .map(|(_, v)| v.as_ptr())
    }

    pub fn into_result(
        self,
        exit_status: i32,
        body: Vec<u8>,
    ) -> ExecutionResult {
        ExecutionResult::new(
            self.status_code.get(),
            exit_status,
            body,
            self.response_headers,
            self.messages,
        )
    }

    pub fn into_report(
        self,
        exit_status: i32,
        php_success: bool,
    ) -> ExecutionReport {
        ExecutionReport::new(ExecutionReportParts {
            status_code: self.status_code.get(),
            exit_status,
            php_success,
            finalized_early: self.finalized_early(),
            aborted: self.report_aborted(),
            client_closed: self.client_closed(),
            timed_out: self.timed_out(),
            post_finish_duration: self.post_finish_duration(),
            abort_reason: self.report_abort_reason(),
            messages: self.messages,
        })
    }

    pub(crate) fn from_context_with_sink(
        ctx: ExecutionContext,
        sink: Box<dyn ResponseSink>,
    ) -> Box<ServerContext> {
        let mut server_ctx = Box::new(ServerContext::with_response_sink(sink));
        server_ctx.apply_context(ctx);
        server_ctx
    }

    pub(crate) fn from_context_with_sink_and_options(
        ctx: ExecutionContext,
        sink: Box<dyn ResponseSink>,
        control: Arc<ExecutionControl>,
    ) -> Box<ServerContext> {
        let mut server_ctx = Box::new(
            ServerContext::with_response_sink_and_options(sink, control),
        );
        server_ctx.apply_context(ctx);
        server_ctx
    }

    fn apply_context(&mut self, ctx: ExecutionContext) {
        self.post_data = ctx.input;
        self.log_to_stderr = ctx.log_to_stderr;

        self.vars = Some(
            ctx.server_vars
                .into_cstring_pairs(),
        );

        self.env_vars = ctx
            .env_vars
            .into_iter()
            .filter_map(|(k, v)| {
                Some((CString::new(k).ok()?, CString::new(v).ok()?))
            })
            .collect();

        self.ini_overrides = ctx
            .ini_overrides
            .into_iter()
            .filter_map(|(k, v)| {
                Some((CString::new(k).ok()?, CString::new(v).ok()?))
            })
            .collect();
    }
}

impl From<ExecutionContext> for Box<ServerContext> {
    fn from(ctx: ExecutionContext) -> Self {
        let mut server_ctx = Box::new(ServerContext::new());
        server_ctx.apply_context(ctx);
        server_ctx
    }
}
