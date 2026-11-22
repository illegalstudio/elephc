//! Purpose:
//! Shared Magician dispatch for the PHP BCMath procedural builtin surface.
//!
//! Called from:
//! - `EvalDirectHook::Bcmath` and `EvalValuesHook::Bcmath`.
//!
//! Key details:
//! - PHP coercions happen through `RuntimeValueOps` before bytes enter the decimal bridge.
//! - Magician calls the standalone C ABI so AOT and eval share one process scale.
//! - ABI errors become catchable `ValueError` or `DivisionByZeroError` objects.

use super::super::super::*;
use std::ptr;

const BCMATH_OK: i32 = 0;
const BCMATH_ERR_DIV_ZERO: i32 = 3;

type BinaryAbi = unsafe extern "C" fn(
    *const u8,
    usize,
    *const u8,
    usize,
    i64,
    i32,
    *mut *mut u8,
    *mut usize,
) -> i32;

type UnaryAbi = unsafe extern "C" fn(*const u8, usize, *mut *mut u8, *mut usize) -> i32;

extern "C" {
    /// Adds two decimal strings through the standalone bridge.
    fn elephc_bcmath_add(
        left_ptr: *const u8,
        left_len: usize,
        right_ptr: *const u8,
        right_len: usize,
        scale: i64,
        scale_is_null: i32,
        out_ptr: *mut *mut u8,
        out_len: *mut usize,
    ) -> i32;
    /// Subtracts two decimal strings through the standalone bridge.
    fn elephc_bcmath_sub(
        left_ptr: *const u8,
        left_len: usize,
        right_ptr: *const u8,
        right_len: usize,
        scale: i64,
        scale_is_null: i32,
        out_ptr: *mut *mut u8,
        out_len: *mut usize,
    ) -> i32;
    /// Multiplies two decimal strings through the standalone bridge.
    fn elephc_bcmath_mul(
        left_ptr: *const u8,
        left_len: usize,
        right_ptr: *const u8,
        right_len: usize,
        scale: i64,
        scale_is_null: i32,
        out_ptr: *mut *mut u8,
        out_len: *mut usize,
    ) -> i32;
    /// Divides two decimal strings through the standalone bridge.
    fn elephc_bcmath_div(
        left_ptr: *const u8,
        left_len: usize,
        right_ptr: *const u8,
        right_len: usize,
        scale: i64,
        scale_is_null: i32,
        out_ptr: *mut *mut u8,
        out_len: *mut usize,
    ) -> i32;
    /// Computes a decimal remainder through the standalone bridge.
    fn elephc_bcmath_mod(
        left_ptr: *const u8,
        left_len: usize,
        right_ptr: *const u8,
        right_len: usize,
        scale: i64,
        scale_is_null: i32,
        out_ptr: *mut *mut u8,
        out_len: *mut usize,
    ) -> i32;
    /// Computes quotient and remainder strings through the standalone bridge.
    fn elephc_bcmath_divmod(
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
    ) -> i32;
    /// Raises a decimal string to an integral exponent through the standalone bridge.
    fn elephc_bcmath_pow(
        base_ptr: *const u8,
        base_len: usize,
        exponent_ptr: *const u8,
        exponent_len: usize,
        scale: i64,
        scale_is_null: i32,
        out_ptr: *mut *mut u8,
        out_len: *mut usize,
    ) -> i32;
    /// Computes modular exponentiation through the standalone bridge.
    fn elephc_bcmath_powmod(
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
    ) -> i32;
    /// Computes a decimal square root through the standalone bridge.
    fn elephc_bcmath_sqrt(
        value_ptr: *const u8,
        value_len: usize,
        scale: i64,
        scale_is_null: i32,
        out_ptr: *mut *mut u8,
        out_len: *mut usize,
    ) -> i32;
    /// Compares two decimal strings through the standalone bridge.
    fn elephc_bcmath_comp(
        left_ptr: *const u8,
        left_len: usize,
        right_ptr: *const u8,
        right_len: usize,
        scale: i64,
        scale_is_null: i32,
        out_cmp: *mut i32,
    ) -> i32;
    /// Computes the decimal ceiling through the standalone bridge.
    fn elephc_bcmath_ceil(
        value_ptr: *const u8,
        value_len: usize,
        out_ptr: *mut *mut u8,
        out_len: *mut usize,
    ) -> i32;
    /// Computes the decimal floor through the standalone bridge.
    fn elephc_bcmath_floor(
        value_ptr: *const u8,
        value_len: usize,
        out_ptr: *mut *mut u8,
        out_len: *mut usize,
    ) -> i32;
    /// Rounds one decimal string through the standalone bridge.
    fn elephc_bcmath_round(
        value_ptr: *const u8,
        value_len: usize,
        precision: i64,
        mode: i64,
        out_ptr: *mut *mut u8,
        out_len: *mut usize,
    ) -> i32;
    /// Reads the bridge-owned process scale.
    fn elephc_bcmath_get_scale(out_scale: *mut i32) -> i32;
    /// Sets the bridge-owned process scale and returns its prior value.
    fn elephc_bcmath_set_scale(scale: i64, out_previous: *mut i32) -> i32;
    /// Borrows the bridge's current thread-local error message.
    fn elephc_bcmath_last_error(out_ptr: *mut *const u8, out_len: *mut usize) -> i32;
    /// Frees a string result allocated by the standalone bridge.
    fn elephc_bcmath_free(ptr: *mut u8, len: usize);
}

/// Evaluates BCMath arguments in source order before entering shared by-value dispatch.
pub(in crate::interpreter) fn eval_builtin_bcmath_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_bcmath_values_result(name, &evaluated, context, values)
}

/// Applies one BCMath builtin to already evaluated, PHP-ordered argument cells.
pub(in crate::interpreter) fn eval_bcmath_values_result(
    name: &str,
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let result = match name {
        "bcadd" => eval_binary(args, values, elephc_bcmath_add).map(BcmathResult::String),
        "bcsub" => eval_binary(args, values, elephc_bcmath_sub).map(BcmathResult::String),
        "bcmul" => eval_binary(args, values, elephc_bcmath_mul).map(BcmathResult::String),
        "bcdiv" => eval_binary(args, values, elephc_bcmath_div).map(BcmathResult::String),
        "bcmod" => eval_binary(args, values, elephc_bcmath_mod).map(BcmathResult::String),
        "bcpow" => eval_binary(args, values, elephc_bcmath_pow).map(BcmathResult::String),
        "bccomp" => eval_comp(args, values).map(BcmathResult::Int),
        "bcdivmod" => eval_divmod(args, values).map(BcmathResult::StringPair),
        "bcpowmod" => eval_powmod(args, values).map(BcmathResult::String),
        "bcsqrt" => eval_sqrt(args, values).map(BcmathResult::String),
        "bcceil" => eval_unary(args, values, elephc_bcmath_ceil).map(BcmathResult::String),
        "bcfloor" => eval_unary(args, values, elephc_bcmath_floor).map(BcmathResult::String),
        "bcround" => eval_round(args, values).map(BcmathResult::String),
        "bcscale" => eval_scale(args, values).map(BcmathResult::Int),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    match result {
        Ok(result) => materialize_result(result, values),
        Err(BcmathDispatchError::Engine(error)) => throw_engine_error(error, context, values),
        Err(BcmathDispatchError::Runtime(status)) => Err(status),
    }
}

/// Result shapes returned by the decimal bridge before runtime-cell allocation.
enum BcmathResult {
    /// One PHP integer.
    Int(i32),
    /// One PHP string.
    String(String),
    /// A two-element quotient/remainder string array.
    StringPair((String, String)),
}

/// Captures a stable ABI status and its PHP-compatible error message.
struct BcmathEngineError {
    /// Stable status code returned by `elephc-bcmath`.
    status: i32,
    /// Error text copied before a later ABI call can replace thread-local storage.
    message: String,
}

/// Separates decimal-domain failures from runtime-cell coercion failures.
enum BcmathDispatchError {
    /// A typed decimal-domain failure with its copied PHP message.
    Engine(BcmathEngineError),
    /// A Magician runtime-hook failure.
    Runtime(EvalStatus),
}

impl From<EvalStatus> for BcmathDispatchError {
    /// Preserves runtime-hook failures without converting them into decimal errors.
    fn from(status: EvalStatus) -> Self {
        Self::Runtime(status)
    }
}

/// Coerces two numeric strings plus an optional scale and invokes a binary ABI operation.
fn eval_binary(
    args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
    operation: BinaryAbi,
) -> Result<String, BcmathDispatchError> {
    let (left, right, scale) = match args {
        [left, right] => (*left, *right, None),
        [left, right, scale] => (*left, *right, Some(*scale)),
        _ => return Err(BcmathDispatchError::Runtime(EvalStatus::RuntimeFatal)),
    };
    let left = eval_decimal_bytes(left, values)?;
    let right = eval_decimal_bytes(right, values)?;
    let scale = eval_optional_scale(scale, values)?;
    call_binary(operation, &left, &right, scale)
}

/// Coerces one numeric string and invokes an exact-arity unary ABI operation.
fn eval_unary(
    args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
    operation: UnaryAbi,
) -> Result<String, BcmathDispatchError> {
    let [value] = args else {
        return Err(BcmathDispatchError::Runtime(EvalStatus::RuntimeFatal));
    };
    let value = eval_decimal_bytes(*value, values)?;
    call_unary(operation, &value)
}

/// Evaluates `bccomp()` and returns its signed comparison result.
fn eval_comp(
    args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<i32, BcmathDispatchError> {
    let (left, right, scale) = match args {
        [left, right] => (*left, *right, None),
        [left, right, scale] => (*left, *right, Some(*scale)),
        _ => return Err(BcmathDispatchError::Runtime(EvalStatus::RuntimeFatal)),
    };
    let left = eval_decimal_bytes(left, values)?;
    let right = eval_decimal_bytes(right, values)?;
    let scale = eval_optional_scale(scale, values)?;
    let (scale, scale_is_null) = encode_scale(scale);
    let mut result = 0;
    let status = unsafe {
        elephc_bcmath_comp(
            left.as_ptr(),
            left.len(),
            right.as_ptr(),
            right.len(),
            scale,
            scale_is_null,
            &mut result,
        )
    };
    check_status(status)?;
    Ok(result)
}

/// Evaluates `bcdivmod()` and copies both bridge-owned result strings.
fn eval_divmod(
    args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<(String, String), BcmathDispatchError> {
    let (left, right, scale) = match args {
        [left, right] => (*left, *right, None),
        [left, right, scale] => (*left, *right, Some(*scale)),
        _ => return Err(BcmathDispatchError::Runtime(EvalStatus::RuntimeFatal)),
    };
    let left = eval_decimal_bytes(left, values)?;
    let right = eval_decimal_bytes(right, values)?;
    let scale = eval_optional_scale(scale, values)?;
    let (scale, scale_is_null) = encode_scale(scale);
    let mut quotient_ptr = ptr::null_mut();
    let mut quotient_len = 0;
    let mut remainder_ptr = ptr::null_mut();
    let mut remainder_len = 0;
    let status = unsafe {
        elephc_bcmath_divmod(
            left.as_ptr(),
            left.len(),
            right.as_ptr(),
            right.len(),
            scale,
            scale_is_null,
            &mut quotient_ptr,
            &mut quotient_len,
            &mut remainder_ptr,
            &mut remainder_len,
        )
    };
    check_status(status)?;
    let quotient = copy_bridge_string(quotient_ptr, quotient_len)?;
    let remainder = copy_bridge_string(remainder_ptr, remainder_len)?;
    Ok((quotient, remainder))
}

/// Coerces the three integral strings and optional scale accepted by `bcpowmod()`.
fn eval_powmod(
    args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<String, BcmathDispatchError> {
    let (num, exponent, modulus, scale) = match args {
        [num, exponent, modulus] => (*num, *exponent, *modulus, None),
        [num, exponent, modulus, scale] => (*num, *exponent, *modulus, Some(*scale)),
        _ => return Err(BcmathDispatchError::Runtime(EvalStatus::RuntimeFatal)),
    };
    let num = eval_decimal_bytes(num, values)?;
    let exponent = eval_decimal_bytes(exponent, values)?;
    let modulus = eval_decimal_bytes(modulus, values)?;
    let scale = eval_optional_scale(scale, values)?;
    let (scale, scale_is_null) = encode_scale(scale);
    call_string(|out_ptr, out_len| unsafe {
        elephc_bcmath_powmod(
            num.as_ptr(),
            num.len(),
            exponent.as_ptr(),
            exponent.len(),
            modulus.as_ptr(),
            modulus.len(),
            scale,
            scale_is_null,
            out_ptr,
            out_len,
        )
    })
}

/// Coerces the numeric string and optional scale accepted by `bcsqrt()`.
fn eval_sqrt(
    args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<String, BcmathDispatchError> {
    let (num, scale) = match args {
        [num] => (*num, None),
        [num, scale] => (*num, Some(*scale)),
        _ => return Err(BcmathDispatchError::Runtime(EvalStatus::RuntimeFatal)),
    };
    let num = eval_decimal_bytes(num, values)?;
    let scale = eval_optional_scale(scale, values)?;
    let (scale, scale_is_null) = encode_scale(scale);
    call_string(|out_ptr, out_len| unsafe {
        elephc_bcmath_sqrt(
            num.as_ptr(),
            num.len(),
            scale,
            scale_is_null,
            out_ptr,
            out_len,
        )
    })
}

/// Coerces the number, precision, and mode accepted by `bcround()`.
fn eval_round(
    args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<String, BcmathDispatchError> {
    let (num, precision, mode) = match args {
        [num] => (*num, 0, 1),
        [num, precision] => (*num, eval_int_value(*precision, values)?, 1),
        [num, precision, mode] => (
            *num,
            eval_int_value(*precision, values)?,
            eval_int_value(*mode, values)?,
        ),
        _ => return Err(BcmathDispatchError::Runtime(EvalStatus::RuntimeFatal)),
    };
    let num = eval_decimal_bytes(num, values)?;
    call_string(|out_ptr, out_len| unsafe {
        elephc_bcmath_round(
            num.as_ptr(),
            num.len(),
            precision,
            mode,
            out_ptr,
            out_len,
        )
    })
}

/// Gets process scale or sets it and returns the previous value through the shared ABI.
fn eval_scale(
    args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<i32, BcmathDispatchError> {
    let mut result = 0;
    let status = match args {
        [] => unsafe { elephc_bcmath_get_scale(&mut result) },
        [scale] if values.is_null(*scale)? => unsafe { elephc_bcmath_get_scale(&mut result) },
        [scale] => unsafe {
            elephc_bcmath_set_scale(eval_int_value(*scale, values)?, &mut result)
        },
        _ => return Err(BcmathDispatchError::Runtime(EvalStatus::RuntimeFatal)),
    };
    check_status(status)?;
    Ok(result)
}

/// Converts a runtime cell through PHP string coercion into exact bridge input bytes.
fn eval_decimal_bytes(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<u8>, BcmathDispatchError> {
    Ok(values.string_bytes(value)?)
}

/// Resolves a missing or PHP-null scale as process-default and coerces an explicit value to int.
fn eval_optional_scale(
    scale: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<i64>, BcmathDispatchError> {
    match scale {
        None => Ok(None),
        Some(scale) if values.is_null(scale)? => Ok(None),
        Some(scale) => Ok(Some(eval_int_value(scale, values)?)),
    }
}

/// Converts an optional Rust scale into the C ABI value/null-flag pair.
fn encode_scale(scale: Option<i64>) -> (i64, i32) {
    match scale {
        Some(scale) => (scale, 0),
        None => (0, 1),
    }
}

/// Invokes a two-string optional-scale ABI function and copies its owned result.
fn call_binary(
    operation: BinaryAbi,
    left: &[u8],
    right: &[u8],
    scale: Option<i64>,
) -> Result<String, BcmathDispatchError> {
    let (scale, scale_is_null) = encode_scale(scale);
    call_string(|out_ptr, out_len| unsafe {
        operation(
            left.as_ptr(),
            left.len(),
            right.as_ptr(),
            right.len(),
            scale,
            scale_is_null,
            out_ptr,
            out_len,
        )
    })
}

/// Invokes a one-string ABI function and copies its owned result.
fn call_unary(
    operation: UnaryAbi,
    value: &[u8],
) -> Result<String, BcmathDispatchError> {
    call_string(|out_ptr, out_len| unsafe {
        operation(value.as_ptr(), value.len(), out_ptr, out_len)
    })
}

/// Invokes an ABI closure that writes one owned string result.
fn call_string(
    operation: impl FnOnce(*mut *mut u8, *mut usize) -> i32,
) -> Result<String, BcmathDispatchError> {
    let mut out_ptr = ptr::null_mut();
    let mut out_len = 0;
    let status = operation(&mut out_ptr, &mut out_len);
    check_status(status)?;
    copy_bridge_string(out_ptr, out_len)
}

/// Copies and releases one string returned by the BCMath bridge.
fn copy_bridge_string(
    out_ptr: *mut u8,
    out_len: usize,
) -> Result<String, BcmathDispatchError> {
    // The bridge accepts only the exact pointer and length it returned.
    let bytes = unsafe {
        elephc_memoir::runtime_bytes::copy_owned(out_ptr, out_len, |pointer| {
            elephc_bcmath_free(pointer.cast_mut(), out_len);
        })
    }
    .ok_or(BcmathDispatchError::Runtime(EvalStatus::RuntimeFatal))?;
    String::from_utf8(bytes)
        .map_err(|_| BcmathDispatchError::Runtime(EvalStatus::RuntimeFatal))
}

/// Converts a nonzero bridge status into an owned engine error.
fn check_status(status: i32) -> Result<(), BcmathDispatchError> {
    if status == BCMATH_OK {
        Ok(())
    } else {
        Err(BcmathDispatchError::Engine(read_engine_error(status)))
    }
}

/// Copies the thread-local bridge error before any later ABI call can replace it.
fn read_engine_error(status: i32) -> BcmathEngineError {
    let mut message_ptr = ptr::null();
    let mut message_len = 0;
    unsafe {
        elephc_bcmath_last_error(&mut message_ptr, &mut message_len);
    }
    let message = if message_ptr.is_null() || message_len == 0 {
        "BCMath operation failed".to_string()
    } else {
        String::from_utf8_lossy(unsafe {
            std::slice::from_raw_parts(message_ptr, message_len)
        })
        .into_owned()
    };
    BcmathEngineError { status, message }
}

/// Allocates the runtime-cell representation for one completed BCMath result.
fn materialize_result(
    result: BcmathResult,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match result {
        BcmathResult::Int(value) => values.int(i64::from(value)),
        BcmathResult::String(value) => values.string(&value),
        BcmathResult::StringPair((quotient, remainder)) => {
            let mut result = values.array_new(2)?;
            let zero = values.int(0)?;
            let quotient = values.string(&quotient)?;
            result = values.array_set(result, zero, quotient)?;
            let one = values.int(1)?;
            let remainder = values.string(&remainder)?;
            values.array_set(result, one, remainder)
        }
    }
}

/// Maps one stable bridge error onto the matching catchable PHP Throwable class.
fn throw_engine_error(
    error: BcmathEngineError,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if error.status == BCMATH_ERR_DIV_ZERO {
        eval_throw_builtin_division_by_zero_error(&error.message, context, values)
    } else {
        eval_throw_builtin_value_error(&error.message, context, values)
    }
}
