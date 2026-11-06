//! Purpose:
//! Eval registry entry and implementation for `getenv`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - Accepts 0–2 arguments, matching the shared catalogue. An omitted or null
//!   name answers the whole live environment as an associative array.
//! - `local_only` is evaluated for side effects and ignored: eval has no
//!   environment separate from the process's, same as AOT CLI.
//! - Missing names return false; present names and environment arrays preserve raw bytes.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;

use super::*;

eval_builtin! {
    contract: "getenv",
    area: NetworkEnv,
    direct: NetworkEnv,
    values: NetworkEnv,
}

/// Evaluates PHP `getenv()`, `getenv($name)`, and `getenv($name, $local_only)`.
pub(in crate::interpreter) fn eval_builtin_getenv(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_getenv_all_result(values),
        [name] => {
            let name = eval_expr(name, context, scope, values)?;
            eval_getenv_name_result(name, values)
        }
        [name, local_only] => {
            let name = eval_expr(name, context, scope, values)?;
            let _local_only = eval_expr(local_only, context, scope, values)?;
            eval_getenv_name_result(name, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Reads one environment variable, or the whole environment when `$name` is null.
pub(in crate::interpreter) fn eval_getenv_name_result(
    name: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.is_null(name)? {
        return eval_getenv_all_result(values);
    }
    eval_getenv_result(name, values)
}

/// Reads one environment variable without Unicode conversion, returning false when absent.
pub(in crate::interpreter) fn eval_getenv_result(
    name: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let name = values.string_bytes(name)?;
    match std::env::var_os(OsStr::from_bytes(&name)) {
        Some(value) => values.string_bytes_value(value.as_bytes()),
        None => values.bool_value(false),
    }
}

/// Builds the live process environment as a string-keyed associative array.
///
/// Also the contents of an eval-created `$_ENV`, and the base of `$_SERVER`. Each temporary key
/// and value is released once the array retains it.
pub(in crate::interpreter) fn eval_getenv_all_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let entries: Vec<_> = std::env::vars_os().collect();
    let mut result = super::super::collection_builder::EvalArrayBuilder::assoc(values, entries.len())?;
    for (key, value) in entries {
        result.entry(
            |values| values.string_bytes_value(value.as_bytes()),
            |values, _| values.string_bytes_value(key.as_bytes()),
        )?;
    }
    Ok(result.finish())
}
