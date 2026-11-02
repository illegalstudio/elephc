//! Purpose:
//! Declarative eval registry entry for `readfile`.
//!
//! The parameter list mirrors php's
//! `readfile(string $filename, bool $use_include_path = false, ?resource $context = null)`. It
//! used to declare $filename alone, so `readfile($f, true)` -- which php accepts -- was a fatal.
//! $use_include_path is accepted and behaves as false and a non-null $context is refused, both
//! the same answers `file_get_contents()` gives for the same arguments.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the streaming file output helper.

eval_builtin! {
    contract: "readfile",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;
use crate::stream_wrappers;

/// Dispatches direct eval calls for the `readfile` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_readfile_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_readfile(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `readfile` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_readfile_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename, rest @ ..] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if rest.len() > 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    // $use_include_path is accepted and behaves as false, exactly as in file_get_contents():
    // eval resolves paths against the current directory only, which is what an include path of
    // "." would do anyway.
    eval_readfile_reject_context(rest.get(1).copied(), values)?;
    eval_readfile_result(*filename, context, values)
}

/// Evaluates PHP `readfile($filename)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_readfile(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.is_empty() || args.len() > 3 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_readfile_declared_values_result(&evaluated, context, values)
}

/// Refuses a non-null `$context` instead of silently dropping the caller's stream options.
///
/// The same answer `file_get_contents()` gives, for the same reason: accepting the argument and
/// ignoring it would report a successful read that did not honour the options it was handed.
fn eval_readfile_reject_context(
    stream_context: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(stream_context) = stream_context else {
        return Ok(());
    };
    if values.type_tag(stream_context)? == EVAL_TAG_NULL {
        return Ok(());
    }
    Err(EvalStatus::RuntimeFatal)
}

/// Streams one local file or supported wrapper to eval output.
pub(in crate::interpreter) fn eval_readfile_result(
    filename: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    if let Some(result) = eval_user_wrapper_readfile_result(&path, context, values)? {
        return Ok(result);
    }
    if let Some(local_path) = stream_wrappers::local_filesystem_path(&path) {
        let path = std::path::Path::new(&local_path);
        if path.is_dir() {
            return values.int(-1);
        }
    }
    let bytes = match super::file_get_contents::eval_read_path_or_wrapper_bytes(&path) {
        Ok(bytes) => bytes,
        Err(_) => return values.bool_value(false),
    };
    let output = values.string_bytes_value(&bytes)?;
    values.echo(output)?;
    values.int(i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?)
}
