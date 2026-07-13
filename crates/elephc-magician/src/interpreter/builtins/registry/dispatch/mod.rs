//! Purpose:
//! Routes by-value dynamic builtin dispatch through declarative registry lookup
//! and eval-only runtime alias fallbacks.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry` re-exports.
//!
//! Key details:
//! - Migrated builtins dispatch through `eval_declared_builtin_values_call`.
//! - Procedural date/time aliases remain a runtime fallback because eval cannot
//!   run the static name-resolver rewrite before dispatch.

use super::eval_declared_builtin_values_call;
use super::super::super::*;

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
        eval_date_procedural_alias_with_values(name, evaluated_args, context, values)?
    {
        return Ok(Some(result));
    }
    Ok(None)
}
