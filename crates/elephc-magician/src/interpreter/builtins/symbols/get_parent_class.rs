//! Purpose:
//! Eval registry entry and implementation for `get_parent_class`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Eval-created object and class-string parents are resolved before runtime fallback.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "get_parent_class",
    area: Symbols,
    params: [object_or_class = EvalBuiltinDefaultValue::Null],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Evaluates direct `get_parent_class(...)` calls.
pub(in crate::interpreter) fn eval_get_parent_class_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_get_parent_class(args, context, scope, values)
}

/// Evaluates materialized `get_parent_class(...)` arguments.
pub(in crate::interpreter) fn eval_get_parent_class_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [] => eval_get_parent_class_no_arg_result(context, values),
        [object_or_class] => eval_get_parent_class_result(*object_or_class, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP's `get_parent_class(...)` over one eval object or class-name expression.
pub(in crate::interpreter) fn eval_builtin_get_parent_class(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_get_parent_class_no_arg_result(context, values),
        [object_or_class] => {
            let object_or_class = eval_expr(object_or_class, context, scope, values)?;
            eval_get_parent_class_result(object_or_class, context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Resolves PHP's deprecated no-arg `get_parent_class()` form from the current class scope.
pub(in crate::interpreter) fn eval_get_parent_class_no_arg_result(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(class_name) = context.current_class_scope() else {
        return values.string("");
    };
    let class_name = values.string(class_name.trim_start_matches('\\'))?;
    eval_get_parent_class_result(class_name, context, values)
}

/// Resolves the PHP-visible parent class name for one object or class-name cell.
pub(in crate::interpreter) fn eval_get_parent_class_result(
    object_or_class: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Ok(identity) = values.object_identity(object_or_class) {
        if let Some(class_name) = context.dynamic_object_class_name(identity) {
            if let Some(parent) = context.class_parent_names(&class_name).into_iter().next() {
                return values.string(&parent);
            }
            return values.string("");
        }
    }
    if values.type_tag(object_or_class)? == EVAL_TAG_STRING {
        let name = values.string_bytes(object_or_class)?;
        let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
        if context.class(&name).is_some() {
            if let Some(parent) = context.class_parent_names(&name).into_iter().next() {
                return values.string(&parent);
            }
            return values.string("");
        }
    }
    values.parent_class_name(object_or_class)
}
