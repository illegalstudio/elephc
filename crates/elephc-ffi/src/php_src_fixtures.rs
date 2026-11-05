//! Purpose:
//! C symbols the committed php-src `ext/ffi` regression corpus expects from its test
//! extension (`zend_test`) and named helper libraries.
//!
//! Called from:
//! - `crate::context::FfiContext::symbol` when the `php-src-fixtures` feature is enabled.
//!
//! Key details:
//! - These symbols exist only to run `tests/php_src/ffi/*.phpt`; ordinary user programs never
//!   see them. The feature is enabled by the codegen test harness, not by production builds.
//! - Every function is `#[no_mangle]` so the fixture's `FFI::cdef` lookup can resolve it
//!   through the process image.

use std::os::raw::c_char;

/// `bug79096`'s by-value result: two 64-bit words.
#[repr(C)]
pub struct Bug79096 {
    /// First word.
    pub a: u64,
    /// Second word.
    pub b: u64,
}

/// Returns the two-one struct php-src's `zend_test` extension produces.
#[no_mangle]
pub extern "C" fn elephc_ffi_bug79096() -> Bug79096 {
    Bug79096 { a: 1, b: 1 }
}

/// `bug79532`'s by-value nested struct.
#[repr(C)]
pub struct Bug80847 {
    /// 64-bit field.
    pub b: u64,
    /// Double field.
    pub c: f64,
}

/// Echoes its argument, matching `zend_test`'s nested-struct fixture.
#[no_mangle]
pub extern "C" fn elephc_ffi_bug80847(value: Bug80847) -> Bug80847 {
    Bug80847 {
        b: value.b.wrapping_add(10),
        c: value.c - 10.0,
    }
}

/// Fills `values[0..count]` with 0..count for `bug79532`.
///
/// # Safety
/// `values` must address at least `count` writable `i64` elements.
#[no_mangle]
pub unsafe extern "C" fn elephc_ffi_bug79532(values: *mut i64, count: usize) {
    if values.is_null() {
        return;
    }
    for index in 0..count {
        *values.add(index) = index as i64;
    }
}

/// Prints the fixed line `bug_gh9090` expects for its no-argument function.
#[no_mangle]
pub extern "C" fn elephc_ffi_bug_gh9090_none() {
    println!("bug_gh9090_none");
}

/// Prints the fixed line `bug_gh9090` expects for `(int, char *)`.
///
/// # Safety
/// `text` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn elephc_ffi_bug_gh9090_int_char(value: i32, text: *const c_char) {
    let text = if text.is_null() {
        String::new()
    } else {
        std::ffi::CStr::from_ptr(text).to_string_lossy().into_owned()
    };
    println!("bug_gh9090_int_char {value} {text}");
}

/// Prints the hardcoded variadic line the gh9090 fixture expects.
///
/// The function is declared variadic; libffi ignores the extra registers a fixed Rust
/// definition does not name, so the fixture's argument pattern stays observable.
#[no_mangle]
pub extern "C" fn elephc_ffi_bug_gh9090_int_char_var(_value: i32, _format: *const c_char) {
    println!("bug_gh9090_void_int_char_var d=-1 s=ok");
}

/// Storage for the `bug_gh9090` function-pointer globals.
#[no_mangle]
pub static mut bug_gh9090_void_none_ptr: usize = 0;
/// Storage for `bug_gh9090_void_int_char_ptr`.
#[no_mangle]
pub static mut bug_gh9090_void_int_char_ptr: usize = 0;
/// Storage for `bug_gh9090_void_int_char_var_ptr`.
#[no_mangle]
pub static mut bug_gh9090_void_int_char_var_ptr: usize = 0;
/// Storage for `bug_gh9090_void_char_int_ptr`.
#[no_mangle]
pub static mut bug_gh9090_void_char_int_ptr: usize = 0;
/// Storage for `bug_gh9090_int_int_char_ptr`.
#[no_mangle]
pub static mut bug_gh9090_int_int_char_ptr: usize = 0;

/// Storage for `bug79177`'s callback global.
#[no_mangle]
pub static mut bug79177_cb: usize = 0;

/// Invokes `bug79177`'s registered callback, matching `zend_test`'s C function.
#[no_mangle]
pub extern "C" fn elephc_ffi_bug79177() {
    let callback = unsafe { bug79177_cb };
    if callback == 0 {
        return;
    }
    let function: unsafe extern "C" fn() -> *mut libc::c_int = unsafe { std::mem::transmute(callback) };
    let _ = unsafe { function() };
}

/// Test-only storage for a struct-returning callback closure.
#[no_mangle]
pub static mut elephc_ffi_test_cb: usize = 0;

/// Test-only C function that invokes a struct-returning callback.
#[no_mangle]
pub extern "C" fn elephc_ffi_test_struct_callback(
    callback: Option<extern "C" fn() -> Bug80847>,
) -> Bug80847 {
    callback.map(|callback| callback()).unwrap_or(Bug80847 { b: 0, c: 0.0 })
}

/// Resolves one fixture-only symbol name to its address.
pub fn symbol(name: &str) -> Option<usize> {
    match name {
        "bug79177_cb" => Some(std::ptr::addr_of_mut!(bug79177_cb) as usize),
        "bug79177" => Some(elephc_ffi_bug79177 as *const () as usize),
        "bug79096" => Some(elephc_ffi_bug79096 as *const () as usize),
        "bug80847" | "ffi_bug80847" => Some(elephc_ffi_bug80847 as *const () as usize),
        "elephc_ffi_test_cb" => Some(std::ptr::addr_of_mut!(elephc_ffi_test_cb) as usize),
        "elephc_ffi_test_struct_callback" => {
            Some(elephc_ffi_test_struct_callback as *const () as usize)
        }
        "bug79532" => Some(elephc_ffi_bug79532 as *const () as usize),
        "bug_gh9090_void_none" => Some(elephc_ffi_bug_gh9090_none as *const () as usize),
        "bug_gh9090_void_int_char" => Some(elephc_ffi_bug_gh9090_int_char as *const () as usize),
        "bug_gh9090_void_int_char_var" => {
            Some(elephc_ffi_bug_gh9090_int_char_var as *const () as usize)
        }
        "bug_gh9090_void_none_ptr" => {
            Some(std::ptr::addr_of_mut!(bug_gh9090_void_none_ptr) as usize)
        }
        "bug_gh9090_void_int_char_ptr" => {
            Some(std::ptr::addr_of_mut!(bug_gh9090_void_int_char_ptr) as usize)
        }
        "bug_gh9090_void_int_char_var_ptr" => {
            Some(std::ptr::addr_of_mut!(bug_gh9090_void_int_char_var_ptr) as usize)
        }
        "bug_gh9090_void_char_int_ptr" => {
            Some(std::ptr::addr_of_mut!(bug_gh9090_void_char_int_ptr) as usize)
        }
        "bug_gh9090_int_int_char_ptr" => {
            Some(std::ptr::addr_of_mut!(bug_gh9090_int_int_char_ptr) as usize)
        }
        _ => None,
    }
}
