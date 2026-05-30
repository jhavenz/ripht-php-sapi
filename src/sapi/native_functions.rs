use std::os::raw::{c_char, c_void};

use super::ffi::{self, zend_function_entry, zval};

unsafe extern "C" fn zif_dory_rust_ping(_execute_data: *mut c_void, return_value: *mut zval) {
    (*return_value).set_string(ffi::zend_string_init_rust(b"pong"));
}

static DORY_FUNC_NAME: &[u8] = b"dory_rust_ping\0";

#[repr(C)]
struct ArgInfoHeader {
    name: *const c_char,
    type_info: [u8; 16],
    default_value: *const c_char,
}

unsafe impl Sync for ArgInfoHeader {}

static ARGINFO_PING: [ArgInfoHeader; 1] = [ArgInfoHeader {
    name: 0usize as *const c_char,
    type_info: [0; 16],
    default_value: std::ptr::null(),
}];

pub fn entries() -> &'static [zend_function_entry] {
    static ENTRIES: [zend_function_entry; 2] = [
        zend_function_entry {
            fname: DORY_FUNC_NAME.as_ptr() as *const c_char,
            handler: Some(zif_dory_rust_ping),
            arg_info: ARGINFO_PING.as_ptr() as *const c_void,
            num_args: 0,
            flags: 0,
            frameless_function_infos: std::ptr::null(),
            doc_comment: std::ptr::null(),
        },
        zend_function_entry::end(),
    ];

    &ENTRIES
}
