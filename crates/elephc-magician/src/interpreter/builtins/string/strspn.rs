//! Purpose:
//! Declarative eval registry entry for `strspn` and the span implementation it shares with
//! `strcspn`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Mirrors php-src's `php_spn_common_handler`: a negative `$offset` counts from the end
//!   and clamps at zero, an `$offset` past the end answers 0, and a negative `$length`
//!   stops that many bytes before the end of the remaining tail.
//! - `strcspn()` with an empty mask counts the whole selected range, as php-src does.

eval_builtin! {
    contract: "strspn",
    area: String,
    direct: Strspn,
    values: Strspn,
}

use super::super::super::*;

/// Evaluates PHP `strspn(...)` / `strcspn(...)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_strspn(
    complement: bool,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_strspn_values_result(complement, &evaluated, context, values)
}

/// Dispatches already evaluated `strspn()` / `strcspn()` arguments.
pub(in crate::interpreter) fn eval_strspn_values_result(
    complement: bool,
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let function = if complement { "strcspn" } else { "strspn" };
    let (subject, mask, offset, length) = match args {
        [subject, mask] => (*subject, *mask, None, None),
        [subject, mask, offset] => (*subject, *mask, Some(*offset), None),
        [subject, mask, offset, length] => (*subject, *mask, Some(*offset), Some(*length)),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let subject = values.string_bytes(subject)?;
    let mask = values.string_bytes(mask)?;
    let offset = match offset {
        Some(offset) => {
            eval_reject_container_argument(function, 3, "offset", "int", offset, context, values)?;
            eval_int_value(offset, values)?
        }
        None => 0,
    };
    let length = match length {
        Some(length) if !values.is_null(length)? => {
            eval_reject_container_argument(function, 4, "length", "?int", length, context, values)?;
            Some(eval_int_value(length, values)?)
        }
        _ => None,
    };
    values.int(eval_span_count(&subject, &mask, offset, length, complement))
}

/// Counts the leading bytes of the selected range that are (or, for `complement`, are not)
/// in `mask`, using php-src's offset and length rules.
pub(in crate::interpreter) fn eval_span_count(
    subject: &[u8],
    mask: &[u8],
    offset: i64,
    length: Option<i64>,
    complement: bool,
) -> i64 {
    let total = subject.len() as i64;
    let start = if offset < 0 {
        (total + offset).max(0)
    } else if offset > total {
        return 0;
    } else {
        offset
    };
    let remaining = total - start;
    let len = match length {
        None => remaining,
        Some(length) if length < 0 => (remaining + length).max(0),
        Some(length) => length.min(remaining),
    };
    let range = &subject[start as usize..(start + len) as usize];
    range
        .iter()
        .take_while(|byte| mask.contains(byte) != complement)
        .count() as i64
}

/// Throws PHP's argument `TypeError` when a scalar parameter receives a value no scalar
/// coercion accepts: an array, or an object (for a string parameter, an object without
/// `__toString()`). The integer and string casts these builtins use afterwards would otherwise
/// turn such a value into a silent `0` / `""` and let the call succeed.
///
/// `type_label` is the declared type as PHP spells it in the message (`int`, `?int`,
/// `?string`); a label ending in `string` admits `Stringable` objects.
pub(in crate::interpreter) fn eval_reject_container_argument(
    function: &str,
    position: u32,
    parameter: &str,
    type_label: &str,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let tag = values.type_tag(value)?;
    let rejected = match tag {
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => true,
        EVAL_TAG_OBJECT => {
            !type_label.ends_with("string") || !crate::interpreter::dynamic_functions::eval_object_has_to_string(value, context, values)?
        }
        _ => false,
    };
    if !rejected {
        return Ok(());
    }
    let given = eval_given_type_name(value, context, values)?;
    eval_throw_type_error(
        &format!(
            "{function}(): Argument #{position} (${parameter}) must be of type {type_label}, {given} given"
        ),
        context,
        values,
    )
}
