//! Purpose:
//! Eval registry entry and implementation for `setlocale`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env` direct and by-value dispatch.
//!
//! Key details:
//! - Follows the compiled runtime's deterministic C-locale model: nothing changes process-wide.
//!   A query (`"0"` or `""`) answers `"C"`, as php does in a C-locale process, and any other
//!   request answers the locale it named (the first entry of an array). Twig's `Compiler::repr()`
//!   brackets every float literal with a query and a restore, and died on the missing function.

use super::*;

eval_builtin! {
    contract: "setlocale",
    area: NetworkEnv,
    direct: Setlocale,
    values: Setlocale,
}

/// Evaluates PHP `setlocale($category, $locales, ...$rest)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_setlocale(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_setlocale_values(&evaluated, values)
}

/// Evaluates PHP `setlocale(...)` from already evaluated argument cells.
pub(in crate::interpreter) fn eval_setlocale_values(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(locale) = evaluated_args.get(1).copied() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let locale = if values.is_array_like(locale)? {
        let first = values.int(0)?;
        values.array_get(locale, first)?
    } else {
        locale
    };
    let requested = values.string_bytes(locale)?;
    if requested.is_empty() || requested == b"0" {
        return values.string_bytes_value(b"C");
    }
    values.string_bytes_value(&requested)
}
