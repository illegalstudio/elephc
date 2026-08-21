//! Purpose:
//! Declarative eval registry entry for PHP's `iconv()` plus the glue every iconv eval
//! builtin shares.
//!
//! Called from:
//! - `crate::interpreter::builtins::string` and the sibling `iconv_*` home files.
//!
//! Key details:
//! - The conversion engine is `elephc-iconv`, the same crate the AOT bridge links, so both
//!   backends agree on charset coverage, diagnostics, and MIME behavior.
//! - Failures become PHP's diagnostic plus `false`; only `iconv_strpos()`'s out-of-range
//!   `$offset` throws, which it does through eval's pending-throw state.
//! - An omitted or `null` `$encoding` stays absent so the engine can tell it apart from an
//!   explicitly empty charset.

eval_builtin! {
    contract: "iconv",
    area: String,
    direct: Iconv,
    values: Iconv,
}

use elephc_iconv::{IconvError, IconvResult};

use super::super::super::*;

/// Applies PHP `iconv(...)` to already evaluated arguments.
pub(in crate::interpreter) fn eval_iconv_result(
    from: RuntimeCellHandle,
    to: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let from = values.string_bytes(from)?;
    let to = values.string_bytes(to)?;
    let subject = values.string_bytes(subject)?;
    let converted = elephc_iconv::convert(&from, &to, &subject);
    eval_iconv_bytes("iconv", converted, values)
}

/// Returns one nullable `$encoding` argument as bytes, or `None` when PHP passed `null`.
pub(in crate::interpreter) fn eval_iconv_charset(
    encoding: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<Vec<u8>>, EvalStatus> {
    match encoding {
        Some(encoding) if !values.is_null(encoding)? => Ok(Some(values.string_bytes(encoding)?)),
        _ => Ok(None),
    }
}

/// Materializes a byte-string outcome, or PHP's diagnostic plus `false`.
pub(in crate::interpreter) fn eval_iconv_bytes(
    function: &str,
    result: IconvResult<Vec<u8>>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match result {
        Ok(bytes) => values.string_bytes_value(&bytes),
        Err(error) => eval_iconv_failure(function, &error, values),
    }
}

/// Materializes an integer outcome, or PHP's diagnostic plus `false`.
pub(in crate::interpreter) fn eval_iconv_int(
    function: &str,
    result: IconvResult<usize>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match result {
        Ok(value) => {
            let value = i64::try_from(value).map_err(|_| EvalStatus::RuntimeFatal)?;
            values.int(value)
        }
        Err(error) => eval_iconv_failure(function, &error, values),
    }
}

/// Materializes a search outcome, distinguishing "no match" from a thrown `ValueError`.
pub(in crate::interpreter) fn eval_iconv_search(
    function: &str,
    result: Result<Option<usize>, elephc_iconv::SearchFailure>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match result {
        Ok(Some(position)) => {
            let position = i64::try_from(position).map_err(|_| EvalStatus::RuntimeFatal)?;
            values.int(position)
        }
        // PHP reports "no match" as `false` without any diagnostic.
        Ok(None) => values.bool_value(false),
        Err(elephc_iconv::SearchFailure::Conversion(error)) => {
            eval_iconv_failure(function, &error, values)
        }
        Err(elephc_iconv::SearchFailure::OffsetOutOfRange) => {
            eval_iconv_offset_error(function, context, values)
        }
    }
}

/// Emits php-src's diagnostic for one failure and answers PHP `false`.
///
/// The complete diagnostic line is passed through, severity label and newline included,
/// so an `iconv*()` call prints identically whether it was compiled or evaluated.
pub(in crate::interpreter) fn eval_iconv_failure(
    function: &str,
    error: &IconvError,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.warning(&error.diagnostic_line(function))?;
    values.bool_value(false)
}

/// Raises PHP 8's catchable `ValueError` for an `$offset` outside the haystack.
fn eval_iconv_offset_error(
    function: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let exception = values.new_object("ValueError")?;
    let message = values.string(&elephc_iconv::offset_value_error_message(function))?;
    let code = values.int(0)?;
    values.construct_object(exception, vec![message, code])?;
    context.set_pending_throw(exception);
    Err(EvalStatus::UncaughtThrowable)
}

/// Evaluates any iconv builtin from its unevaluated arguments, in PHP source order.
pub(in crate::interpreter) fn eval_builtin_iconv_call(
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
    eval_iconv_values(name, &evaluated, context, values)
}

/// Applies any iconv builtin to already evaluated arguments.
pub(in crate::interpreter) fn eval_iconv_values(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let argument = |index: usize| evaluated_args.get(index).copied();
    match (name, evaluated_args.len()) {
        ("iconv", 3) => eval_iconv_result(evaluated_args[0], evaluated_args[1], evaluated_args[2], values),
        ("iconv_strlen", 1..=2) => super::iconv_strlen::eval_iconv_strlen_result(
            evaluated_args[0],
            argument(1),
            values,
        ),
        ("iconv_substr", 2..=4) => super::iconv_substr::eval_iconv_substr_result(
            evaluated_args[0],
            evaluated_args[1],
            argument(2),
            argument(3),
            values,
        ),
        ("iconv_strpos", 2..=4) => super::iconv_strpos::eval_iconv_strpos_result(
            evaluated_args[0],
            evaluated_args[1],
            argument(2),
            argument(3),
            context,
            values,
        ),
        ("iconv_strrpos", 2..=3) => super::iconv_strrpos::eval_iconv_strrpos_result(
            evaluated_args[0],
            evaluated_args[1],
            argument(2),
            context,
            values,
        ),
        ("iconv_mime_encode", 2..=3) => super::iconv_mime_encode::eval_iconv_mime_encode_result(
            evaluated_args[0],
            evaluated_args[1],
            argument(2),
            values,
        ),
        ("iconv_mime_decode", 1..=3) => super::iconv_mime_decode::eval_iconv_mime_decode_result(
            evaluated_args[0],
            argument(1),
            argument(2),
            values,
        ),
        ("iconv_mime_decode_headers", 1..=3) => {
            super::iconv_mime_decode_headers::eval_iconv_mime_decode_headers_result(
                evaluated_args[0],
                argument(1),
                argument(2),
                values,
            )
        }
        ("iconv_get_encoding", 0..=1) => {
            super::iconv_get_encoding::eval_iconv_get_encoding_result(argument(0), values)
        }
        ("iconv_set_encoding", 2) => super::iconv_set_encoding::eval_iconv_set_encoding_result(
            evaluated_args[0],
            evaluated_args[1],
            values,
        ),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
