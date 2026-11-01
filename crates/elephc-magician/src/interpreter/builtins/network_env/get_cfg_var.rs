//! Purpose:
//! Eval registry entry and implementation for `get_cfg_var`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - `get_cfg_var()` reads a value set in `php.ini` ONLY, which the interpreter has no more
//!   of than a compiled program does: even in reference PHP an option set at run time with
//!   `ini_set()` stays invisible to it. Every option is therefore "not set", and PHP's
//!   answer for a not-set option is `false`.
//! - `ini_get()` asks the different question -- the ACTIVE value -- and is answered
//!   elsewhere. The two appear together in the wild
//!   (`\ini_get('xdebug.file_link_format') ?: get_cfg_var('xdebug.file_link_format')` in
//!   Symfony's `HtmlErrorRenderer`), so answering only one of them left interpreted code
//!   with a `Call to undefined function get_cfg_var()`.

use super::*;

eval_builtin! {
    contract: "get_cfg_var",
    area: NetworkEnv,
    direct: NetworkEnv,
    values: NetworkEnv,
}

/// Evaluates PHP `get_cfg_var($option)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_get_cfg_var(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [option] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let option = eval_expr(option, context, scope, values)?;
    let result = eval_get_cfg_var_result(option, values);
    values.release(option)?;
    result
}

/// Returns `false` for every option, because no `php.ini` is ever read.
pub(in crate::interpreter) fn eval_get_cfg_var_result(
    option: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    // The option name is still read so a non-string argument raises PHP's own conversion
    // diagnostic rather than being silently accepted.
    values.string_bytes(option)?;
    values.bool_value(false)
}
