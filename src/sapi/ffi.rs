//! Raw PHP C API bindings.
//!
//! Contains struct definitions and extern declarations matching PHP's headers.
//! Users shouldn't need to interact with this module directly.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::os::raw::{c_char, c_double, c_int, c_uint, c_void};

pub const SUCCESS: c_int = 0;
pub const FAILURE: c_int = -1;
pub const SAPI_HEADER_SENT_SUCCESSFULLY: c_int = 1;
pub const SAPI_HEADER_SEND_FAILED: c_int = 0;
pub const ZEND_HANDLE_FILENAME: u8 = 0;
pub const ZEND_INI_USER: c_int = 1;
pub const ZEND_INI_SYSTEM: c_int = 4;
pub const ZEND_INI_STAGE_RUNTIME: c_int = 16;

pub const IS_FALSE: u32 = 2;
pub const IS_TRUE: u32 = 3;
pub const IS_LONG: u32 = 4;
pub const IS_STRING_EX: u32 = 6 | (1 << 2);

#[cfg(unix)]
pub type uid_t = libc::uid_t;
#[cfg(not(unix))]
pub type uid_t = u32;

#[cfg(unix)]
pub type gid_t = libc::gid_t;
#[cfg(not(unix))]
pub type gid_t = u32;

#[cfg(unix)]
pub type zend_stat_t = libc::stat;

#[cfg(not(unix))]
#[repr(C)]
pub struct zend_stat_t {
    _opaque: [u8; 144],
}

#[repr(C)]
pub union zend_value {
    pub lval: i64,
    pub dval: f64,
    pub str: *mut zend_string,
    pub ptr: *mut c_void,
}

#[repr(C)]
pub struct zval {
    pub value: zend_value,
    pub type_info: u32,
    pub _u2: u32,
}

impl zval {
    pub unsafe fn set_long(&mut self, val: i64) {
        self.value.lval = val;
        self.type_info = IS_LONG;
    }

    pub unsafe fn set_bool(&mut self, val: bool) {
        self.type_info = if val { IS_TRUE } else { IS_FALSE };
    }

    pub unsafe fn set_string(&mut self, s: *mut zend_string) {
        self.value.str = s;
        self.type_info = IS_STRING_EX;
    }

    pub unsafe fn set_null(&mut self) {
        self.type_info = 1;
    }

    pub fn ztype(&self) -> u32 {
        self.type_info & 0xFF
    }

    pub unsafe fn as_long(&self) -> Option<i64> {
        if self.ztype() == IS_LONG {
            Some(self.value.lval)
        } else {
            None
        }
    }

    pub unsafe fn as_str(&self) -> Option<&[u8]> {
        if self.ztype() != 6 {
            return None;
        }

        let s = self.value.str;

        if s.is_null() {
            return None;
        }

        let len = (*s).len;
        let ptr = (*s).val.as_ptr() as *const u8;

        Some(std::slice::from_raw_parts(ptr, len))
    }
}

pub unsafe fn call_num_args(execute_data: *mut c_void) -> u32 {
    // num_args is in the This.u2 field of zend_execute_data.
    // This is a zval at offset 32 bytes (4 pointers) into the struct.
    // u2 is at offset 12 within the zval (after 8-byte value + 4-byte type_info).
    let ptr = (execute_data as *const u8).add(32 + 12) as *const u32;

    *ptr
}

pub unsafe fn call_arg_unchecked(
    execute_data: *mut c_void,
    n: usize,
) -> *mut zval {
    // ZEND_CALL_FRAME_SLOT = (sizeof(zend_execute_data) + 15) / 16 = 5
    // Args start at ((zval*)execute_data) + FRAME_SLOT + (n-1)
    let base = execute_data as *mut zval;

    base.add(5 + n - 1)
}

#[repr(C)]
pub struct HashTable {
    _opaque: [u8; 56],
}

#[repr(C)]
pub struct zend_fcall_info_cache {
    _opaque: [u8; 40],
}

#[repr(C)]
pub struct sapi_request_parse_body_options_cache_entry {
    pub set: bool,
    pub value: i64,
}

#[repr(C)]
pub struct sapi_request_parse_body_context {
    pub throw_exceptions: bool,
    pub options_cache: [sapi_request_parse_body_options_cache_entry; 5],
}

#[repr(C)]
pub struct zend_llist_element {
    pub next: *mut zend_llist_element,
    pub prev: *mut zend_llist_element,
    pub data: [c_char; 1],
}

pub type llist_dtor_func_t = Option<unsafe extern "C" fn(*mut c_void)>;

#[repr(C)]
pub struct zend_llist {
    pub head: *mut zend_llist_element,
    pub tail: *mut zend_llist_element,
    pub count: usize,
    pub size: usize,
    pub dtor: llist_dtor_func_t,
    pub persistent: u8,
    pub traverse_ptr: *mut zend_llist_element,
}

impl Default for zend_llist {
    fn default() -> Self {
        Self {
            head: std::ptr::null_mut(),
            tail: std::ptr::null_mut(),
            count: 0,
            size: 0,
            dtor: None,
            persistent: 0,
            traverse_ptr: std::ptr::null_mut(),
        }
    }
}

pub type zif_handler = Option<
    unsafe extern "C" fn(execute_data: *mut c_void, return_value: *mut zval),
>;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct zend_function_entry {
    pub fname: *const c_char,
    pub handler: zif_handler,
    pub arg_info: *const c_void,
    pub num_args: u32,
    pub flags: u32,
    pub frameless_function_infos: *const c_void,
    pub doc_comment: *const c_char,
}

// SAFETY: zend_function_entry is a read-only static table registered once at MINIT.
// The raw pointers within point to static data (function names, handler fns).
unsafe impl Sync for zend_function_entry {}
unsafe impl Send for zend_function_entry {}

impl zend_function_entry {
    pub const fn end() -> Self {
        Self {
            fname: std::ptr::null(),
            handler: None,
            arg_info: std::ptr::null(),
            num_args: 0,
            flags: 0,
            frameless_function_infos: std::ptr::null(),
            doc_comment: std::ptr::null(),
        }
    }
}

#[repr(C)]
pub struct zend_module_entry {
    _private: [u8; 0],
}

#[repr(C)]
pub struct sapi_header_struct {
    pub header: *mut c_char,
    pub header_len: usize,
}

impl Default for sapi_header_struct {
    fn default() -> Self {
        Self {
            header: std::ptr::null_mut(),
            header_len: 0,
        }
    }
}

#[repr(C)]
pub struct sapi_headers_struct {
    pub headers: zend_llist,
    pub http_response_code: c_int,
    pub send_default_content_type: u8,
    pub mimetype: *mut c_char,
    pub http_status_line: *mut c_char,
}

impl Default for sapi_headers_struct {
    fn default() -> Self {
        Self {
            headers: zend_llist::default(),
            http_response_code: 200,
            send_default_content_type: 0,
            mimetype: std::ptr::null_mut(),
            http_status_line: std::ptr::null_mut(),
        }
    }
}

#[repr(C)]
pub struct sapi_request_info {
    pub request_method: *const c_char,
    pub query_string: *mut c_char,
    pub cookie_data: *mut c_char,
    pub content_length: i64,
    pub path_translated: *mut c_char,
    pub request_uri: *mut c_char,
    pub request_body: *mut c_void,
    pub content_type: *const c_char,
    pub headers_only: bool,
    pub no_headers: bool,
    pub headers_read: bool,
    pub post_entry: *mut c_void,
    pub content_type_dup: *mut c_char,
    pub auth_user: *mut c_char,
    pub auth_password: *mut c_char,
    pub auth_digest: *mut c_char,
    pub argv0: *mut c_char,
    pub current_user: *mut c_char,
    pub current_user_length: c_int,
    pub argc: c_int,
    pub argv: *mut *mut c_char,
    pub proto_num: c_int,
}

#[repr(C)]
pub struct sapi_globals_struct {
    pub server_context: *mut c_void,
    pub request_info: sapi_request_info,
    pub sapi_headers: sapi_headers_struct,
    pub read_post_bytes: i64,
    pub post_read: u8,
    pub headers_sent: u8,
    pub global_stat: zend_stat_t,
    pub default_mimetype: *mut c_char,
    pub default_charset: *mut c_char,
    pub rfc1867_uploaded_files: *mut HashTable,
    pub post_max_size: i64,
    pub options: c_int,
    pub sapi_started: bool,
    pub global_request_time: c_double,
    pub known_post_content_types: HashTable,
    pub callback_func: zval,
    pub fci_cache: zend_fcall_info_cache,
    pub request_parse_body_context: sapi_request_parse_body_context,
}

#[repr(C)]
pub struct sapi_module_struct {
    pub name: *mut c_char,
    pub pretty_name: *mut c_char,
    pub startup: Option<unsafe extern "C" fn(*mut sapi_module_struct) -> c_int>,
    pub shutdown:
        Option<unsafe extern "C" fn(*mut sapi_module_struct) -> c_int>,
    pub activate: Option<unsafe extern "C" fn() -> c_int>,
    pub deactivate: Option<unsafe extern "C" fn() -> c_int>,
    pub ub_write: Option<unsafe extern "C" fn(*const c_char, usize) -> usize>,
    pub flush: Option<unsafe extern "C" fn(*mut c_void)>,
    pub get_stat: Option<unsafe extern "C" fn() -> *mut zend_stat_t>,
    pub getenv:
        Option<unsafe extern "C" fn(*const c_char, usize) -> *mut c_char>,
    pub sapi_error: Option<unsafe extern "C" fn(c_int, *const c_char)>,
    pub header_handler: Option<
        unsafe extern "C" fn(
            *mut sapi_header_struct,
            c_int,
            *mut sapi_headers_struct,
        ) -> c_int,
    >,
    pub send_headers:
        Option<unsafe extern "C" fn(*mut sapi_headers_struct) -> c_int>,
    pub send_header:
        Option<unsafe extern "C" fn(*mut sapi_header_struct, *mut c_void)>,
    pub read_post: Option<unsafe extern "C" fn(*mut c_char, usize) -> usize>,
    pub read_cookies: Option<unsafe extern "C" fn() -> *mut c_char>,
    pub register_server_variables: Option<unsafe extern "C" fn(*mut zval)>,
    pub log_message: Option<unsafe extern "C" fn(*const c_char, c_int)>,
    pub get_request_time: Option<unsafe extern "C" fn(*mut c_double) -> c_int>,
    pub terminate_process: Option<unsafe extern "C" fn()>,
    pub php_ini_path_override: *mut c_char,
    pub default_post_reader: Option<unsafe extern "C" fn()>,
    pub treat_data: Option<unsafe extern "C" fn(c_int, *mut c_char, *mut zval)>,
    pub executable_location: *mut c_char,
    pub php_ini_ignore: c_int,
    pub php_ini_ignore_cwd: c_int,
    pub get_fd: Option<unsafe extern "C" fn(*mut c_int) -> c_int>,
    pub force_http_10: Option<unsafe extern "C" fn() -> c_int>,
    pub get_target_uid: Option<unsafe extern "C" fn(*mut uid_t) -> c_int>,
    pub get_target_gid: Option<unsafe extern "C" fn(*mut gid_t) -> c_int>,
    pub input_filter: Option<
        unsafe extern "C" fn(
            c_int,
            *const c_char,
            *mut *mut c_char,
            usize,
            *mut usize,
        ) -> c_uint,
    >,
    pub ini_defaults: Option<unsafe extern "C" fn(*mut HashTable)>,
    pub phpinfo_as_text: c_int,
    pub ini_entries: *const c_char,
    pub additional_functions: *const zend_function_entry,
    pub input_filter_init: Option<unsafe extern "C" fn() -> c_uint>,
    pub pre_request_init: Option<unsafe extern "C" fn() -> c_int>,
}

impl Default for sapi_module_struct {
    fn default() -> Self {
        Self {
            name: std::ptr::null_mut(),
            pretty_name: std::ptr::null_mut(),
            startup: None,
            shutdown: None,
            activate: None,
            deactivate: None,
            ub_write: None,
            flush: None,
            get_stat: None,
            getenv: None,
            sapi_error: None,
            header_handler: None,
            send_headers: None,
            send_header: None,
            read_post: None,
            read_cookies: None,
            register_server_variables: None,
            log_message: None,
            get_request_time: None,
            terminate_process: None,
            php_ini_path_override: std::ptr::null_mut(),
            default_post_reader: None,
            treat_data: None,
            executable_location: std::ptr::null_mut(),
            php_ini_ignore: 0,
            php_ini_ignore_cwd: 0,
            get_fd: None,
            force_http_10: None,
            get_target_uid: None,
            get_target_gid: None,
            input_filter: None,
            ini_defaults: None,
            phpinfo_as_text: 0,
            ini_entries: std::ptr::null(),
            additional_functions: std::ptr::null(),
            input_filter_init: None,
            pre_request_init: None,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct zend_stream {
    pub handle: *mut c_void,
    pub isatty: c_int,
    pub reader: *mut c_void,
    pub fsizer: *mut c_void,
    pub closer: *mut c_void,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union zend_file_handle_union {
    pub fp: *mut c_void,
    pub stream: zend_stream,
}

#[repr(C)]
pub struct zend_file_handle {
    pub handle: zend_file_handle_union,
    pub filename: *mut c_void,
    pub opened_path: *mut c_void,
    pub handle_type: u8,
    pub primary_script: u8,
    pub in_list: u8,
    _padding: [u8; 5],
    pub buf: *mut c_char,
    pub len: usize,
}

impl Default for zend_file_handle {
    fn default() -> Self {
        Self {
            handle: zend_file_handle_union {
                fp: std::ptr::null_mut(),
            },
            filename: std::ptr::null_mut(),
            opened_path: std::ptr::null_mut(),
            handle_type: ZEND_HANDLE_FILENAME,
            primary_script: 0,
            in_list: 0,
            _padding: [0; 5],
            buf: std::ptr::null_mut(),
            len: 0,
        }
    }
}

#[repr(C)]
pub struct zend_refcounted_h {
    pub refcount: u32,
    pub type_info: u32,
}

#[repr(C)]
pub struct zend_string {
    pub gc: zend_refcounted_h,
    pub h: u64,
    pub len: usize,
    pub val: [c_char; 1],
}

// SAFETY: zend_string is variable-length (val extends past the struct).
// Allocate with _emalloc(_ZSTR_STRUCT_SIZE(len)), then memcpy.
pub const GC_STRING: u32 = 6;

pub unsafe fn zend_string_init_rust(s: &[u8]) -> *mut zend_string {
    let struct_size = std::mem::size_of::<zend_string>() - 1 + s.len() + 1;
    let aligned = (struct_size + 7) & !7;
    let ptr = _emalloc(aligned) as *mut zend_string;

    (*ptr).gc.refcount = 1;
    (*ptr).gc.type_info = GC_STRING | (1 << 8);
    (*ptr).h = 0;
    (*ptr).len = s.len();

    std::ptr::copy_nonoverlapping(
        s.as_ptr(),
        (*ptr).val.as_mut_ptr() as *mut u8,
        s.len(),
    );
    *((*ptr)
        .val
        .as_mut_ptr()
        .add(s.len())) = 0;

    ptr
}

// Function pointer exported by PHP for creating interned zend_string values.
pub type zend_string_init_interned_func_t = Option<
    unsafe extern "C" fn(
        str: *const c_char,
        size: usize,
        permanent: bool,
    ) -> *mut zend_string,
>;

extern "C" {
    pub fn sapi_startup(sapi_module: *mut sapi_module_struct);
    pub fn sapi_shutdown();
    pub fn php_module_startup(
        sapi_module: *mut sapi_module_struct,
        additional_module: *mut zend_module_entry,
    ) -> c_int;
    pub fn php_module_shutdown();
    pub fn php_request_startup() -> c_int;
    pub fn php_request_shutdown(dummy: *mut c_void);
    pub fn php_output_flush_all();
    pub fn php_default_treat_data(
        arg: c_int,
        str: *mut c_char,
        dest_array: *mut zval,
    );
    pub fn php_execute_script(primary_file: *mut zend_file_handle) -> c_int;
    pub fn ripht_php_sapi_exit_status() -> c_int;
    pub fn zend_stream_init_filename(
        handle: *mut zend_file_handle,
        filename: *const c_char,
    );
    pub fn zend_destroy_file_handle(handle: *mut zend_file_handle);
    pub fn zend_alter_ini_entry_chars(
        name: *mut zend_string,
        value: *const c_char,
        value_length: usize,
        modify_type: c_int,
        stage: c_int,
    ) -> c_int;
    pub fn zend_ini_string(
        name: *const c_char,
        name_length: usize,
        orig: c_int,
    ) -> *mut c_char;
    pub fn php_default_input_filter(
        arg: c_int,
        var: *const c_char,
        val: *mut *mut c_char,
        val_len: usize,
        new_val_len: *mut usize,
    ) -> c_uint;
    pub fn _emalloc(size: usize) -> *mut c_void;

    pub fn php_register_variable_safe(
        var_name: *const c_char,
        val: *const c_char,
        val_len: usize,
        track_vars_array: *mut zval,
    );

    // idempotent
    pub fn sapi_send_headers() -> c_int;

    pub static mut zend_string_init_interned: zend_string_init_interned_func_t;

    pub static mut sapi_module: sapi_module_struct;
    pub static mut sapi_globals: sapi_globals_struct;
}

#[cfg(all(test, bindgen_available))]
mod bindgen_tests {
    #![allow(unused)]
    #![allow(dead_code)]
    #![allow(clippy::all)]
    #![allow(non_snake_case)]
    #![allow(improper_ctypes)]
    #![allow(non_camel_case_types)]
    #![allow(non_upper_case_globals)]

    mod bindgen_validation {
        include!(concat!(env!("OUT_DIR"), "/bindgen_validation.rs"));
    }

    use super::*;

    macro_rules! bindgen_offset_test {
        ($test_name:ident, $manual_type:ty, $bindgen_type:ty, $field:ident) => {
            #[test]
            fn $test_name() {
                let manual = std::mem::offset_of!($manual_type, $field);
                let bindgen = std::mem::offset_of!($bindgen_type, $field);
                assert_eq!(
                    manual,
                    bindgen,
                    "{} offset mismatch: manual={} bindgen={}",
                    stringify!($field),
                    manual,
                    bindgen
                );
            }
        };
    }

    bindgen_offset_test!(
        test_sapi_globals_server_context,
        sapi_globals_struct,
        bindgen_validation::_sapi_globals_struct,
        server_context
    );

    bindgen_offset_test!(
        test_sapi_globals_request_info,
        sapi_globals_struct,
        bindgen_validation::_sapi_globals_struct,
        request_info
    );

    bindgen_offset_test!(
        test_sapi_globals_sapi_headers,
        sapi_globals_struct,
        bindgen_validation::_sapi_globals_struct,
        sapi_headers
    );

    bindgen_offset_test!(
        test_sapi_module_name,
        sapi_module_struct,
        bindgen_validation::_sapi_module_struct,
        name
    );

    bindgen_offset_test!(
        test_sapi_module_startup,
        sapi_module_struct,
        bindgen_validation::_sapi_module_struct,
        startup
    );

    bindgen_offset_test!(
        test_request_info_request_method,
        sapi_request_info,
        bindgen_validation::sapi_request_info,
        request_method
    );

    bindgen_offset_test!(
        test_request_info_content_length,
        sapi_request_info,
        bindgen_validation::sapi_request_info,
        content_length
    );
}
