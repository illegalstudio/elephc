//! Purpose:
//! Eval registry entry and implementation for `phpversion`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - Reports the PHP LANGUAGE version, not the elephc compiler's package version, matching
//!   native `codegen::lower_inst::builtins::lower_phpversion`.
//! - `phpversion($extension)` answers over the same effective extension set eval's
//!   `extension_loaded` uses, including host-linked bridge extensions such as `tidy`,
//!   and returns `false` for anything else.

use super::*;

eval_builtin! {
    contract: "phpversion",
    area: NetworkEnv,
    direct: NetworkEnv,
    values: NetworkEnv,
}

/// Evaluates PHP `phpversion()` / `phpversion($extension)`.
pub(in crate::interpreter) fn eval_builtin_phpversion(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_phpversion_result(values),
        [extension] => {
            let extension = eval_expr(extension, context, scope, values)?;
            eval_phpversion_extension_result(extension, context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Returns the reported PHP version as a boxed PHP string.
pub(in crate::interpreter) fn eval_phpversion_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.string(crate::eval_php_profile::eval_php_version_string())
}

/// Returns the reported PHP version for a loaded extension, or `false` for anything else.
///
/// Reference PHP reports the interpreter's own version for every bundled extension (verified
/// on 8.5.6: `Core`, `json`, `pcre`, `Zend OPcache`, … all report `8.5.6`), and every
/// extension eval reports as loaded is a bundled-equivalent, so there is no per-extension
/// version to track. Membership is delegated to `eval_extension_is_loaded` so this and
/// `extension_loaded()` cannot disagree, and it is case-insensitive like reference PHP
/// (`phpversion('core')` and `phpversion('Core')` both answer).
pub(in crate::interpreter) fn eval_phpversion_extension_result(
    extension: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let name = values.string_bytes(extension)?;
    let name = String::from_utf8_lossy(&name);
    if eval_extension_is_loaded_in(name.as_ref(), context) {
        values.string(crate::eval_php_profile::eval_php_version_string())
    } else {
        values.bool_value(false)
    }
}
