//! Purpose:
//! Routes evaluated-argument filesystem registry hooks to focused value dispatchers.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks::EvalValuesHook::call()`.
//!
//! Key details:
//! - Values hooks run after named/default argument binding has produced PHP
//!   parameter order.

use super::super::super::super::*;

use super::path_values_dispatch::eval_filesystem_path_values_result;
use super::stream_values_dispatch::eval_filesystem_stream_values_result;

/// Dispatches evaluated-argument calls for declaratively migrated filesystem builtins.
pub(in crate::interpreter) fn eval_filesystem_values_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(result) =
        eval_filesystem_path_values_result(name, evaluated_args, context, values)?
    {
        return Ok(result);
    }
    if let Some(result) =
        eval_filesystem_stream_values_result(name, evaluated_args, context, values)?
    {
        return Ok(result);
    }
    Err(EvalStatus::RuntimeFatal)
}
