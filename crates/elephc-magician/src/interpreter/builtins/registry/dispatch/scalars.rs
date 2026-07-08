//! Purpose:
//! Dispatches remaining already evaluated scalar mutation builtins by dynamic callable name.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry::dispatch`.
//!
//! Key details:
//! - Returns `Ok(None)` for names outside this domain so the parent dispatcher can
//!   continue probing other builtin families.

use super::super::super::super::*;
use super::super::super::*;

/// Attempts to dispatch evaluated scalar mutation builtins.
pub(in crate::interpreter) fn eval_scalars_builtin_with_values(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let result = match name {
        "settype" => {
            let [value, type_name] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_settype_value_result(*value, *type_name, values)?
        }
        _ => return Ok(None),
    };
    Ok(Some(result))
}
