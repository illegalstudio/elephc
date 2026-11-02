//! Purpose:
//! Shared implementation of the six fixed-base conversions: `dechex`, `decbin`, `decoct`
//! (int to numeral) and `hexdec`, `bindec`, `octdec` (numeral to int, widening to float).
//!
//! Called from:
//! - The six home files beside this one, through the `BaseDigits` hooks.
//!
//! Key details:
//! - Reuses `base_convert`'s php-src transcription: a negative int renders as its unsigned
//!   64-bit value, invalid digits are skipped, and a numeral past `PHP_INT_MAX` widens to float.

use super::super::super::*;
use super::base_convert::{eval_base_to_number, eval_number_to_base, ParsedNumeral};

/// Returns the base a fixed-base builtin converts from or to.
fn base_of(name: &str) -> Option<(u32, bool)> {
    match name {
        "dechex" => Some((16, true)),
        "decbin" => Some((2, true)),
        "decoct" => Some((8, true)),
        "hexdec" => Some((16, false)),
        "bindec" => Some((2, false)),
        "octdec" => Some((8, false)),
        _ => None,
    }
}

/// Evaluates one fixed-base builtin from unevaluated call-site expressions.
pub(in crate::interpreter) fn eval_builtin_base_digits(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_base_digits_result(name, value, values)
}

/// Converts one already evaluated argument.
pub(in crate::interpreter) fn eval_base_digits_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((base, from_decimal)) = base_of(name) else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if from_decimal {
        let number = eval_int_value(value, values)?;
        return values.string_bytes_value(&eval_number_to_base(ParsedNumeral::Int(number), base));
    }
    let bytes = values.string_bytes(value)?;
    match eval_base_to_number(&bytes, base) {
        ParsedNumeral::Int(number) => values.int(number),
        ParsedNumeral::Float(number) => values.float(number),
    }
}
