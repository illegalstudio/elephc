//! Purpose:
//! Area-level direct dispatch for source-sensitive array mutator builtins.
//!
//! Called from:
//! - `crate::interpreter::expressions::calls::eval_call()`.
//!
//! Key details:
//! - Dispatch stays orchestration-only; actual PHP-visible behavior lives in the
//!   builtin owner files or the closest shared owner builtin.

use super::super::super::*;

/// Routes direct source-sensitive array mutator calls through builtin owner files.
pub(in crate::interpreter) fn eval_builtin_array_mutating_declared_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "settype" => eval_builtin_settype_call(args, context, scope, values),
        "array_pop" | "array_shift" => {
            super::array_pop::eval_array_pop_shift_declared_call(name, args, context, scope, values)
        }
        "array_push" | "array_unshift" => {
            super::array_push::eval_array_push_unshift_declared_call(name, args, context, scope, values)
        }
        "array_splice" => super::array_splice::eval_builtin_array_splice_call(args, context, scope, values),
        "array_walk" => super::array_walk::eval_builtin_array_walk_call(args, context, scope, values),
        "arsort" | "asort" | "krsort" | "ksort" | "natcasesort" | "natsort" | "rsort"
        | "shuffle" | "sort" => {
            super::sort::eval_array_sort_declared_call(name, args, context, scope, values)
        }
        "uasort" | "uksort" | "usort" => {
            super::usort::eval_user_sort_declared_call(name, args, context, scope, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
