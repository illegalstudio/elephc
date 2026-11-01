//! Purpose:
//! Eval implementation shared by PHP `strcspn()` and `strspn()`.
//!
//! Called from:
//! - Declarative string-span hooks in the eval builtin registry.
//!
//! Key details:
//! - Mirrors php-src's byte membership table and saturating offset/nullable-length window rules.

use super::super::super::*;

eval_builtin! {
    contract: "strcspn",
    area: String,
    direct: StringSpan,
    values: StringSpan,
}

/// Evaluates a string-span call in PHP source order.
pub(in crate::interpreter) fn eval_builtin_string_span_named(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() < 2 || args.len() > 4 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_string_span_named_result(name, &evaluated, values)
}

/// Applies php-src string-span semantics to already evaluated arguments.
pub(in crate::interpreter) fn eval_string_span_named_result(
    name: &str,
    args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (subject, characters, offset, length) = match args {
        [subject, characters] => (*subject, *characters, None, None),
        [subject, characters, offset] => (*subject, *characters, Some(*offset), None),
        [subject, characters, offset, length] => {
            (*subject, *characters, Some(*offset), Some(*length))
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let subject = values.string_bytes(subject)?;
    let characters = values.string_bytes(characters)?;
    let offset = match offset {
        Some(offset) => eval_int_value(offset, values)?,
        None => 0,
    };
    let length = match length {
        Some(length) if !values.is_null(length)? => Some(eval_int_value(length, values)?),
        _ => None,
    };
    let (start, length) = eval_string_span_window(subject.len(), offset, length);
    let mut membership = [false; 256];
    for byte in characters {
        membership[usize::from(byte)] = true;
    }
    let must_match = match name {
        "strcspn" => false,
        "strspn" => true,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let count = subject[start..start + length]
        .iter()
        .take_while(|byte| membership[usize::from(**byte)] == must_match)
        .count();
    values.int(i64::try_from(count).map_err(|_| EvalStatus::RuntimeFatal)?)
}

/// Resolves php-src's saturating string-span window without signed overflow.
fn eval_string_span_window(total: usize, offset: i64, length: Option<i64>) -> (usize, usize) {
    let start = if offset < 0 {
        let magnitude = offset.unsigned_abs();
        if magnitude > total as u64 {
            0
        } else {
            total - magnitude as usize
        }
    } else {
        usize::try_from(offset).unwrap_or(usize::MAX).min(total)
    };
    let remaining = total - start;
    let length = match length {
        None => remaining,
        Some(length) if length < 0 => {
            let magnitude = length.unsigned_abs();
            if magnitude > remaining as u64 {
                0
            } else {
                remaining - magnitude as usize
            }
        }
        Some(length) => usize::try_from(length).unwrap_or(usize::MAX).min(remaining),
    };
    (start, length)
}
