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

use super::*;
use std::ffi::c_void;

pub(in crate::interpreter) use closure_execution::*;
pub(in crate::interpreter) use function_binding::*;
pub(in crate::interpreter) use method_binding::*;
pub(in crate::interpreter) use native_execution::*;

/// Evaluates an eval-declared user function with PHP-style argument binding.
pub(in crate::interpreter) fn eval_dynamic_function(
    function: &EvalFunction,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args = eval_call_arg_values(args, context, caller_scope, values)?;
    eval_dynamic_function_with_evaluated_args(function, evaluated_args, context, values)
}

/// Evaluates and binds native AOT function arguments, filling registered defaults.
pub(in crate::interpreter) fn eval_native_function_call_args(
    function: &NativeFunction,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<BoundNativeFunctionArgs, EvalStatus> {
    let evaluated_args = eval_call_arg_values(args, context, caller_scope, values)?;
    bind_evaluated_native_function_args(function, evaluated_args, context, values)
}

/// Evaluates source-order call arguments while preserving named-argument metadata.
pub(in crate::interpreter) fn eval_call_arg_values(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    let mut evaluated_args = Vec::with_capacity(args.len());
    let mut saw_named = false;

    for arg in args {
        if arg.is_spread() {
            if saw_named {
                return Err(EvalStatus::RuntimeFatal);
            }
            let spread = eval_expr(arg.value(), context, caller_scope, values)?;
            if !values.is_array_like(spread)? {
                return Err(EvalStatus::RuntimeFatal);
            }
            append_unpacked_call_arg_values(
                spread,
                &mut evaluated_args,
                &mut saw_named,
                context,
                values,
            )?;
            continue;
        }

        if let Some(name) = arg.name() {
            saw_named = true;
            let (value, ref_target) =
                eval_call_arg_value(arg.value(), context, caller_scope, values)?;
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
        let (value, ref_target) = eval_call_arg_value(arg.value(), context, caller_scope, values)?;
        evaluated_args.push(EvaluatedCallArg {
            name: None,
            value,
            ref_target,
        });
    }

    Ok(evaluated_args)
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

/// Converts a `call_user_func_array` argument array into ordered call arguments.
pub(in crate::interpreter) fn eval_array_call_arg_values(
    arg_array: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    let len = values.array_len(arg_array)?;
    let mut evaluated_args = Vec::with_capacity(len);
    let mut saw_named = false;
    append_unpacked_call_arg_values(
        arg_array,
        &mut evaluated_args,
        &mut saw_named,
        context,
        values,
    )?;
    Ok(evaluated_args)
}

/// Appends one unpacked array's values using PHP named-argument key semantics.
pub(in crate::interpreter) fn append_unpacked_call_arg_values(
    array: RuntimeCellHandle,
    evaluated_args: &mut Vec<EvaluatedCallArg>,
    saw_named: &mut bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let len = values.array_len(array)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let ref_target = eval_array_reference_key(key, values)?
            .and_then(|key| context.array_element_alias(array, &key).cloned());
        let arg = match values.type_tag(key)? {
            EVAL_TAG_INT => {
                if *saw_named {
                    values.release(key)?;
                    return Err(EvalStatus::RuntimeFatal);
                }
                let value = match values.array_get(array, key) {
                    Ok(value) => value,
                    Err(status) => {
                        values.release(key)?;
                        return Err(status);
                    }
                };
                let (value, ref_target) =
                    eval_invoker_ref_arg_value_and_target(value, ref_target, values)?;
                EvaluatedCallArg {
                    name: None,
                    value,
                    ref_target,
                }
            }
            EVAL_TAG_STRING => {
                *saw_named = true;
                let name = values.string_bytes(key)?;
                let name = match String::from_utf8(name) {
                    Ok(name) => name,
                    Err(_) => {
                        values.release(key)?;
                        return Err(EvalStatus::RuntimeFatal);
                    }
                };
                let value = match values.array_get(array, key) {
                    Ok(value) => value,
                    Err(status) => {
                        values.release(key)?;
                        return Err(status);
                    }
                };
                let (value, ref_target) =
                    eval_invoker_ref_arg_value_and_target(value, ref_target, values)?;
                EvaluatedCallArg {
                    name: Some(name),
                    value,
                    ref_target,
                }
            }
            _ => {
                values.release(key)?;
                return Err(EvalStatus::RuntimeFatal);
            }
        };
        values.release(key)?;
        evaluated_args.push(arg);
    }
    Ok(())
}

/// Converts a descriptor-invoker ref marker into an eval-visible value and writeback target.
fn eval_invoker_ref_arg_value_and_target(
    value: RuntimeCellHandle,
    ref_target: Option<EvalReferenceTarget>,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<EvalReferenceTarget>), EvalStatus> {
    if values.type_tag(value)? != EVAL_TAG_INVOKER_REF_CELL {
        return Ok((value, ref_target));
    }
    let slot = values.raw_value_word(value)? as usize;
    let source_tag = values.raw_value_high_word(value)?;
    let value = eval_invoker_ref_slot_value(slot, source_tag, values)?;
    Ok((
        value,
        ref_target.or(Some(EvalReferenceTarget::InvokerSlot { slot, source_tag })),
    ))
}

/// Reads the current PHP value from a native descriptor-invoker by-reference slot.
fn eval_invoker_ref_slot_value(
    slot: usize,
    source_tag: u64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match source_tag {
        EVAL_TAG_INT | EVAL_TAG_FLOAT | EVAL_TAG_BOOL | EVAL_TAG_RESOURCE => {
            let word = unsafe { *(slot as *const u64) };
            values.raw_word_value(source_tag, word)
        }
        EVAL_TAG_STRING => {
            let words = unsafe { *(slot as *const [u64; 2]) };
            values.raw_string_value(words[0], words[1])
        }
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC | EVAL_TAG_OBJECT | EVAL_TAG_CALLABLE => {
            let word = unsafe { *(slot as *const u64) };
            values.raw_word_value(source_tag, word)
        }
        EVAL_TAG_MIXED => {
            let value = unsafe { *(slot as *const RuntimeCellHandle) };
            values.retain(value)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
