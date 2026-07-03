use std::os::raw::{c_char, c_void};

use super::callbacks;
use super::ffi::{self, zend_function_entry, zval};

unsafe extern "C" fn zif_dory_rust_ping(
    _execute_data: *mut c_void,
    return_value: *mut zval,
) {
    let bytes = std::panic::catch_unwind(|| b"pong".as_slice())
        .unwrap_or(b"".as_slice());

    (*return_value).set_string(ffi::zend_string_init_rust(bytes));
}

unsafe extern "C" fn zif_fastcgi_finish_request(
    _execute_data: *mut c_void,
    return_value: *mut zval,
) {
    let finished = std::panic::catch_unwind(callbacks::finish_current_request)
        .unwrap_or(false);

    (*return_value).set_bool(finished);
}

#[repr(C)]
struct ArgInfoHeader {
    name: *const c_char,
    type_info: [u8; 16],
    default_value: *const c_char,
}

unsafe impl Sync for ArgInfoHeader {}

static ARGINFO_0: [ArgInfoHeader; 1] = [ArgInfoHeader {
    name: 0usize as *const c_char,
    type_info: [0; 16],
    default_value: std::ptr::null(),
}];

macro_rules! func_entry {
    ($name:expr, $handler:expr, $arginfo:expr) => {
        zend_function_entry {
            fname: $name.as_ptr() as *const c_char,
            handler: Some($handler),
            arg_info: $arginfo.as_ptr() as *const c_void,
            num_args: ($arginfo.len() - 1) as u32,
            flags: 0,
            frameless_function_infos: std::ptr::null(),
            doc_comment: std::ptr::null(),
        }
    };
}

pub fn entries() -> &'static [zend_function_entry] {
    static ENTRIES: [zend_function_entry; 3] = [
        func_entry!(b"dory_rust_ping\0", zif_dory_rust_ping, ARGINFO_0),
        func_entry!(
            b"fastcgi_finish_request\0",
            zif_fastcgi_finish_request,
            ARGINFO_0
        ),
        zend_function_entry::end(),
    ];

    &ENTRIES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_function_entries_derive_num_args_from_arginfo() {
        for entry in entries()
            .iter()
            .take_while(|entry| !entry.fname.is_null())
        {
            assert_eq!(entry.num_args, 0);
            assert!(!entry.arg_info.is_null());
        }
    }
}
