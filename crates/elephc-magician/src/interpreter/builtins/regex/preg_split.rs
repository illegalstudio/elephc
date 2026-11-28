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
use super::*;
use crate::interpreter::builtins::collection_builder::EvalArrayBuilder;

eval_builtin! {
    contract: "preg_split",
    area: Regex,
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
    if !(2..=4).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let operands = args.iter().collect::<Vec<_>>();
    with_eval_operands(&operands, context, scope, values, |operands, _, _, values| {
        eval_preg_split_values_result(operands, values)
    })
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

    // The limit is checked before each match runs: PHP never runs the matches past it, so
    // their errors must not fail the call.
    let mut matches = regex.captures_iter(&subject);
    while !eval_preg_split_reached_limit(&pieces, limit) {
        let Some(captures) = matches.next() else {
            break;
        };
        let Some(matched) = captures.get(0) else {
            continue;
        };
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
    if eval_preg_failed() {
        return values.bool_value(false);
    }
    eval_preg_split_push_piece(&mut pieces, &subject[cursor..], cursor, no_empty);

    let mut result = EvalArrayBuilder::indexed(values, pieces.len())?;
    for (index, piece) in pieces.iter().enumerate() {
        result.index(index, |values| eval_preg_split_piece_value(piece, offset_capture, values))?;
    }
    Ok(result.finish())
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
