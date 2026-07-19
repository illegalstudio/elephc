//! Purpose:
//! Eval registry entry and implementation for `get_class`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Eval-created object class names are resolved from eval context before
//!   falling back to runtime object metadata.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "get_class",
    area: Symbols,
    params: [object = EvalBuiltinDefaultValue::Null],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Evaluates direct `get_class(...)` calls.
pub(in crate::interpreter) fn eval_get_class_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_get_class(args, context, scope, values)
}

/// Evaluates materialized `get_class(...)` arguments.
pub(in crate::interpreter) fn eval_get_class_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [] => eval_get_class_no_arg_result(context, values),
        [object] => eval_get_class_result(*object, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP's `get_class(...)` over one eval object expression.
pub(in crate::interpreter) fn eval_builtin_get_class(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_get_class_no_arg_result(context, values),
        [object] => {
            let object = eval_expr(object, context, scope, values)?;
            eval_get_class_result(object, context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Resolves PHP's deprecated no-arg `get_class()` form from the current class scope.
pub(in crate::interpreter) fn eval_get_class_no_arg_result(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(class_name) = context.current_class_scope() else {
        return eval_throw_error(
            "get_class() without arguments must be called from within a class",
            context,
            values,
        );
    };
    values.string(class_name.trim_start_matches('\\'))
}

/// Resolves the PHP-visible class name for one already materialized object cell.
pub(in crate::interpreter) fn eval_get_class_result(
    object: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Ok(identity) = values.object_identity(object) {
        if let Some(class_name) = context.dynamic_object_class_name(identity) {
            return values.string(&class_name);
        }
    }
    values.object_class_name(object)
}
