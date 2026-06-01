//! Low-level native PHP function integration.
//!
//! Hosts can register native functions before PHP startup, while argument and
//! return helpers keep the call-frame assumptions centralized in ripht.

use std::marker::PhantomData;
use std::os::raw::{c_char, c_void};
use std::ptr::NonNull;

use super::ffi;
use super::ffi::zend_function_entry;
use super::ffi::zend_string;

pub type Handler = unsafe extern "C" fn(
    execute_data: *mut c_void,
    return_value: *mut ReturnValue,
);

pub type ReturnValue = ffi::zval;

#[derive(Clone, Copy, Debug)]
#[repr(transparent)]
pub struct Function {
    entry: zend_function_entry,
}

impl Function {
    /// Creates a native PHP function entry from raw Zend metadata.
    ///
    /// # Safety
    ///
    /// `fname` must point to a process-lifetime nul-terminated string.
    /// `arg_info` must point to process-lifetime Zend-compatible arginfo.
    /// `handler` must obey PHP's `zif_handler` calling convention.
    pub const unsafe fn new_unchecked(
        fname: *const c_char,
        handler: Handler,
        arg_info: *const c_void,
        num_args: u32,
    ) -> Self {
        Self {
            entry: zend_function_entry {
                fname,
                handler: Some(handler),
                arg_info,
                num_args,
                flags: 0,
                frameless_function_infos: std::ptr::null(),
                doc_comment: std::ptr::null(),
            },
        }
    }

    pub(crate) const fn entry(self) -> zend_function_entry {
        self.entry
    }
}

pub struct Call<'frame> {
    execute_data: *mut c_void,
    num_args: u32,
    _marker: PhantomData<&'frame ReturnValue>,
}

impl<'frame> Call<'frame> {
    /// Opens a borrowed view over PHP's current call frame.
    ///
    /// # Safety
    ///
    /// `execute_data` must be the valid call-frame pointer PHP passed to the
    /// active native function. The returned value must not outlive that call.
    pub unsafe fn from_execute_data(execute_data: *mut c_void) -> Self {
        // SAFETY: guaranteed by the caller.
        let num_args = unsafe { ffi::call_num_args(execute_data) };

        Self {
            execute_data,
            num_args,
            _marker: PhantomData,
        }
    }

    pub fn num_args(&self) -> u32 {
        self.num_args
    }

    pub fn arg(&self, n: usize) -> Option<NonNull<ReturnValue>> {
        if n == 0 || n as u32 > self.num_args {
            return None;
        }

        // SAFETY: the argument index was checked against PHP's call-frame count.
        NonNull::new(unsafe { ffi::call_arg_unchecked(self.execute_data, n) })
    }

    pub fn arg_string(&'frame self, n: usize) -> Option<&'frame [u8]> {
        let value = self.arg(n)?;

        // SAFETY: the zval pointer belongs to this call frame, and the returned
        // borrow is tied to the frame lifetime carried by `Call`.
        unsafe { value.as_ref().as_str() }
    }
}

/// # Safety
///
/// `return_value` must be the valid pointer PHP passed to the active native function.
pub unsafe fn set_null(return_value: *mut ReturnValue) {
    if !return_value.is_null() {
        // SAFETY: guaranteed by the caller.
        unsafe { (*return_value).set_null() };
    }
}

/// # Safety
///
/// `return_value` must be the valid pointer PHP passed to the active native function.
pub unsafe fn set_string(return_value: *mut ReturnValue, value: &[u8]) {
    if !return_value.is_null() {
        // SAFETY: guaranteed by the caller; PHP owns the allocated zend string.
        unsafe { (*return_value).set_string(zend_string(value)) };
    }
}

/// # Safety
///
/// Must be called while the PHP engine allocator is initialized.
pub unsafe fn zend_string(value: &[u8]) -> *mut zend_string {
    // SAFETY: guaranteed by the caller.
    unsafe { ffi::zend_string_init_rust(value) }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" fn fake_handler(
        _execute_data: *mut c_void,
        _return_value: *mut ReturnValue,
    ) {
    }

    #[test]
    fn function_wraps_static_zend_metadata() {
        let name = b"ripht_test_native\0";

        // SAFETY: the function name is static, this test does not invoke the handler,
        // and a null arginfo pointer is enough to validate metadata wrapping.
        let function = unsafe {
            Function::new_unchecked(
                name.as_ptr() as *const c_char,
                fake_handler,
                std::ptr::null(),
                2,
            )
        };

        let entry = function.entry();

        assert_eq!(entry.fname, name.as_ptr() as *const c_char);
        assert_eq!(entry.num_args, 2);
        assert!(entry.handler.is_some());
        assert!(entry.arg_info.is_null());
    }

    #[test]
    fn call_bounds_arguments_against_php_frame_count() {
        let mut frame = (0..7)
            .map(|_| ffi::zval {
                value: ffi::zend_value { lval: 0 },
                type_info: 0,
                _u2: 0,
            })
            .collect::<Vec<_>>();

        frame[2]._u2 = 2;

        // SAFETY: the zvals are locally owned and used only for this synthetic frame test.
        unsafe {
            frame[5].set_long(10);
            frame[6].set_long(20);
        }

        // SAFETY: `frame` is laid out to satisfy the limited call-frame offsets
        // used by `Call` for num_args and argument lookup.
        let call = unsafe {
            Call::from_execute_data(
                frame
                    .as_mut_ptr()
                    .cast::<c_void>(),
            )
        };

        assert_eq!(call.num_args(), 2);
        assert!(call.arg(0).is_none());
        assert_eq!(call.arg(1).unwrap().as_ptr(), &mut frame[5] as *mut _);
        assert_eq!(call.arg(2).unwrap().as_ptr(), &mut frame[6] as *mut _);
        assert!(call.arg(3).is_none());
    }
}
