//! Purpose:
//! Coordinates call-argument evaluation for user-declared and native functions.
//! Binding and execution paths live in focused child modules.
//!
//! Called from:
//! - `crate::interpreter::eval_call()` and dynamic callable dispatch helpers.
//!
//! Key details:
//! - PHP source evaluation order is preserved before argument binding.
//! - Static locals are persisted through `ElephcEvalContext` after function execution.

mod closure_execution;
mod function_binding;
mod method_binding;
mod native_execution;
mod native_staging;
pub(in crate::interpreter) mod builtin_arguments;

use super::*;
use std::ffi::c_void;

pub(in crate::interpreter) use closure_execution::*;
pub(in crate::interpreter) use function_binding::*;
pub(in crate::interpreter) use method_binding::*;
pub(in crate::interpreter) use native_execution::*;
use native_staging::stage_native_function_invoker_args;

/// Evaluates an eval-declared user function with PHP-style argument binding.
pub(in crate::interpreter) fn eval_dynamic_function(
    function: &EvalFunction,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_eval_call_arguments(
        args,
        context,
        caller_scope,
        values,
        |arguments, context, _, values| {
            eval_dynamic_function_with_evaluated_args(function, arguments, context, values)
        },
    )
}

/// Evaluates source-order call arguments while preserving named-argument metadata.
pub(in crate::interpreter) fn eval_call_arg_values(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    eval_call_arg_values_observed(args, context, caller_scope, values, |_, _| {})
}

/// Reports each directly evaluated argument before binding so callers can manage known temporary owners.
pub(in crate::interpreter) fn eval_call_arg_values_observed(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
    mut observe: impl FnMut(&EvalExpr, RuntimeCellHandle),
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    let mut evaluated = Vec::with_capacity(args.len());
    evaluate_call_arguments(args, context, caller_scope, values, &mut observe, None, &mut evaluated, None)?;
    Ok(evaluated)
}

/// Captures owned builtin inputs, using shared reference modes when a contract is supplied.
/// Without a contract, call_user_func captures independent values even for reference parameters.
pub(in crate::interpreter) fn eval_owned_call_arg_values(
    args: &[EvalCallArg], context: &mut ElephcEvalContext, caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps, owners: &mut Vec<RuntimeCellHandle>, evaluated: &mut Vec<EvaluatedCallArg>,
    contract: Option<&elephc_builtin_contract::BuiltinContract>,
) -> Result<(), EvalStatus> {
    evaluate_call_arguments(args, context, caller_scope, values, &mut |_, _| {}, Some(owners), evaluated, contract)
}

/// Acquires normal-call argument owners without discarding caller reference targets.
pub(in crate::interpreter) fn eval_leased_call_arg_values(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
    leases: &mut Vec<EvalValueLease>,
    evaluated_args: &mut Vec<EvaluatedCallArg>,
) -> Result<(), EvalStatus> {
    let mut saw_named = false;

    for arg in args {
        if arg.is_spread() {
            if saw_named {
                return Err(EvalStatus::RuntimeFatal);
            }
            let spread = acquire_expr_lease(arg.value(), context, caller_scope, values)?;
            let spread_value = spread.owner;
            leases.push(spread);
            if !values.is_array_like(spread_value)? {
                return Err(EvalStatus::RuntimeFatal);
            }
            let mut unpacked_owners = Vec::new();
            let unpacked = append_unpacked_call_arg_values_with_owners(
                spread_value,
                evaluated_args,
                &mut saw_named,
                context,
                values,
                Some(&mut unpacked_owners),
            );
            leases.extend(
                unpacked_owners
                    .into_iter()
                    .map(EvalValueLease::preserving_metadata),
            );
            unpacked?;
            continue;
        }

        if arg.name().is_none() && saw_named {
            return Err(EvalStatus::RuntimeFatal);
        }
        let name = arg.name().map(str::to_string);
        saw_named |= name.is_some();
        let (value, ref_target) =
            eval_call_arg_value(arg.value(), context, caller_scope, values)?;
        let lease = acquire_value_lease(value, values)?;
        let value = lease.owner;
        leases.push(lease);
        evaluated_args.push(EvaluatedCallArg {
            name,
            value,
            ref_target,
        });
    }

    Ok(())
}

/// Evaluates source arguments with legacy targets or explicit owners selected by the parameter contract.
fn evaluate_call_arguments(
    args: &[EvalCallArg], context: &mut ElephcEvalContext, caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps, observe: &mut impl FnMut(&EvalExpr, RuntimeCellHandle),
    mut owners: Option<&mut Vec<RuntimeCellHandle>>, evaluated_args: &mut Vec<EvaluatedCallArg>,
    contract: Option<&elephc_builtin_contract::BuiltinContract>,
) -> Result<(), EvalStatus> {
    let mut saw_named = false;

    for arg in args {
        if arg.is_spread() {
            if saw_named {
                return Err(EvalStatus::RuntimeFatal);
            }
            let spread = if let Some(owners) = owners.as_deref_mut() {
                let spread = eval_owned_expr(arg.value(), context, caller_scope, values)?;
                owners.push(spread);
                spread
            } else { eval_expr(arg.value(), context, caller_scope, values)? };
            observe(arg.value(), spread);
            if !values.is_array_like(spread)? {
                return Err(EvalStatus::RuntimeFatal);
            }
            if let Some(owners) = owners.as_deref_mut() {
                if let Some(contract) = contract.filter(|contract| contract.params.iter().any(|param| param.by_ref)) {
                    builtin_arguments::append_spread(contract, spread, evaluated_args, &mut saw_named, context, values, owners)?;
                } else {
                    append_unpacked_value_call_args(spread, evaluated_args, &mut saw_named, context, values, owners)?;
                }
                let index = owners.iter().position(|value| *value == spread).expect("captured spread owner");
                owners.remove(index);
                context.clear_array_metadata(spread);
                eval_release_value(context, values, spread)?;
            } else {
                append_unpacked_call_arg_values(spread, evaluated_args, &mut saw_named, context, values)?;
            }
            continue;
        }

        if let Some(name) = arg.name() {
            saw_named = true;
            let (value, ref_target) =
                evaluate_call_argument_value(arg.value(), context, caller_scope, values, owners.as_deref_mut(),
                    builtin_arguments::by_reference(contract, Some(name), evaluated_args.len()))?;
            observe(arg.value(), value);
            evaluated_args.push(EvaluatedCallArg {
                name: Some(name.to_string()),
                value,
                ref_target,
            });
            continue;
        }

        if saw_named {
            return Err(EvalStatus::RuntimeFatal);
        }
        let (value, ref_target) = evaluate_call_argument_value(arg.value(), context, caller_scope, values, owners.as_deref_mut(),
            builtin_arguments::by_reference(contract, None, evaluated_args.len()))?;
        observe(arg.value(), value);
        evaluated_args.push(EvaluatedCallArg {
            name: None,
            value,
            ref_target,
        });
    }

    Ok(())
}

/// Captures an independent value or persistent reference owner, retaining legacy binding for unowned calls.
fn evaluate_call_argument_value(
    expr: &EvalExpr, context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps, owners: Option<&mut Vec<RuntimeCellHandle>>,
    by_reference: bool,
) -> Result<(RuntimeCellHandle, Option<EvalReferenceTarget>), EvalStatus> {
    if let Some(owners) = owners {
        if by_reference {
            let reference = builtin_arguments::reference(expr, context, scope, values)?;
            owners.push(reference);
            return Ok((reference, Some(EvalReferenceTarget::Cell { cell: reference })));
        }
        let value = eval_owned_expr(expr, context, scope, values)?;
        owners.push(value);
        Ok((value, None))
    } else { eval_call_arg_value(expr, context, scope, values) }
}

/// Evaluates one call arg and captures caller-side storage for by-reference parameters.
pub(in crate::interpreter) fn eval_call_arg_value(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<EvalReferenceTarget>), EvalStatus> {
    match expr {
        EvalExpr::LoadVar(name) => {
            let value = visible_scope_cell(context, caller_scope, name)
                .map_or_else(|| values.null(), Ok)?;
            Ok((
                value,
                Some(EvalReferenceTarget::Variable {
                    scope: caller_scope as *mut ElephcEvalScope,
                    name: name.clone(),
                }),
            ))
        }
        EvalExpr::ArrayGet { array, index } => {
            let EvalExpr::LoadVar(array_name) = array.as_ref() else {
                return eval_nested_array_element_call_arg_value(
                    array,
                    index,
                    context,
                    caller_scope,
                    values,
                );
            };
            let array = visible_scope_cell(context, caller_scope, array_name)
                .map_or_else(|| values.null(), Ok)?;
            let index = eval_expr(index, context, caller_scope, values)?;
            let value = eval_array_get_result(array, index, context, values)?;
            if values.type_tag(array)? == EVAL_TAG_OBJECT {
                return Ok((value, None));
            }
            Ok((
                value,
                Some(EvalReferenceTarget::ArrayElement {
                    scope: caller_scope as *mut ElephcEvalScope,
                    array_name: array_name.clone(),
                    index,
                }),
            ))
        }
        EvalExpr::PropertyGet { object, property } => {
            let access_scope = context.execution_scope();
            let object = eval_expr(object, context, caller_scope, values)?;
            let value = eval_property_get_result(object, property, context, values)?;
            validate_property_ref_target(object, property, context, values)?;
            Ok((
                value,
                Some(EvalReferenceTarget::ObjectProperty {
                    object,
                    property: property.clone(),
                    access_scope,
                }),
            ))
        }
        EvalExpr::DynamicPropertyGet { object, property } => {
            let access_scope = context.execution_scope();
            let object = eval_expr(object, context, caller_scope, values)?;
            let property = eval_dynamic_member_name(property, context, caller_scope, values)?;
            let value = eval_property_get_result(object, &property, context, values)?;
            validate_property_ref_target(object, &property, context, values)?;
            Ok((
                value,
                Some(EvalReferenceTarget::ObjectProperty {
                    object,
                    property,
                    access_scope,
                }),
            ))
        }
        EvalExpr::StaticPropertyGet {
            class_name,
            property,
        } => {
            let access_scope = context.execution_scope();
            let class_name = resolve_eval_static_member_class_name(class_name, context)?;
            eval_static_property_call_arg_value(
                class_name,
                property.clone(),
                access_scope,
                context,
                values,
            )
        }
        EvalExpr::DynamicStaticPropertyGet {
            class_name,
            property,
        } => {
            let access_scope = context.execution_scope();
            let class_name = eval_expr(class_name, context, caller_scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            eval_static_property_call_arg_value(
                class_name,
                property.clone(),
                access_scope,
                context,
                values,
            )
        }
        EvalExpr::DynamicStaticPropertyNameGet {
            class_name,
            property,
        } => {
            let access_scope = context.execution_scope();
            let class_name = eval_expr(class_name, context, caller_scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            let property = eval_dynamic_member_name(property, context, caller_scope, values)?;
            eval_static_property_call_arg_value(
                class_name,
                property,
                access_scope,
                context,
                values,
            )
        }
        _ => eval_expr(expr, context, caller_scope, values).map(|value| (value, None)),
    }
}

/// Evaluates an array element whose array expression is itself a writable caller target.
fn eval_nested_array_element_call_arg_value(
    array: &EvalExpr,
    index: &EvalExpr,
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<EvalReferenceTarget>), EvalStatus> {
    let (array, array_target) = eval_call_arg_value(array, context, caller_scope, values)?;
    let index = eval_expr(index, context, caller_scope, values)?;
    let value = eval_array_get_result(array, index, context, values)?;
    if values.type_tag(array)? == EVAL_TAG_OBJECT {
        return Ok((value, None));
    }
    let Some(array_target) = array_target else {
        return Ok((value, None));
    };
    Ok((
        value,
        Some(EvalReferenceTarget::NestedArrayElement {
            array_target: Box::new(array_target),
            index,
        }),
    ))
}

/// Evaluates one static-property lvalue and records it as a by-reference call target.
fn eval_static_property_call_arg_value(
    class_name: String,
    property: String,
    access_scope: ElephcEvalExecutionScope,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<EvalReferenceTarget>), EvalStatus> {
    let value = eval_static_property_get_result(&class_name, &property, context, values)?;
    Ok((
        value,
        Some(EvalReferenceTarget::StaticProperty {
            class_name,
            property,
            access_scope,
        }),
    ))
}
