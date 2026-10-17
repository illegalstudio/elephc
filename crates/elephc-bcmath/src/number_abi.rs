//! Purpose:
//! Exposes Number operations through the existing length-aware decimal C ABI shapes.
//!
//! Called from:
//! - Target-aware Number runtime trampolines and Magician decimal adapters.
//!
//! Key details:
//! - Reuses panic containment, owned result buffers, and thread-local error publication.
//! - No method reads the procedural process scale.

use super::*;

/// Defines a binary Number operation using the common optional-scale ABI.
macro_rules! binary {
    ($export:ident, $operation:ident, $method:literal) => {
        /// Evaluates one immutable Number method with its own automatic-scale rules.
        fn $operation(left: &str, right: &str, scale: Option<i64>) -> Result<String, BcError> {
            number::number_binary($method, left, right, scale)
        }

        /// Returns a newly allocated decimal result, or publishes a method-specific error.
        ///
        /// # Safety
        /// Inputs must be readable for their lengths and output pointers must be writable.
        #[no_mangle]
        pub unsafe extern "C" fn $export(
            left: *const u8, left_len: usize, right: *const u8, right_len: usize,
            scale: i64, scale_is_null: i32, out: *mut *mut u8, out_len: *mut usize,
        ) -> i32 {
            if !valid_output_pair(out, out_len) { return panic_status(); }
            clear_outputs(out, out_len);
            run_abi(|| {
                let function = concat!("BcMath\\Number::", $method);
                let rhs_name = if $method == "pow" { "exponent" } else { "num" };
                let lhs = read_utf8(left, left_len, function, 1, "num")?;
                let rhs = read_utf8(right, right_len, function, 1, rhs_name)?;
                write_output($operation(lhs, rhs, optional_scale(scale, scale_is_null))?, out, out_len);
                Ok(())
            })
        }
    };
}

binary!(elephc_bcmath_number_add, add, "add");
binary!(elephc_bcmath_number_sub, sub, "sub");
binary!(elephc_bcmath_number_mul, mul, "mul");
binary!(elephc_bcmath_number_div, div, "div");
binary!(elephc_bcmath_number_mod, modulo, "mod");
binary!(elephc_bcmath_number_pow, pow, "pow");

/// Defines unary operations whose result is a fresh decimal string.
macro_rules! unary {
    ($export:ident, $operation:path, $method:literal) => {
        /// Evaluates a Number constructor or integer-boundary method.
        ///
        /// # Safety
        /// Input bytes must be readable and both output pointers must be writable.
        #[no_mangle]
        pub unsafe extern "C" fn $export(
            value: *const u8, value_len: usize, out: *mut *mut u8, out_len: *mut usize,
        ) -> i32 {
            unary_abi(value, value_len, out, out_len,
                concat!("BcMath\\Number::", $method), "num", $operation)
        }
    };
}

unary!(elephc_bcmath_number_create, number::number_create, "__construct");
unary!(elephc_bcmath_number_ceil, ceil, "ceil");
unary!(elephc_bcmath_number_floor, floor, "floor");

/// Adapts the ceiling method to the unary ABI's function signature.
fn ceil(value: &str) -> Result<String, BcError> {
    number::number_rounding("ceil", value, 0, 1)
}

/// Adapts the floor method to the unary ABI's function signature.
fn floor(value: &str) -> Result<String, BcError> {
    number::number_rounding("floor", value, 0, 1)
}

/// Returns a Number square root with nullable automatic expansion.
///
/// # Safety
/// Input bytes must be readable and both output pointers must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_bcmath_number_sqrt(
    value: *const u8, value_len: usize, scale: i64, scale_is_null: i32,
    out: *mut *mut u8, out_len: *mut usize,
) -> i32 {
    unary_scaled_abi(value, value_len, scale, scale_is_null, out, out_len,
        "BcMath\\Number::sqrt", "num", number::number_sqrt)
}

/// Compares Number operands without consulting the procedural scale.
///
/// # Safety
/// Input bytes must be readable and the output integer must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_bcmath_number_comp(
    left: *const u8, left_len: usize, right: *const u8, right_len: usize,
    scale: i64, scale_is_null: i32, out: *mut i32,
) -> i32 {
    if out.is_null() { return panic_status(); }
    *out = 0;
    run_abi(|| {
        let lhs = read_utf8(left, left_len, "BcMath\\Number::compare", 1, "num")?;
        let rhs = read_utf8(right, right_len, "BcMath\\Number::compare", 1, "num")?;
        *out = number::number_compare(lhs, rhs, optional_scale(scale, scale_is_null))?;
        Ok(())
    })
}

/// Returns the Number quotient and remainder through two owned result pairs.
///
/// # Safety
/// Input bytes must be readable and all output pointers must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_bcmath_number_divmod(
    left: *const u8, left_len: usize, right: *const u8, right_len: usize,
    scale: i64, scale_is_null: i32,
    quotient: *mut *mut u8, quotient_len: *mut usize,
    remainder: *mut *mut u8, remainder_len: *mut usize,
) -> i32 {
    if !valid_output_pair(quotient, quotient_len) || !valid_output_pair(remainder, remainder_len) {
        return panic_status();
    }
    clear_outputs(quotient, quotient_len);
    clear_outputs(remainder, remainder_len);
    run_abi(|| {
        let lhs = read_utf8(left, left_len, "BcMath\\Number::divmod", 1, "num")?;
        let rhs = read_utf8(right, right_len, "BcMath\\Number::divmod", 1, "num")?;
        let (q, r) = number::number_divmod(lhs, rhs, optional_scale(scale, scale_is_null))?;
        write_output(q, quotient, quotient_len);
        write_output(r, remainder, remainder_len);
        Ok(())
    })
}

/// Returns a Number modular power with an automatic scale of zero.
///
/// # Safety
/// Input bytes must be readable and both output pointers must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_bcmath_number_powmod(
    base: *const u8, base_len: usize, exponent: *const u8, exponent_len: usize,
    modulus: *const u8, modulus_len: usize, scale: i64, scale_is_null: i32,
    out: *mut *mut u8, out_len: *mut usize,
) -> i32 {
    if !valid_output_pair(out, out_len) { return panic_status(); }
    clear_outputs(out, out_len);
    run_abi(|| {
        let base = read_utf8(base, base_len, "BcMath\\Number::powmod", 1, "num")?;
        let exponent = read_utf8(exponent, exponent_len, "BcMath\\Number::powmod", 1, "exponent")?;
        let modulus = read_utf8(modulus, modulus_len, "BcMath\\Number::powmod", 2, "modulus")?;
        let value = number::number_powmod(base, exponent, modulus, optional_scale(scale, scale_is_null))?;
        write_output(value, out, out_len);
        Ok(())
    })
}

/// Returns a rounded Number value using PHP's rounding-mode integer representation.
///
/// # Safety
/// Input bytes must be readable and both output pointers must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_bcmath_number_round(
    value: *const u8, value_len: usize, precision: i64, mode: i64,
    out: *mut *mut u8, out_len: *mut usize,
) -> i32 {
    if !valid_output_pair(out, out_len) { return panic_status(); }
    clear_outputs(out, out_len);
    run_abi(|| {
        let value = read_utf8(value, value_len, "BcMath\\Number::round", 1, "num")?;
        write_output(number::number_rounding("round", value, precision, mode)?, out, out_len);
        Ok(())
    })
}
