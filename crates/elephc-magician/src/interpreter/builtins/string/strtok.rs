//! Purpose:
//! Eval registry entry and implementation for `strtok`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - A transcription of php-src's `strtok`: leading delimiter bytes are skipped, the token runs to
//!   the next delimiter byte, that delimiter is consumed, and `false` means no token remains. An
//!   empty delimiter list yields the rest of the string. All of it is byte-wise.
//! - The state (the string and the next offset) lives in the REQUEST-scoped store
//!   (`context::request_statics`), so it is shared by every eval context of the request and
//!   forgotten at a `--web` request boundary. The compiled side keeps its own state in the
//!   `static` locals of `src/strtok_prelude.rs`; the two sides do not share a tokenizer, so a
//!   string started on one side must be continued on the same side.

use super::super::super::*;

eval_builtin! {
    contract: "strtok",
    area: String,
    direct: Strtok,
    values: Strtok,
}

/// Evaluates PHP `strtok(...)` from unevaluated call-site expressions.
pub(in crate::interpreter) fn eval_builtin_strtok(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_strtok_values(&evaluated, context, values)
}

/// Evaluates PHP `strtok(...)` from already-evaluated argument cells.
pub(in crate::interpreter) fn eval_strtok_values(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (first, second) = match evaluated_args {
        [a] => (*a, None),
        [a, b] => (*a, Some(*b)),
        _ => {
            let message = if evaluated_args.is_empty() {
                "strtok() expects at least 1 argument, 0 given".to_string()
            } else {
                format!(
                    "strtok() expects at most 2 arguments, {} given",
                    evaluated_args.len()
                )
            };
            return eval_throw_argument_count_error(&message, context, values);
        }
    };
    let first = values.string_bytes(first)?;
    // A null second argument is the one-argument form, exactly as in php.
    let second = match second {
        Some(cell) if !values.is_null(cell)? => Some(values.string_bytes(cell)?),
        _ => None,
    };
    let token = crate::context::with_request_strtok(|state| match second {
        Some(delimiters) => {
            *state = Some((first, 0));
            strtok_next(state, &delimiters)
        }
        None => strtok_next(state, &first),
    });
    match token {
        Some(bytes) => values.string_bytes_value(&bytes),
        None => values.bool_value(false),
    }
}

/// Takes the next token off the stored string, advancing the offset past its delimiter.
fn strtok_next(state: &mut Option<(Vec<u8>, usize)>, delimiters: &[u8]) -> Option<Vec<u8>> {
    let (string, offset) = state.as_mut()?;
    let is_delimiter = |byte: &u8| delimiters.contains(byte);
    while *offset < string.len() && is_delimiter(&string[*offset]) {
        *offset += 1;
    }
    if *offset >= string.len() {
        return None;
    }
    let start = *offset;
    while *offset < string.len() && !is_delimiter(&string[*offset]) {
        *offset += 1;
    }
    let token = string[start..*offset].to_vec();
    if *offset < string.len() {
        *offset += 1;
    }
    Some(token)
}
