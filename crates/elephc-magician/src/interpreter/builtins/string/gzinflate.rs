//! Purpose:
//! Declarative eval registry entry for `gzinflate`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the gzip/zlib hook.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "gzinflate",
    area: String,
    params: [data, max_length = EvalBuiltinDefaultValue::Int(0)],
    direct: Gzip,
    values: Gzip,
}

use super::super::super::*;

/// Evaluates PHP `gzinflate(...)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_gzinflate(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::gzcompress::eval_builtin_gzip_named("gzinflate", args, context, scope, values)
}

/// Applies PHP `gzinflate(...)` to already evaluated arguments.
pub(in crate::interpreter) fn eval_gzinflate_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::gzcompress::eval_gzip_named_result("gzinflate", evaluated_args, values)
}
