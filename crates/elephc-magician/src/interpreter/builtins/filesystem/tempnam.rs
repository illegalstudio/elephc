//! Purpose:
//! Declarative eval registry entry for `tempnam`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the temporary-name helper.

eval_builtin! {
    name: "tempnam",
    area: Filesystem,
    params: [directory, prefix],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `tempnam` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_tempnam_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_tempnam(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `tempnam` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_tempnam_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [directory, prefix] => eval_tempnam_result(*directory, *prefix, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `tempnam($directory, $prefix)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_tempnam(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [directory, prefix] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let directory = eval_expr(directory, context, scope, values)?;
    let prefix = eval_expr(prefix, context, scope, values)?;
    eval_tempnam_result(directory, prefix, values)
}

/// Creates a unique local temporary file and returns its path, or an empty string on failure.
pub(in crate::interpreter) fn eval_tempnam_result(
    directory: RuntimeCellHandle,
    prefix: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let directory = eval_path_string(directory, values)?;
    let prefix = values.string_bytes(prefix)?;
    let prefix = String::from_utf8_lossy(&prefix);
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    for attempt in 0..1000_u32 {
        let candidate =
            std::path::Path::new(&directory).join(eval_tempnam_filename(&prefix, nonce, attempt));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(_) => return values.string(candidate.to_string_lossy().as_ref()),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return values.string(""),
        }
    }
    values.string("")
}

/// Builds one deterministic tempnam candidate basename from prefix, process, and attempt data.
pub(in crate::interpreter) fn eval_tempnam_filename(
    prefix: &str,
    nonce: u128,
    attempt: u32,
) -> String {
    format!("{}{}_{:x}_{attempt}", prefix, std::process::id(), nonce)
}
