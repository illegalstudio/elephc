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

use std::ffi::{OsStr, OsString};

/// Converts a host environment string to the byte representation exposed by PHP.
#[cfg(unix)]
fn os_str_bytes(value: &OsStr) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;

    value.as_bytes().to_vec()
}

/// Converts a Windows UTF-16 environment string to PHP's UTF-8 byte representation.
#[cfg(not(unix))]
fn os_str_bytes(value: &OsStr) -> Vec<u8> {
    value.to_string_lossy().into_owned().into_bytes()
}

/// Looks up an environment name without changing its PHP byte spelling.
#[cfg(unix)]
fn var_os_bytes(name: &[u8]) -> Option<OsString> {
    use std::os::unix::ffi::OsStrExt;

    std::env::var_os(OsStr::from_bytes(name))
}

/// Windows exposes environment names as UTF-16, so invalid UTF-8 input follows
/// the same replacement conversion as the platform's string surface.
#[cfg(not(unix))]
fn var_os_bytes(name: &[u8]) -> Option<OsString> {
    let name = String::from_utf8_lossy(name);
    std::env::var_os(OsStr::new(name.as_ref()))
}

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
    match var_os_bytes(&name) {
        Some(value) => values.string_bytes_value(&os_str_bytes(&value)),
        None => values.bool_value(false),
    }
}

/// Builds the live process environment as a string-keyed associative array.
pub(in crate::interpreter) fn eval_getenv_all_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let entries: Vec<_> = std::env::vars_os().collect();
    let mut result = values.assoc_new(entries.len())?;
    for (key, value) in entries {
        let key = values.string_bytes_value(&os_str_bytes(&key))?;
        let value = values.string_bytes_value(&os_str_bytes(&value))?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}
