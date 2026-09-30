//! Purpose:
//! Declarative eval registry entry for `file_get_contents`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the one-shot file read helper.
//! - The parameter list mirrors PHP's
//!   `file_get_contents(string $filename, bool $use_include_path = false,
//!   ?resource $context = null, int $offset = 0, ?int $length = null)` and must stay
//!   shape-identical to the static registry declaration, which the builtin parity gate asserts.
//! - `$offset`/`$length` are applied to the bytes the read produced. That is what PHP's
//!   seek-then-read produces for a seekable stream, and it keeps the kept byte count bounded by
//!   the bytes actually available.
//! - A negative `$length` is php-src's catchable `ValueError`, raised BEFORE the file is opened
//!   so a missing file plus a negative length throws instead of warning.
//! - `$use_include_path` is accepted and behaves as `false`: eval resolves paths against the
//!   current directory only, which is what an include path of `"."` would do anyway.
//! - A non-null `$context` is refused rather than ignored, matching the compiler backend.

eval_builtin! {
    contract: "file_get_contents",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;
use crate::stream_wrappers;

/// php-src's `ValueError` for a negative `file_get_contents()` `$length`.
const FILE_GET_CONTENTS_NEGATIVE_LENGTH_MESSAGE: &str =
    "file_get_contents(): Argument #5 ($length) must be greater than or equal to 0";

/// The `$offset`/`$length` window a `file_get_contents()` call applies to the bytes it read.
#[derive(Clone, Copy)]
struct EvalFileReadRange {
    /// PHP's `$offset`: non-negative counts from the start, negative counts from the end.
    offset: i64,
    /// PHP's `$length`, or `None` when the caller passed `null` / omitted it.
    length: Option<i64>,
}

/// Dispatches direct eval calls for the `file_get_contents` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_file_get_contents_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_file_get_contents(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `file_get_contents` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_file_get_contents_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(filename) = evaluated_args.first().copied() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if evaluated_args.len() > 5 {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_file_get_contents_reject_context(evaluated_args.get(2).copied(), values)?;
    let range = eval_file_get_contents_range(
        evaluated_args.get(3).copied(),
        evaluated_args.get(4).copied(),
        context,
        values,
    )?;
    eval_file_get_contents_windowed_result(filename, range, context, values)
}

/// Evaluates PHP `file_get_contents($filename, …)` over its eval expressions.
pub(in crate::interpreter) fn eval_builtin_file_get_contents(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.is_empty() || args.len() > 5 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_file_get_contents_declared_values_result(&evaluated, context, values)
}

/// Reads one path and applies PHP's `$offset`/`$length` window to the bytes it produced.
fn eval_file_get_contents_windowed_result(
    filename: RuntimeCellHandle,
    range: EvalFileReadRange,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    if let Some(result) = eval_user_wrapper_file_get_contents_result(&path, context, values)? {
        return eval_file_get_contents_window_cell(result, range, values);
    }
    match eval_read_path_or_wrapper_bytes(&path) {
        Ok(bytes) => match eval_file_get_contents_window_bytes(&bytes, range) {
            Some(window) => values.string_bytes_value(window),
            None => {
                values.warning(&format!(
                    "Warning: file_get_contents(): Failed to seek to position {} in the stream\n",
                    range.offset
                ))?;
                values.bool_value(false)
            }
        },
        Err(reason) => {
            values.warning(&format!(
                "Warning: file_get_contents({path}): Failed to open stream: {reason}\n"
            ))?;
            values.bool_value(false)
        }
    }
}

/// Applies the window to a user stream wrapper's already-built result cell.
///
/// A wrapper that answered `false` stays `false`; a string result is windowed like a file read.
fn eval_file_get_contents_window_cell(
    result: RuntimeCellHandle,
    range: EvalFileReadRange,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if matches!(range.offset, 0) && range.length.is_none() {
        return Ok(result);
    }
    if values.type_tag(result)? != EVAL_TAG_STRING {
        return Ok(result);
    }
    let bytes = values.string_bytes(result)?;
    match eval_file_get_contents_window_bytes(&bytes, range) {
        Some(window) => values.string_bytes_value(window),
        None => {
            values.warning(&format!(
                "Warning: file_get_contents(): Failed to seek to position {} in the stream\n",
                range.offset
            ))?;
            values.bool_value(false)
        }
    }
}

/// Returns the requested byte window, or `None` when the seek lands before the first byte.
///
/// A non-negative `$offset` past the end is not an error in PHP: the stream seeks there and the
/// read answers with an empty string. Only a negative `$offset` whose magnitude exceeds the byte
/// count fails the seek. The kept byte count is bounded by the bytes that remain after the start
/// position, so a huge `$length` can never index past the buffer.
fn eval_file_get_contents_window_bytes(bytes: &[u8], range: EvalFileReadRange) -> Option<&[u8]> {
    let len = i64::try_from(bytes.len()).ok()?;
    let start = if range.offset < 0 {
        len.checked_add(range.offset)?
    } else {
        range.offset
    };
    if start < 0 {
        return None;
    }
    let start = start.min(len) as usize;
    let available = bytes.len() - start;
    let take = match range.length {
        Some(length) if length < available as i64 => length.max(0) as usize,
        _ => available,
    };
    Some(&bytes[start..start + take])
}

/// Builds the `$offset`/`$length` window, raising php-src's negative-`$length` `ValueError`.
fn eval_file_get_contents_range(
    offset: Option<RuntimeCellHandle>,
    length: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalFileReadRange, EvalStatus> {
    let offset = match offset {
        Some(offset) => eval_int_value(offset, values)?,
        None => 0,
    };
    let length = match length {
        Some(length) if values.type_tag(length)? != EVAL_TAG_NULL => {
            let length = eval_int_value(length, values)?;
            if length < 0 {
                return eval_file_get_contents_negative_length_error(context, values);
            }
            Some(length)
        }
        _ => None,
    };
    Ok(EvalFileReadRange { offset, length })
}

/// Refuses a non-null `$context` instead of silently dropping the caller's stream options.
fn eval_file_get_contents_reject_context(
    context: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(context) = context else {
        return Ok(());
    };
    if values.type_tag(context)? == EVAL_TAG_NULL {
        return Ok(());
    }
    Err(EvalStatus::RuntimeFatal)
}

/// Raises PHP's catchable `ValueError` for a negative `file_get_contents()` `$length`.
fn eval_file_get_contents_negative_length_error<T>(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    let exception = values.new_object("ValueError")?;
    let message = values.string(FILE_GET_CONTENTS_NEGATIVE_LENGTH_MESSAGE)?;
    let code = values.int(0)?;
    values.construct_object(exception, vec![message, code])?;
    context.set_pending_throw(exception);
    Err(EvalStatus::UncaughtThrowable)
}

/// Reads bytes from supported direct path or stream-wrapper URLs.
pub(in crate::interpreter) fn eval_read_path_or_wrapper_bytes(
    path: &str,
) -> Result<Vec<u8>, String> {
    // The error carries PHP's REASON rather than `()`. Discarding it forced every caller to
    // print "Failed to open stream" with nothing after it, which is wrong for a file that
    // exists and cannot be read; the wrapper paths have no errno behind them and keep the
    // wording PHP uses when nothing described the failure.
    let no_reason = || crate::stream_resources::EVAL_OPEN_DEFAULT_REASON.to_string();
    if stream_wrappers::is_data_stream(path) {
        return stream_wrappers::decode_data_uri(path).ok_or_else(no_reason);
    }
    if stream_wrappers::is_phar_stream(path) {
        return elephc_phar::extract_url_bytes(path.as_bytes()).ok_or_else(no_reason);
    }
    if stream_wrappers::is_http_stream(path) {
        return stream_wrappers::read_http_url(path).ok_or_else(no_reason);
    }
    let Some(path) = stream_wrappers::local_filesystem_path(path) else {
        return Err(no_reason());
    };
    std::fs::read(path).map_err(|error| crate::stream_resources::eval_open_failure_reason(&error))
}
