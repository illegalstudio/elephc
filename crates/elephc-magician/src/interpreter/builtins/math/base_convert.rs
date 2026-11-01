//! Purpose:
//! Declarative eval registry entry and implementation for `base_convert`.
//!
//! Called from:
//! - `crate::interpreter::builtins::math`.
//!
//! Key details:
//! - Mirrors php-src's `_php_math_basetozval` + `_php_math_zvaltobase` pair, including the
//!   widening to `double` past `PHP_INT_MAX` and the deliberately lossy float render that
//!   makes `base_convert("ffffffffffffffff", 16, 10)` produce `"18446744073709552046"`.
//! - Characters that are not digits of `$from_base` are ignored rather than terminating the
//!   scan, and a base outside `2..=36` is php-src's `ValueError`, reported as a runtime fatal.

eval_builtin! {
    contract: "base_convert",
    area: Math,
    direct: BaseConvert,
    values: BaseConvert,
}

use super::super::super::*;

/// Digit alphabet php-src uses for every base up to 36.
const BASE_DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";

/// Largest digit count php-src's `_php_math_zvaltobase` float buffer can hold.
const MAX_FLOAT_DIGITS: usize = 64;

/// Numeric value parsed out of a numeral string, widened exactly where php-src widens.
pub(super) enum ParsedNumeral {
    /// The value still fits `PHP_INT_MAX` and renders exactly.
    Int(i64),
    /// The value overflowed and renders through php-src's lossy float loop.
    Float(f64),
}

/// Evaluates PHP `base_convert(...)` over one numeral and its two base arguments.
pub(in crate::interpreter) fn eval_builtin_base_convert(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [num, from_base, to_base] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let num = eval_expr(num, context, scope, values)?;
    let from_base = eval_expr(from_base, context, scope, values)?;
    let to_base = eval_expr(to_base, context, scope, values)?;
    eval_base_convert_result(num, from_base, to_base, values)
}

/// Re-renders an already evaluated numeral from one base into another.
pub(in crate::interpreter) fn eval_base_convert_result(
    num: RuntimeCellHandle,
    from_base: RuntimeCellHandle,
    to_base: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(num)?;
    let from_base = eval_int_value(from_base, values)?;
    let to_base = eval_int_value(to_base, values)?;
    if !(2..=36).contains(&from_base) || !(2..=36).contains(&to_base) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let parsed = eval_base_to_number(&bytes, from_base as u32);
    let output = eval_number_to_base(parsed, to_base as u32);
    values.string_bytes_value(&output)
}

/// Parses `bytes` as a numeral in `base`, widening to `f64` exactly where php-src does.
pub(super) fn eval_base_to_number(bytes: &[u8], base: u32) -> ParsedNumeral {
    let base_i64 = i64::from(base);
    let cutoff = i64::MAX / base_i64;
    let cutlim = i64::MAX % base_i64;
    let mut accumulator = 0i64;
    let mut widened = 0f64;
    let mut is_float = false;
    for byte in bytes {
        let digit = match byte {
            b'0'..=b'9' => u32::from(byte - b'0'),
            b'A'..=b'Z' => u32::from(byte - b'A') + 10,
            b'a'..=b'z' => u32::from(byte - b'a') + 10,
            _ => continue,
        };
        if digit >= base {
            continue;
        }
        let digit = i64::from(digit);
        if is_float {
            widened = widened * f64::from(base) + digit as f64;
            continue;
        }
        if accumulator > cutoff || (accumulator == cutoff && digit > cutlim) {
            is_float = true;
            widened = accumulator as f64 * f64::from(base) + digit as f64;
            continue;
        }
        accumulator = accumulator * base_i64 + digit;
    }
    if is_float {
        ParsedNumeral::Float(widened)
    } else {
        ParsedNumeral::Int(accumulator)
    }
}

/// Renders a parsed numeral in `base`, reproducing php-src's exact and lossy paths.
pub(super) fn eval_number_to_base(parsed: ParsedNumeral, base: u32) -> Vec<u8> {
    let value = match parsed {
        ParsedNumeral::Int(value) => {
            let mut unsigned = value as u64;
            if unsigned == 0 {
                return b"0".to_vec();
            }
            let mut digits = Vec::new();
            while unsigned != 0 {
                digits.push(BASE_DIGITS[(unsigned % u64::from(base)) as usize]);
                unsigned /= u64::from(base);
            }
            digits.reverse();
            return digits;
        }
        ParsedNumeral::Float(value) => value,
    };

    let mut running = value.floor();
    if running.is_infinite() {
        return Vec::new();
    }
    let divisor = f64::from(base);
    let mut digits = Vec::new();
    loop {
        let remainder = running % divisor;
        digits.push(BASE_DIGITS[(remainder as i64).unsigned_abs() as usize % base as usize]);
        running /= divisor;
        if digits.len() >= MAX_FLOAT_DIGITS || running.abs() < 1.0 {
            break;
        }
    }
    digits.reverse();
    digits
}
