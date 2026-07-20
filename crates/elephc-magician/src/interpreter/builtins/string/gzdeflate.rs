//! Purpose:
//! Declarative eval registry entry for `gzdeflate`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the gzip/zlib hook.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "gzdeflate",
    area: String,
    params: [data, level = EvalBuiltinDefaultValue::Int(-1)],
    direct: Gzip,
    values: Gzip,
}

use super::super::super::*;

/// Evaluates PHP `gzdeflate(...)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_gzdeflate(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::gzcompress::eval_builtin_gzip_named("gzdeflate", args, context, scope, values)
}

/// Applies PHP `gzdeflate(...)` to already evaluated arguments.
pub(in crate::interpreter) fn eval_gzdeflate_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::gzcompress::eval_gzip_named_result("gzdeflate", evaluated_args, values)
}
