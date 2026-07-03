//! Registering and executing a Rust native function from PHP.
//!
//! Run: `cargo run --example native_function`

use std::fs;
use std::os::raw::{c_char, c_void};

use ripht_php_sapi::native::{Function, ReturnValue};
use ripht_php_sapi::{native, RiphtSapi, SapiConfig, WebRequest};

unsafe extern "C" fn zif_dory_custom_ping(
    _execute_data: *mut c_void,
    return_value: *mut ReturnValue,
) {
    // SAFETY: PHP supplied `return_value` for this active native call.
    unsafe { native::set_string(return_value, b"hello Dory") };
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

static ARGINFO: [ArgInfo; 1] = [ArgInfo {
    name: std::ptr::null(),
    type_info: string_type(),
    default_value: std::ptr::null(),
}];

fn native_functions() -> &'static [Function] {
    static FUNCTIONS: [Function; 1] = [
        // SAFETY: function names and arginfo point to immutable static data, and
        // the handler uses PHP's native function calling convention.
        unsafe {
            Function::new_unchecked(
                c"dory_native_greeting".as_ptr(),
                zif_dory_custom_ping,
                ARGINFO.as_ptr() as *const c_void,
                0,
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
    fs::write(&script_path, "<?php echo dory_native_greeting();")?;

    let result = RiphtSapi::instance()
        .execute(WebRequest::get().build(&script_path)?)?;

    println!("{}", result.body_string());

    fs::remove_file(script_path).ok();

    Ok(())
}
