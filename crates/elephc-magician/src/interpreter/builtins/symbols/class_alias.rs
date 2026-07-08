//! Purpose:
//! Declarative eval registry entry for `class_alias`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the symbol dispatch adapter.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "class_alias",
    area: Symbols,
    params: [r#class, alias, autoload = EvalBuiltinDefaultValue::Bool(true)],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `class_alias` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_class_alias_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::dispatch::eval_builtin_symbols_call_impl("class_alias", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `class_alias` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_class_alias_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::dispatch::eval_symbols_values_result_impl("class_alias", evaluated_args, context, values)
}
