//! Purpose:
//! Dispatches already evaluated core callable, constant, and debug-output builtins by dynamic callable name.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry::dispatch`.
//!
//! Key details:
//! - Returns `Ok(None)` for names outside this domain so the parent dispatcher can
//!   continue probing other builtin families.

use super::super::super::super::*;
use super::super::super::*;
use super::super::*;

/// Attempts to dispatch evaluated core callable, constant, and debug-output builtins.
pub(in crate::interpreter) fn eval_core_builtin_with_values(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let result = match name {
        "print_r" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_print_r_result(*value, values)?
        }
        "var_dump" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_var_dump_result(*value, values)?
        }
        "call_user_func" => {
            return eval_call_user_func_with_values(evaluated_args.to_vec(), context, values)
                .map(Some);
        }
        "call_user_func_array" => {
            let [callback, arg_array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            return eval_call_user_func_array_with_values(*callback, *arg_array, context, values)
                .map(Some);
        }
        "define" => eval_define_result(evaluated_args, context, values)?,
        "defined" => eval_defined_result(evaluated_args, context, values)?,
        _ => return Ok(None),
    };
    Ok(Some(result))
}
