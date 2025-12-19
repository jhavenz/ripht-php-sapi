use std::ffi::CString;
use std::sync::OnceLock;

use crate::execution::{ExecutionContext, ExecutionMessage};
use crate::sapi::ServerVarsCString;

#[derive(Clone, Copy)]
struct BufferPolicy {
    initial_cap: usize,
    strategy: Growth,
}

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
            .filter(|&n| n >= 4096)
            .unwrap_or(65536);

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

pub struct ServerContext {
    pub status_code: u16,
    pub post_data: Vec<u8>,
    pub post_position: usize,
    pub output_buffer: Vec<u8>,
    pub messages: Vec<ExecutionMessage>,
    pub vars: Option<ServerVarsCString>,
    pub env_vars: Vec<(CString, CString)>,
    pub ini_overrides: Vec<(CString, CString)>,
    pub response_headers: Vec<(String, String)>,
    pub output_callback: Option<OutputCallback>,
    pub flush_callback: Option<FlushCallback>,
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
            post_position: 0,
            output_buffer: Vec::with_capacity(policy.initial_cap),
            status_code: 200,
            messages: Vec::with_capacity(4),
            vars: None,
            env_vars: Vec::new(),
            ini_overrides: Vec::new(),
            response_headers: Vec::with_capacity(8),
            output_callback: None,
            flush_callback: None,
        }
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

    pub fn read_post(&mut self, buffer: &mut [u8]) -> usize {
        if buffer.is_empty() {
            return 0;
        }

        let remaining_post_data = self
            .post_data
            .len()
            .saturating_sub(self.post_position);

        let post_data_overflow = remaining_post_data.min(buffer.len());

        if post_data_overflow > 0 {
            let start = self.post_position;
            let end = start + post_data_overflow;
            buffer[..post_data_overflow]
                .copy_from_slice(&self.post_data[start..end]);

            self.post_position = end;
        }

        post_data_overflow
    }

    pub fn write_output(&mut self, data: &[u8]) -> usize {
        if let Some(ref mut callback) = self.output_callback {
            callback(data);

            return data.len();
        }

        let actual_buffer_length = self.output_buffer.capacity();
        let required_buffer_length = self.output_buffer.len() + data.len();

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

            self.output_buffer
                .reserve(new_cap - self.output_buffer.len());
        }

        self.output_buffer
            .extend_from_slice(data);
        data.len()
    }

    pub fn add_header(&mut self, name: String, value: String) {
        self.response_headers
            .push((name, value));
    }

    pub fn set_status(&mut self, code: u16) {
        self.status_code = code;
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
        if let Some(ref mut callback) = self.flush_callback {
            callback();
        }
    }

    pub fn get_env(&self, key: &[u8]) -> Option<*const std::ffi::c_char> {
        self.env_vars
            .iter()
            .find(|(k, _)| k.as_bytes() == key)
            .map(|(_, v)| v.as_ptr())
    }
}

impl From<ExecutionContext> for Box<ServerContext> {
    fn from(ctx: ExecutionContext) -> Self {
        let mut server_ctx = Box::new(ServerContext::new());

        server_ctx.post_data = ctx.input;

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
