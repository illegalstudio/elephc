//! Purpose:
//! Routes by-value dynamic builtin dispatch to focused builtin-family dispatchers.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry` re-exports.
//!
//! Key details:
//! - Each child dispatcher handles already evaluated runtime-cell arguments for one
//!   builtin family and returns `Ok(None)` when the name is outside its domain.

mod symbols;

use super::eval_declared_builtin_values_call;
use super::super::super::*;

use symbols::*;

/// Evaluates PHP-visible builtins when they are invoked through a dynamic callable name.
pub(in crate::interpreter) fn eval_builtin_with_values(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if let Some(result) = eval_declared_builtin_values_call(name, evaluated_args, context, values)? {
        return Ok(Some(result));
    }

    if let Some(result) =
        eval_raw_memory_builtin_with_values(name, evaluated_args, context, values)?
    {
        return Ok(Some(result));
    }
    if let Some(result) =
        eval_date_procedural_alias_with_values(name, evaluated_args, context, values)?
    {
        return Ok(Some(result));
    }
    if let Some(result) = eval_symbols_builtin_with_values(name, evaluated_args, context, values)? {
        return Ok(Some(result));
    }
    Ok(None)
}
