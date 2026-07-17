//! Purpose:
//! Eval registry entry and implementation for `preg_split`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - This file owns registry metadata, direct dispatch, by-value dispatch, and
//! - result assembly for `preg_split()`.
use super::super::super::*;
use super::super::spec::EvalBuiltinDefaultValue;
use super::*;

eval_builtin! {
    name: "preg_split",
    area: Regex,
    params: [
        pattern,
        subject,
        limit = EvalBuiltinDefaultValue::Int(-1),
        flags = EvalBuiltinDefaultValue::Int(0),
    ],
    direct: PregSplit,
    values: PregSplit,
}


/// Evaluates PHP `preg_split()` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_preg_split(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [pattern, subject] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let subject = eval_expr(subject, context, scope, values)?;
            eval_preg_split_result(pattern, subject, None, None, values)
        }
        [pattern, subject, limit] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let subject = eval_expr(subject, context, scope, values)?;
            let limit = eval_expr(limit, context, scope, values)?;
            eval_preg_split_result(pattern, subject, Some(limit), None, values)
        }
        [pattern, subject, limit, flags] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let subject = eval_expr(subject, context, scope, values)?;
            let limit = eval_expr(limit, context, scope, values)?;
            let flags = eval_expr(flags, context, scope, values)?;
            eval_preg_split_result(pattern, subject, Some(limit), Some(flags), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Splits a subject string with eval-supported `preg_split()` flags.
pub(in crate::interpreter) fn eval_preg_split_result(
    pattern: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    limit: Option<RuntimeCellHandle>,
    flags: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let regex = eval_preg_regex(pattern, values)?;
    let subject = values.string_bytes(subject)?;
    let limit = eval_preg_split_limit(limit, values)?;
    let flags = eval_preg_split_flags(flags, values)?;
    let no_empty = flags & EVAL_PREG_SPLIT_NO_EMPTY != 0;
    let capture_delimiters = flags & EVAL_PREG_SPLIT_DELIM_CAPTURE != 0;
    let offset_capture = flags & EVAL_PREG_SPLIT_OFFSET_CAPTURE != 0;
    let mut pieces = Vec::<EvalPregSplitPiece>::new();
    let mut cursor = 0;

    for captures in regex.captures_iter(&subject) {
        let Some(matched) = captures.get(0) else {
            continue;
        };
        if eval_preg_split_reached_limit(&pieces, limit) {
            break;
        }
        eval_preg_split_push_piece(
            &mut pieces,
            &subject[cursor..matched.start()],
            cursor,
            no_empty,
        );
        if capture_delimiters {
            eval_preg_split_push_captures(&mut pieces, &subject, &captures, no_empty);
        }
        cursor = matched.end();
    }
    eval_preg_split_push_piece(&mut pieces, &subject[cursor..], cursor, no_empty);

    let mut result = values.array_new(pieces.len())?;
    for (index, piece) in pieces.iter().enumerate() {
        let key = values.int(i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        let value = eval_preg_split_piece_value(piece, offset_capture, values)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Dispatches by-value `preg_split()` calls after argument binding.
pub(in crate::interpreter) fn eval_preg_split_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [pattern, subject] => eval_preg_split_result(*pattern, *subject, None, None, values),
        [pattern, subject, limit] => {
            eval_preg_split_result(*pattern, *subject, Some(*limit), None, values)
        }
        [pattern, subject, limit, flags] => {
            eval_preg_split_result(*pattern, *subject, Some(*limit), Some(*flags), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
