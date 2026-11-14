//! Purpose:
//! Eval registry entry and implementation for `intval`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Cast behavior is implemented here; shared scalar coercions still flow
//!   through `RuntimeValueOps`.
//! - PHP's optional `$base` applies only to a string subject; every other type keeps the
//!   plain `(int)` cast, which is why the base path is guarded by the cell's runtime tag.
//! - The base parser mirrors `strtol()` plus php-src's extra `0b` prefix, including its
//!   `PHP_INT_MAX`/`PHP_INT_MIN` saturation and its `0` answer for an out-of-range base.
//! - Checked profiles use the exact shared kernel and release source-order operand leases.

use super::super::super::*;
use crate::interpreter::runtime_ops::EVAL_TAG_STRING;

eval_builtin! {
    contract: "intval",
    area: Types,
    direct: Intval,
    values: Intval,
}

/// Evaluates PHP `intval()` over one eval expression and an optional base expression.
pub(in crate::interpreter) fn eval_builtin_intval(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (value, base) = match args {
        [value] => (value, None),
        [value, base] => (value, Some(base)),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let mut args = vec![value]; if let Some(base) = base { args.push(base); }
    with_eval_operands(&args, context, scope, values, |args, context, _, values| {
        eval_intval_result(args[0], args.get(1).copied(), context, values)
    })
}

/// Applies PHP `intval()` to one already evaluated value and optional base.
///
/// An omitted base, a base of exactly `10`, and a non-string subject all reduce to the plain
/// `(int)` cast, exactly as php-src's `PHP_FUNCTION(intval)` short-circuits.
pub(in crate::interpreter) fn eval_intval_result(
    value: RuntimeCellHandle,
    base: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if context.semantic_profile().contains(elephc_semantics::StrictFeature::Casts) {
        let base = match base { Some(base) => eval_int_value(base, values)?, None => 10 };
        if base == 10 || values.type_tag(value)? != EVAL_TAG_STRING {
            return eval_cast_value(&EvalCastType::Int, value, context, values);
        }
        let bytes = values.string_bytes(value)?;
        return match elephc_semantics::integer_cast_base::checked_string_to_int_base(&bytes, base) {
            Ok(value) => values.int(value),
            Err(failure) => eval_throw_builtin_exception("ValueError", failure.message(), context, values),
        };
    }
    let Some(base) = base else {
        return values.cast_int(value);
    };
    let base = eval_int_value(base, values)?;
    if base == 10 || values.type_tag(value)? != EVAL_TAG_STRING {
        return values.cast_int(value);
    }
    let bytes = values.string_bytes(value)?;
    let parsed = eval_intval_parse_base(&bytes, base);
    values.int(parsed)
}

/// Parses one PHP byte string the way `strtol()` does for `intval($string, $base)`.
///
/// Returns `0` for a base outside `0` and `2..=36`, saturates at `PHP_INT_MAX`/`PHP_INT_MIN`
/// instead of wrapping, and stops at the first byte that is not a digit of the resolved base.
fn eval_intval_parse_base(bytes: &[u8], base: i64) -> i64 {
    if base != 0 && !(2..=36).contains(&base) {
        return 0;
    }
    let mut rest = bytes;
    while let [first, tail @ ..] = rest {
        if *first == b' ' || (b'\t'..=b'\r').contains(first) {
            rest = tail;
        } else {
            break;
        }
    }
    let mut negative = false;
    if let [first, tail @ ..] = rest {
        if *first == b'-' || *first == b'+' {
            negative = *first == b'-';
            rest = tail;
        }
    }
    let mut base = base;
    if rest.len() >= 2 && rest[0] == b'0' {
        let marker = rest[1] | 0x20;
        if marker == b'x' && (base == 0 || base == 16) {
            base = 16;
            rest = &rest[2..];
        } else if marker == b'b' && (base == 0 || base == 2) {
            base = 2;
            rest = &rest[2..];
        }
    }
    if base == 0 {
        base = if rest.first() == Some(&b'0') { 8 } else { 10 };
    }
    let limit: u64 = if negative { 1u64 << 63 } else { i64::MAX as u64 };
    let base = base as u64;
    let mut accumulator: u64 = 0;
    for byte in rest {
        let Some(digit) = char::from(*byte).to_digit(36).map(u64::from) else {
            break;
        };
        if digit >= base {
            break;
        }
        match accumulator
            .checked_mul(base)
            .and_then(|shifted| shifted.checked_add(digit))
            .filter(|candidate| *candidate <= limit)
        {
            Some(candidate) => accumulator = candidate,
            None => {
                accumulator = limit;
                break;
            }
        }
    }
    if negative {
        (accumulator as i64).wrapping_neg()
    } else {
        accumulator as i64
    }
}
