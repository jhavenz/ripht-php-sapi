//! Registering and executing a Rust native function from PHP.
//!
//! Run: `cargo run --example native_function`

use std::os::raw::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

use ripht_php_sapi::native::{Call, Function, ReturnValue};
use ripht_php_sapi::{native, RiphtSapi, SapiConfig, WebRequest};

unsafe extern "C" fn zif_ripht_content_hash(
    execute_data: *mut c_void,
    return_value: *mut ReturnValue,
) {
    let result = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: PHP supplied `execute_data` for this active native call.
        let call = unsafe { Call::from_execute_data(execute_data) };
        let Some(value) = call.arg_string(1) else {
            // SAFETY: PHP supplied `return_value` for this active native call.
            unsafe { native::set_null(return_value) };
            return;
        };

        let hash = value
            .iter()
            .fold(0xcbf29ce484222325_u64, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
            });
        let encoded = format!("{hash:016x}");

        // SAFETY: PHP supplied `return_value` for this active native call.
        unsafe { native::set_string(return_value, encoded.as_bytes()) };
    }));

    if result.is_err() {
        // SAFETY: PHP supplied `return_value` for this active native call.
        unsafe { native::set_null(return_value) };
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ZendType {
    ptr: *const c_void,
    type_mask: u32,
    _padding: u32,
}

#[repr(C)]
struct ArgInfo {
    name: *const c_char,
    type_info: ZendType,
    default_value: *const c_char,
}

// SAFETY: arginfo rows are immutable process-lifetime static metadata.
unsafe impl Sync for ArgInfo {}

const IS_STRING: u32 = 6;

const fn string_type() -> ZendType {
    ZendType {
        ptr: std::ptr::null(),
        type_mask: 1 << IS_STRING,
        _padding: 0,
    }
}

static ARGINFO: [ArgInfo; 2] = [
    ArgInfo {
        name: std::ptr::null(),
        type_info: string_type(),
        default_value: std::ptr::null(),
    },
    ArgInfo {
        name: c"value".as_ptr(),
        type_info: string_type(),
        default_value: std::ptr::null(),
    },
];

fn native_functions() -> &'static [Function] {
    static FUNCTIONS: [Function; 1] = [
        // SAFETY: function names and arginfo point to immutable static data, and
        // the handler uses PHP's native function calling convention.
        unsafe {
            Function::new_unchecked(
                c"ripht_content_hash".as_ptr(),
                zif_ripht_content_hash,
                ARGINFO.as_ptr() as *const c_void,
                1,
            )
        },
    ];

    &FUNCTIONS
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    RiphtSapi::configure(
        SapiConfig::new()
            .sapi_name("cli")
            .native_functions(native_functions()),
    )?;

    let script_path = std::env::temp_dir()
        .join(format!("ripht-native-function-{}.php", std::process::id()));
    std::fs::write(
        &script_path,
        "<?php echo 'The rust computed hash is: '.ripht_content_hash('native php bridge');",
    )?;

    let result = RiphtSapi::instance()
        .execute(WebRequest::get().build(&script_path)?)?;

    println!("{}", result.body_string());

    std::fs::remove_file(script_path).ok();

    Ok(())
}
