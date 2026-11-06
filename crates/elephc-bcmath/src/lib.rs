//! Purpose:
//! Exposes the pure-Rust BCMath engine and its stable panic-free C ABI.
//!
//! Called from:
//! - Elephc AOT runtime helpers through `elephc_bcmath_*` function pointers.
//! - Magician through the same exported `elephc_bcmath_*` C ABI.
//!
//! Key details:
//! - String outputs are freshly allocated and released with `elephc_bcmath_free`.
//! - Every ABI operation catches panics and publishes a thread-local PHP error message.

mod error;
mod format;
mod num;
pub mod number;
mod number_abi;
mod ops;
mod parse;
mod pow;
mod round;
mod scale;

use std::cell::RefCell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;

pub use error::{
    BcError, BCMATH_ERR_DIV_ZERO, BCMATH_ERR_MALFORMED, BCMATH_ERR_POWMOD,
    BCMATH_ERR_POW_FRACTIONAL, BCMATH_ERR_POW_RANGE, BCMATH_ERR_ROUND_MODE,
    BCMATH_ERR_SCALE_RANGE, BCMATH_ERR_SQRT_NEGATIVE, BCMATH_OK,
};
pub use format::format_bcmath_number;
pub use num::BcNum;
pub use number_abi::*;
pub use ops::{bc_add, bc_comp, bc_div, bc_divmod, bc_mod, bc_mul, bc_sub};
pub use parse::parse_bcmath_number;
pub use pow::{bc_pow, bc_powmod, bc_sqrt};
pub use round::{bc_ceil, bc_floor, bc_round};
pub use scale::{get_scale, set_scale};

thread_local! {
    static LAST_ERROR: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Adds two numeric strings and returns a freshly allocated formatted result.
///
/// # Safety
/// Input pointers must be readable for their lengths and output pointers must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_bcmath_add(
    left_ptr: *const u8,
    left_len: usize,
    right_ptr: *const u8,
    right_len: usize,
    scale: i64,
    scale_is_null: i32,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    binary_abi(
        left_ptr,
        left_len,
        right_ptr,
        right_len,
        scale,
        scale_is_null,
        out_ptr,
        out_len,
        "bcadd",
        "num1",
        "num2",
        bc_add,
    )
}

/// Subtracts two numeric strings and returns a freshly allocated formatted result.
///
/// # Safety
/// Input pointers must be readable for their lengths and output pointers must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_bcmath_sub(
    left_ptr: *const u8,
    left_len: usize,
    right_ptr: *const u8,
    right_len: usize,
    scale: i64,
    scale_is_null: i32,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    binary_abi(
        left_ptr,
        left_len,
        right_ptr,
        right_len,
        scale,
        scale_is_null,
        out_ptr,
        out_len,
        "bcsub",
        "num1",
        "num2",
        bc_sub,
    )
}

/// Multiplies two numeric strings and returns a freshly allocated formatted result.
///
/// # Safety
/// Input pointers must be readable for their lengths and output pointers must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_bcmath_mul(
    left_ptr: *const u8,
    left_len: usize,
    right_ptr: *const u8,
    right_len: usize,
    scale: i64,
    scale_is_null: i32,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    binary_abi(
        left_ptr,
        left_len,
        right_ptr,
        right_len,
        scale,
        scale_is_null,
        out_ptr,
        out_len,
        "bcmul",
        "num1",
        "num2",
        bc_mul,
    )
}

/// Divides two numeric strings and returns a freshly allocated formatted result.
///
/// # Safety
/// Input pointers must be readable for their lengths and output pointers must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_bcmath_div(
    left_ptr: *const u8,
    left_len: usize,
    right_ptr: *const u8,
    right_len: usize,
    scale: i64,
    scale_is_null: i32,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    binary_abi(
        left_ptr,
        left_len,
        right_ptr,
        right_len,
        scale,
        scale_is_null,
        out_ptr,
        out_len,
        "bcdiv",
        "num1",
        "num2",
        bc_div,
    )
}

/// Computes the remainder of two numeric strings and returns a freshly allocated result.
///
/// # Safety
/// Input pointers must be readable for their lengths and output pointers must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_bcmath_mod(
    left_ptr: *const u8,
    left_len: usize,
    right_ptr: *const u8,
    right_len: usize,
    scale: i64,
    scale_is_null: i32,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    binary_abi(
        left_ptr,
        left_len,
        right_ptr,
        right_len,
        scale,
        scale_is_null,
        out_ptr,
        out_len,
        "bcmod",
        "num1",
        "num2",
        bc_mod,
    )
}

/// Computes quotient and remainder and returns two freshly allocated strings.
///
/// # Safety
/// Input pointers must be readable for their lengths and all output pointers must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_bcmath_divmod(
    left_ptr: *const u8,
    left_len: usize,
    right_ptr: *const u8,
    right_len: usize,
    scale: i64,
    scale_is_null: i32,
    quotient_ptr: *mut *mut u8,
    quotient_len: *mut usize,
    remainder_ptr: *mut *mut u8,
    remainder_len: *mut usize,
) -> i32 {
    if !valid_output_pair(quotient_ptr, quotient_len)
        || !valid_output_pair(remainder_ptr, remainder_len)
    {
        return panic_status();
    }
    clear_outputs(quotient_ptr, quotient_len);
    clear_outputs(remainder_ptr, remainder_len);
    run_abi(|| {
        let left = read_utf8(left_ptr, left_len, "bcdivmod", 1, "num1")?;
        let right = read_utf8(right_ptr, right_len, "bcdivmod", 2, "num2")?;
        let (quotient, remainder) = bc_divmod(left, right, optional_scale(scale, scale_is_null))?;
        write_output(quotient, quotient_ptr, quotient_len);
        write_output(remainder, remainder_ptr, remainder_len);
        Ok(())
    })
}

/// Raises a numeric string to an integral exponent and returns a fresh result string.
///
/// # Safety
/// Input pointers must be readable for their lengths and output pointers must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_bcmath_pow(
    base_ptr: *const u8,
    base_len: usize,
    exponent_ptr: *const u8,
    exponent_len: usize,
    scale: i64,
    scale_is_null: i32,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    binary_abi(
        base_ptr,
        base_len,
        exponent_ptr,
        exponent_len,
        scale,
        scale_is_null,
        out_ptr,
        out_len,
        "bcpow",
        "num",
        "exponent",
        bc_pow,
    )
}

/// Computes an integral modular exponent and returns a fresh formatted string.
///
/// # Safety
/// Input pointers must be readable for their lengths and output pointers must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_bcmath_powmod(
    base_ptr: *const u8,
    base_len: usize,
    exponent_ptr: *const u8,
    exponent_len: usize,
    modulus_ptr: *const u8,
    modulus_len: usize,
    scale: i64,
    scale_is_null: i32,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    if !valid_output_pair(out_ptr, out_len) {
        return panic_status();
    }
    clear_outputs(out_ptr, out_len);
    run_abi(|| {
        let base = read_utf8(base_ptr, base_len, "bcpowmod", 1, "num")?;
        let exponent = read_utf8(exponent_ptr, exponent_len, "bcpowmod", 2, "exponent")?;
        let modulus = read_utf8(modulus_ptr, modulus_len, "bcpowmod", 3, "modulus")?;
        let result = bc_powmod(
            base,
            exponent,
            modulus,
            optional_scale(scale, scale_is_null),
        )?;
        write_output(result, out_ptr, out_len);
        Ok(())
    })
}

/// Computes a truncated square root and returns a fresh formatted string.
///
/// # Safety
/// The input pointer must be readable for its length and output pointers must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_bcmath_sqrt(
    value_ptr: *const u8,
    value_len: usize,
    scale: i64,
    scale_is_null: i32,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    unary_scaled_abi(
        value_ptr,
        value_len,
        scale,
        scale_is_null,
        out_ptr,
        out_len,
        "bcsqrt",
        "num",
        bc_sqrt,
    )
}

/// Compares two numeric strings and writes `-1`, `0`, or `1`.
///
/// # Safety
/// Input pointers must be readable for their lengths and `out_cmp` must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_bcmath_comp(
    left_ptr: *const u8,
    left_len: usize,
    right_ptr: *const u8,
    right_len: usize,
    scale: i64,
    scale_is_null: i32,
    out_cmp: *mut i32,
) -> i32 {
    if out_cmp.is_null() {
        return panic_status();
    }
    *out_cmp = 0;
    run_abi(|| {
        let left = read_utf8(left_ptr, left_len, "bccomp", 1, "num1")?;
        let right = read_utf8(right_ptr, right_len, "bccomp", 2, "num2")?;
        *out_cmp = bc_comp(left, right, optional_scale(scale, scale_is_null))?;
        Ok(())
    })
}

/// Returns the least integer greater than or equal to a numeric string.
///
/// # Safety
/// The input pointer must be readable for its length and output pointers must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_bcmath_ceil(
    value_ptr: *const u8,
    value_len: usize,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    unary_abi(value_ptr, value_len, out_ptr, out_len, "bcceil", "num", bc_ceil)
}

/// Returns the greatest integer less than or equal to a numeric string.
///
/// # Safety
/// The input pointer must be readable for its length and output pointers must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_bcmath_floor(
    value_ptr: *const u8,
    value_len: usize,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    unary_abi(
        value_ptr,
        value_len,
        out_ptr,
        out_len,
        "bcfloor",
        "num",
        bc_floor,
    )
}

/// Rounds a numeric string with signed precision and a PHP rounding-mode integer.
///
/// # Safety
/// The input pointer must be readable for its length and output pointers must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_bcmath_round(
    value_ptr: *const u8,
    value_len: usize,
    precision: i64,
    mode: i64,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    if !valid_output_pair(out_ptr, out_len) {
        return panic_status();
    }
    clear_outputs(out_ptr, out_len);
    run_abi(|| {
        let value = read_utf8(value_ptr, value_len, "bcround", 1, "num")?;
        write_output(bc_round(value, precision, mode)?, out_ptr, out_len);
        Ok(())
    })
}

/// Writes the current process-wide BCMath scale.
///
/// # Safety
/// `out_scale` must point to writable `i32` storage.
#[no_mangle]
pub unsafe extern "C" fn elephc_bcmath_get_scale(out_scale: *mut i32) -> i32 {
    if out_scale.is_null() {
        return panic_status();
    }
    *out_scale = get_scale();
    clear_last_error();
    BCMATH_OK
}

/// Sets the process-wide BCMath scale and writes its previous value.
///
/// # Safety
/// `out_previous` must point to writable `i32` storage.
#[no_mangle]
pub unsafe extern "C" fn elephc_bcmath_set_scale(scale: i64, out_previous: *mut i32) -> i32 {
    if out_previous.is_null() {
        return panic_status();
    }
    *out_previous = 0;
    run_abi(|| {
        *out_previous = set_scale(scale)?;
        Ok(())
    })
}

/// Borrows the current thread's last PHP-compatible error-message bytes.
///
/// # Safety
/// `out_ptr` and `out_len` must point to writable storage. The returned bytes remain valid
/// until the next BCMath ABI call on the same thread.
#[no_mangle]
pub unsafe extern "C" fn elephc_bcmath_last_error(
    out_ptr: *mut *const u8,
    out_len: *mut usize,
) -> i32 {
    if out_ptr.is_null() || out_len.is_null() {
        return panic_status();
    }
    LAST_ERROR.with(|message| {
        let message = message.borrow();
        *out_ptr = message.as_ptr();
        *out_len = message.len();
    });
    BCMATH_OK
}

/// Releases a result buffer previously returned by this crate.
///
/// # Safety
/// `ptr` and `len` must be the exact pair produced by a successful BCMath ABI call and must
/// not have been freed before. A null pointer is accepted as a no-op.
#[no_mangle]
pub unsafe extern "C" fn elephc_bcmath_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let slice = ptr::slice_from_raw_parts_mut(ptr, len);
        drop(Box::from_raw(slice));
    }));
}

/// Executes one common two-string, optional-scale operation through the C ABI.
unsafe fn binary_abi(
    left_ptr: *const u8,
    left_len: usize,
    right_ptr: *const u8,
    right_len: usize,
    scale: i64,
    scale_is_null: i32,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
    func: &'static str,
    left_name: &'static str,
    right_name: &'static str,
    operation: fn(&str, &str, Option<i64>) -> Result<String, BcError>,
) -> i32 {
    if !valid_output_pair(out_ptr, out_len) {
        return panic_status();
    }
    clear_outputs(out_ptr, out_len);
    run_abi(|| {
        let left = read_utf8(left_ptr, left_len, func, 1, left_name)?;
        let right = read_utf8(right_ptr, right_len, func, 2, right_name)?;
        let result = operation(left, right, optional_scale(scale, scale_is_null))?;
        write_output(result, out_ptr, out_len);
        Ok(())
    })
}

/// Executes one common single-string, optional-scale operation through the C ABI.
unsafe fn unary_scaled_abi(
    value_ptr: *const u8,
    value_len: usize,
    scale: i64,
    scale_is_null: i32,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
    func: &'static str,
    arg_name: &'static str,
    operation: fn(&str, Option<i64>) -> Result<String, BcError>,
) -> i32 {
    if !valid_output_pair(out_ptr, out_len) {
        return panic_status();
    }
    clear_outputs(out_ptr, out_len);
    run_abi(|| {
        let value = read_utf8(value_ptr, value_len, func, 1, arg_name)?;
        let result = operation(value, optional_scale(scale, scale_is_null))?;
        write_output(result, out_ptr, out_len);
        Ok(())
    })
}

/// Executes one common single-string operation through the C ABI.
unsafe fn unary_abi(
    value_ptr: *const u8,
    value_len: usize,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
    func: &'static str,
    arg_name: &'static str,
    operation: fn(&str) -> Result<String, BcError>,
) -> i32 {
    if !valid_output_pair(out_ptr, out_len) {
        return panic_status();
    }
    clear_outputs(out_ptr, out_len);
    run_abi(|| {
        let value = read_utf8(value_ptr, value_len, func, 1, arg_name)?;
        write_output(operation(value)?, out_ptr, out_len);
        Ok(())
    })
}

/// Runs a fallible ABI body, catches panics, and translates typed errors into statuses.
fn run_abi(operation: impl FnOnce() -> Result<(), BcError>) -> i32 {
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(())) => {
            clear_last_error();
            BCMATH_OK
        }
        Ok(Err(error)) => {
            set_last_error(error.php_message());
            error.status_code()
        }
        Err(_) => panic_status(),
    }
}

/// Converts one pointer/length input pair to UTF-8 or a stable malformed status.
unsafe fn read_utf8<'a>(
    ptr: *const u8,
    len: usize,
    func: &'static str,
    arg_pos: u32,
    arg_name: &'static str,
) -> Result<&'a str, BcError> {
    if ptr.is_null() && len != 0 {
        return Err(BcError::Malformed {
            func,
            arg_pos,
            arg_name,
        });
    }
    let bytes = if len == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(ptr, len)
    };
    std::str::from_utf8(bytes).map_err(|_| BcError::Malformed {
        func,
        arg_pos,
        arg_name,
    })
}

/// Leaks one result string to its C caller and writes its exact pointer/length pair.
unsafe fn write_output(value: String, out_ptr: *mut *mut u8, out_len: *mut usize) {
    let mut bytes = value.into_bytes().into_boxed_slice();
    *out_ptr = bytes.as_mut_ptr();
    *out_len = bytes.len();
    std::mem::forget(bytes);
}

/// Converts the ABI null flag into the Rust optional-scale contract.
fn optional_scale(scale: i64, scale_is_null: i32) -> Option<i64> {
    if scale_is_null != 0 {
        None
    } else {
        Some(scale)
    }
}

/// Returns whether output pointer parameters can be safely initialized.
fn valid_output_pair(out_ptr: *mut *mut u8, out_len: *mut usize) -> bool {
    !out_ptr.is_null() && !out_len.is_null()
}

/// Initializes one output pair to a safe empty state before executing an operation.
unsafe fn clear_outputs(out_ptr: *mut *mut u8, out_len: *mut usize) {
    *out_ptr = ptr::null_mut();
    *out_len = 0;
}

/// Clears the current thread's last error after a successful operation.
fn clear_last_error() {
    LAST_ERROR.with(|message| message.borrow_mut().clear());
}

/// Stores one error message for retrieval by AOT runtime code.
fn set_last_error(message: String) {
    LAST_ERROR.with(|slot| *slot.borrow_mut() = message.into_bytes());
}

/// Publishes a deterministic status and message for an invalid ABI call or caught panic.
fn panic_status() -> i32 {
    set_last_error("BCMath operation failed".to_string());
    BCMATH_ERR_MALFORMED
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies a successful C ABI call allocates bytes that the matching free function accepts.
    #[test]
    fn c_abi_add_round_trips_owned_result() {
        let mut out_ptr = ptr::null_mut();
        let mut out_len = 0usize;
        let status = unsafe {
            elephc_bcmath_add(
                b"1.2".as_ptr(),
                3,
                b"3".as_ptr(),
                1,
                2,
                0,
                &mut out_ptr,
                &mut out_len,
            )
        };
        assert_eq!(status, BCMATH_OK);
        let value = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
        assert_eq!(value, b"4.20");
        unsafe { elephc_bcmath_free(out_ptr, out_len) };
    }

    /// Verifies the C ABI publishes the typed status and exact PHP error message.
    #[test]
    fn c_abi_error_publishes_message() {
        let mut out_ptr = ptr::null_mut();
        let mut out_len = 0usize;
        let status = unsafe {
            elephc_bcmath_div(
                b"1".as_ptr(),
                1,
                b"0".as_ptr(),
                1,
                0,
                0,
                &mut out_ptr,
                &mut out_len,
            )
        };
        assert_eq!(status, BCMATH_ERR_DIV_ZERO);
        let mut message_ptr = ptr::null();
        let mut message_len = 0;
        unsafe { elephc_bcmath_last_error(&mut message_ptr, &mut message_len) };
        let message = unsafe { std::slice::from_raw_parts(message_ptr, message_len) };
        assert_eq!(message, b"Division by zero");
    }

    /// Verifies invalid UTF-8 retains the called function and argument context.
    #[test]
    fn c_abi_invalid_utf8_publishes_argument_message() {
        let invalid = [0xff];
        let mut out_ptr = ptr::null_mut();
        let mut out_len = 0usize;
        let status = unsafe {
            elephc_bcmath_add(
                invalid.as_ptr(),
                invalid.len(),
                b"1".as_ptr(),
                1,
                0,
                0,
                &mut out_ptr,
                &mut out_len,
            )
        };
        assert_eq!(status, BCMATH_ERR_MALFORMED);
        let mut message_ptr = ptr::null();
        let mut message_len = 0;
        unsafe { elephc_bcmath_last_error(&mut message_ptr, &mut message_len) };
        let message = unsafe { std::slice::from_raw_parts(message_ptr, message_len) };
        assert_eq!(
            message,
            b"bcadd(): Argument #1 ($num1) is not well-formed"
        );
    }
}
