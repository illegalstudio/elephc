//! Purpose:
//! Eval registry entry and implementation for PHP 8.5's `clone()` function.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols` for direct and materialized calls.
//!
//! Key details:
//! - The existing clone engine owns hook dispatch and shallow-copy semantics.
//! - Property overrides are applied after `__clone()` and preserve the caller's class scope.

eval_builtin! {
    contract: "clone",
    area: Symbols,
    source_arguments: true,
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Dispatches direct eval calls while preserving source evaluation order and cleanup.
pub(in crate::interpreter) fn eval_clone_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(1..=2).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let operands = args.iter().collect::<Vec<_>>();
    with_eval_operands(&operands, context, scope, values, |args, context, scope, values| {
        eval_clone_declared_values_result_with_scope(args, Some(scope), context, values)
    })
}

/// Clones one materialized object and optionally initializes selected properties.
pub(in crate::interpreter) fn eval_clone_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_clone_declared_values_result_with_scope(evaluated_args, None, context, values)
}

/// Keeps the direct caller's visible reference aliases available to clone validation.
fn eval_clone_declared_values_result_with_scope(
    evaluated_args: &[RuntimeCellHandle],
    scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [object, rest @ ..] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if rest.len() > 1 {
        return Err(EvalStatus::RuntimeFatal);
    }

    let object_tag = values.type_tag(*object)?;
    if object_tag != EVAL_TAG_OBJECT {
        return eval_throw_type_error(
            &format!(
                "clone(): Argument #1 ($object) must be of type object, {} given",
                eval_gettype_name(object_tag)
            ),
            context,
            values,
        );
    }

    let with_properties = rest.first().copied();
    if let Some(properties) = with_properties {
        let properties_tag = values.type_tag(properties)?;
        if !matches!(properties_tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
            return eval_throw_type_error(
                &format!(
                    "clone(): Argument #2 ($withProperties) must be of type array, {} given",
                    eval_gettype_name(properties_tag)
                ),
                context,
                values,
            );
        }
    }

    eval_object_clone_with_properties_result(*object, with_properties, scope, context, values)
}
