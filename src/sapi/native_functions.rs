use std::collections::HashMap;
use std::os::raw::{c_char, c_void};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};

use super::ffi::{self, zend_function_entry, zval};

static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);
static REGISTRY: Mutex<Option<HashMap<u64, Arc<String>>>> = Mutex::new(None);

fn registry() -> std::sync::MutexGuard<'static, Option<HashMap<u64, Arc<String>>>> {
    let mut guard = REGISTRY.lock().unwrap();

    if guard.is_none() {
        *guard = Some(HashMap::new());
    }

    guard
}

unsafe extern "C" fn zif_dory_rust_ping(_execute_data: *mut c_void, return_value: *mut zval) {
    (*return_value).set_string(ffi::zend_string_init_rust(b"pong"));
}

unsafe extern "C" fn zif_dory_argc(execute_data: *mut c_void, return_value: *mut zval) {
    let n = ffi::call_num_args(execute_data);

    (*return_value).set_long(n as i64);
}

unsafe extern "C" fn zif_dory_echo(execute_data: *mut c_void, return_value: *mut zval) {
    if ffi::call_num_args(execute_data) < 1 {
        (*return_value).set_null();
        return;
    }

    let arg = &*ffi::call_arg(execute_data, 1);

    match arg.as_str() {
        Some(bytes) => {
            let upper = String::from_utf8_lossy(bytes).to_uppercase();

            (*return_value).set_string(ffi::zend_string_init_rust(upper.as_bytes()));
        }
        None => (*return_value).set_null(),
    }
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

unsafe extern "C" fn zif_dory_store(execute_data: *mut c_void, return_value: *mut zval) {
    if ffi::call_num_args(execute_data) < 1 {
        (*return_value).set_long(-1);
        return;
    }

    let arg = &*ffi::call_arg(execute_data, 1);

    let data = match arg.as_str() {
        Some(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        None => {
            (*return_value).set_long(-1);
            return;
        }
    };

    let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);

    registry().as_mut().unwrap().insert(handle, Arc::new(data));

    (*return_value).set_long(handle as i64);
}

unsafe extern "C" fn zif_dory_fetch(execute_data: *mut c_void, return_value: *mut zval) {
    if ffi::call_num_args(execute_data) < 1 {
        (*return_value).set_null();
        return;
    }

    let handle = match (*ffi::call_arg(execute_data, 1)).as_long() {
        Some(h) => h as u64,
        None => {
            (*return_value).set_null();
            return;
        }
    };

    let guard = registry();

    match guard.as_ref().unwrap().get(&handle) {
        Some(arc) => (*return_value).set_string(ffi::zend_string_init_rust(arc.as_bytes())),
        None => (*return_value).set_null(),
    }
}

unsafe extern "C" fn zif_dory_free(execute_data: *mut c_void, return_value: *mut zval) {
    if ffi::call_num_args(execute_data) < 1 {
        (*return_value).set_long(0);
        return;
    }

    let handle = match (*ffi::call_arg(execute_data, 1)).as_long() {
        Some(h) => h as u64,
        None => {
            (*return_value).set_long(0);
            return;
        }
    };

    let removed = registry().as_mut().unwrap().remove(&handle).is_some();

    (*return_value).set_long(i64::from(removed));
}

unsafe extern "C" fn zif_dory_registry_count(_execute_data: *mut c_void, return_value: *mut zval) {
    let count = registry().as_ref().unwrap().len();

    (*return_value).set_long(count as i64);
}

static ARGINFO_1: [ArgInfoHeader; 2] = [
    ArgInfoHeader {
        name: 1usize as *const c_char,
        type_info: [0; 16],
        default_value: std::ptr::null(),
    },
    ArgInfoHeader {
        name: b"value\0".as_ptr() as *const c_char,
        type_info: [0; 16],
        default_value: std::ptr::null(),
    },
];

macro_rules! func_entry {
    ($name:expr, $handler:expr, $arginfo:expr, $num_args:expr) => {
        zend_function_entry {
            fname: $name.as_ptr() as *const c_char,
            handler: Some($handler),
            arg_info: $arginfo.as_ptr() as *const c_void,
            num_args: $num_args,
            flags: 0,
            frameless_function_infos: std::ptr::null(),
            doc_comment: std::ptr::null(),
        }
    };
}

pub fn entries() -> &'static [zend_function_entry] {
    static ENTRIES: [zend_function_entry; 8] = [
        func_entry!(b"dory_rust_ping\0", zif_dory_rust_ping, ARGINFO_0, 0),
        func_entry!(b"dory_argc\0", zif_dory_argc, ARGINFO_0, 0),
        func_entry!(b"dory_echo\0", zif_dory_echo, ARGINFO_1, 1),
        func_entry!(b"dory_store\0", zif_dory_store, ARGINFO_1, 1),
        func_entry!(b"dory_fetch\0", zif_dory_fetch, ARGINFO_1, 1),
        func_entry!(b"dory_free\0", zif_dory_free, ARGINFO_1, 1),
        func_entry!(b"dory_registry_count\0", zif_dory_registry_count, ARGINFO_0, 0),
        zend_function_entry::end(),
    ];

    &ENTRIES
}
