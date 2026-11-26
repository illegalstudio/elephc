//! Purpose:
//! Shared evaluated-expression helpers for operators, class names, closures, and match.
//!
//! Called from:
//! - `crate::interpreter::expressions::eval_expr()`.
//!
//! Key details:
//! - Helpers operate after the parent evaluator has selected the expression shape
//!   and preserve PHP runtime conversion, class-alias, and closure-capture rules.

use super::*;
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};
use elephc_memoir::runtime_owners::{CleanupErrorPolicy, ReleaseOrder, RuntimeOwnerBatch};

/// Applies one already-evaluated binary operation with eval runtime semantics.
pub(in crate::interpreter) fn eval_binary_result(
    op: EvalBinOp,
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match op {
        EvalBinOp::Add => values.add(left, right),
        EvalBinOp::Sub => values.sub(left, right),
        EvalBinOp::Mul => values.mul(left, right),
        EvalBinOp::Div => values.div(left, right),
        EvalBinOp::Mod => values.modulo(left, right),
        EvalBinOp::Pow => values.pow(left, right),
        EvalBinOp::BitAnd
        | EvalBinOp::BitOr
        | EvalBinOp::BitXor
        | EvalBinOp::ShiftLeft
        | EvalBinOp::ShiftRight => values.bitwise_dispatch(op, left, right),
        EvalBinOp::Concat => eval_concat_result(left, right, context, values),
        EvalBinOp::LogicalXor => {
            let left_truthy = values.truthy(left)?;
            let right_truthy = values.truthy(right)?;
            values.bool_value(left_truthy ^ right_truthy)
        }
        EvalBinOp::LooseEq
        | EvalBinOp::LooseNotEq
        | EvalBinOp::StrictEq
        | EvalBinOp::StrictNotEq
        | EvalBinOp::Lt
        | EvalBinOp::LtEq
        | EvalBinOp::Gt
        | EvalBinOp::GtEq => values.compare(op, left, right),
        EvalBinOp::Spaceship => values.spaceship(left, right),
        EvalBinOp::LogicalAnd | EvalBinOp::LogicalOr => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Concatenates borrowed operands and releases any separate cells returned by object string hooks.
fn eval_concat_result(
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    use crate::runtime_hooks::owner_frames::StagedRuntimeOwners;

    let owners = StagedRuntimeOwners::default();
    let result = RuntimeOwnerSlot::empty();
    with_runtime_owners(&(&owners, &result), || {
        let operation = (|| {
            let left_string = eval_string_context_value(left, context, values)?;
            if left_string != left {
                owners.stage(left_string);
            }
            let right_string = eval_string_context_value(right, context, values)?;
            if right_string != right {
                owners.stage(right_string);
            }
            result.replace(Some(values.concat(left_string, right_string)?));
            Ok(())
        })();
        owners.finish(operation, &result, |cell| release_expr_result(cell, context, values))
    })
}

/// Evaluates a runtime property or method name expression and returns its PHP string bytes as UTF-8.
pub(in crate::interpreter) fn eval_dynamic_member_name(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let source = RuntimeOwnerSlot::new(eval_owned_expr(expr, context, scope, values)?);
    let converted = RuntimeOwnerSlot::empty();
    with_runtime_owners(&(&source, &converted), || {
        let result = (|| {
            let value = eval_string_context_value(source.borrowed(), context, values)?;
            if value != source.borrowed() {
                converted.replace(Some(value));
            }
            let bytes = values.string_bytes(value)?;
            String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)
        })();
        let converted_released = converted
            .take_optional()
            .map_or(Ok(()), |value| release_expr_result(value, context, values));
        let source_released = eval_release_value(context, values, source.take());
        result.and_then(|result| converted_released.and(source_released).map(|()| result))
    })
}

/// Reads an array element or dispatches `ArrayAccess::offsetGet()` for objects.
pub(in crate::interpreter) fn eval_array_get_result(
    array: RuntimeCellHandle,
    index: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.type_tag(array)? != EVAL_TAG_OBJECT {
        if let Some(target) = eval_array_reference_key(index, values)?
            .and_then(|key| context.array_element_alias(array, &key).cloned())
        {
            return eval_reference_target_value(&target, context, values);
        }
        return values.array_get(array, index);
    }
    if !eval_array_access_object_matches(array, context, values)? {
        let class_name = eval_closure_bound_object_class_name(array, context, values)?;
        return eval_throw_error(&format!("Cannot use object of type {class_name} as array"), context, values);
    }
    if let Some(value) = super::super::runtime_ops::simplexml_properties::dimension(
        values, array, elephc_builtin_contract::ObjectDimension::Get, vec![index],
    )? { return Ok(value); }
    eval_method_call_result(array, "offsetGet", vec![index], context, values)
}

/// Returns whether an object value satisfies PHP's `ArrayAccess` interface.
pub(in crate::interpreter) fn eval_array_access_object_matches(
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if dynamic_object_is_a(value, "SimpleXMLElement", false, context, values)?
        .map_or_else(|| values.object_is_a(value, "SimpleXMLElement", false), Ok)?
    { return Ok(true); }
    dynamic_object_is_a(value, "ArrayAccess", false, context, values)?
        .map_or_else(|| values.object_is_a(value, "ArrayAccess", false), Ok)
}

/// Evaluates one PHP scalar cast expression through the runtime conversion hooks.
pub(super) fn eval_cast_expr(
    target: &EvalCastType,
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_eval_operands(&[expr], context, scope, values, |args, context, _, values| {
        eval_cast_value(target, args[0], context, values)
    })
}

/// Casts an evaluated operand through the same object adapters for both EvalIR consumers.
pub(in crate::interpreter) fn eval_cast_value(
    target: &EvalCastType,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match target {
        EvalCastType::Array => {
            if let Some(result) = super::super::runtime_ops::object_operators::numeric_cast(values, value, elephc_builtin_contract::ObjectCast::Array)? { return Ok(result); }
            values.cast_array(value)
        }
        EvalCastType::Int => {
            if let Some(result) = super::super::runtime_ops::object_operators::numeric_cast(values, value, elephc_builtin_contract::ObjectCast::Int)? { return Ok(result); }
            match eval_object_scalar_adapter(value, "__elephc_to_int", context, values)? {
                Some(value) => Ok(value),
                None => values.cast_int(value),
            }
        }
        EvalCastType::Float => {
            if let Some(result) = super::super::runtime_ops::object_operators::numeric_cast(values, value, elephc_builtin_contract::ObjectCast::Float)? { return Ok(result); }
            match eval_object_scalar_adapter(value, "__elephc_to_float", context, values)? {
                Some(value) => Ok(value),
                None => values.cast_float(value),
            }
        }
        EvalCastType::String => {
            let converted = eval_string_context_value(value, context, values)?;
            let result = values.cast_string(converted);
            if converted != value {
                finish_synthesized_method_arguments(result, [converted], context, values)
            } else { result }
        }
        EvalCastType::Bool => match eval_object_scalar_adapter(
            value,
            "__elephc_to_bool",
            context,
            values,
        )? {
            Some(value) => Ok(value),
            None => values.cast_bool(value),
        },
    }
}

/// Constructs an object after the target class name and constructor arguments have been evaluated.
pub(super) fn eval_new_object_result(
    class_name: &str,
    args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let concrete = ensure_generic_class(class_name, Some(&args), "__construct", context, scope, values)?;
    let class_name = concrete.as_str();
    if let Some(object) =
        eval_reflection_owner_new_object(class_name, args.clone(), context, values)?
    {
        return Ok(object);
    }
    if let Some(class) = context.class(class_name).cloned() {
        return eval_dynamic_class_new_object(&class, args, context, scope, values);
    }
    // `CURLFile`/`CURLStringFile` DELIBERATELY FALL THROUGH to the native-class fallback
    // below, and that is the whole point rather than an oversight. They used to be
    // intercepted here alongside the curl multi/share interfaces, on a "TWO DISTINCT OBJECT
    // SPACES" argument that does not apply to them: unlike every other curl class, they are
    // PURE PHP DATA CLASSES wrapping no native handle at all
    // (`crate::curl_prelude`'s own comment above their declarations), so the real AOT class
    // an `elephc_curl`-linked program already carries works correctly inside `eval()` — its
    // properties read, its getters and setters dispatch, and
    // `crate::interpreter::builtins::curl::multipart` consumes exactly such an object when
    // it walks a `CURLOPT_POSTFIELDS` array. The interception existed only because that
    // walk did not, which made a constructible `CURLFile` a value nothing could use.
    let object = RuntimeOwnerSlot::new(values.new_object(class_name)?);
    let argument_owners = RuntimeOwnerBatch::new(args.iter().map(|arg| arg.value));
    with_runtime_owners(&(&object, &argument_owners), || {
        let constructed = eval_native_constructor_with_evaluated_args(
            class_name, object.borrowed(), args.clone(), context, values,
        );
        argument_owners.finish(
            constructed,
            &object,
            ReleaseOrder::Forward,
            CleanupErrorPolicy::First,
            |arg| release_expr_result(arg, context, values),
        )
    })
}

/// Resolves special class names used by `new` while preserving AOT fallback names.
pub(super) fn eval_new_object_class_name(
    class_name: &str,
    context: &ElephcEvalContext,
) -> Result<String, EvalStatus> {
    match class_name.to_ascii_lowercase().as_str() {
        "self" | "parent" | "static" => resolve_eval_static_class_name(class_name, context),
        _ => Ok(context
            .resolve_class_name(class_name)
            .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string())),
    }
}

/// Resolves a runtime class-name value used by dynamic class operations.
pub(in crate::interpreter) fn eval_dynamic_class_name(
    class_name: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    match values.type_tag(class_name)? {
        EVAL_TAG_STRING => {
            let bytes = values.string_bytes(class_name)?;
            let class_name = String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
            Ok(context
                .resolve_class_like_name(&class_name)
                .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string()))
        }
        EVAL_TAG_OBJECT => eval_instanceof_object_target_name(class_name, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Converts one evaluated class-name operand and retires its temporary owner on every exit.
pub(in crate::interpreter) fn eval_owned_dynamic_class_name(
    class_name: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let owner = crate::runtime_hooks::owner_frames::RuntimeOwnerSlot::new(class_name);
    crate::runtime_hooks::owner_frames::with_runtime_owners(&owner, || {
        let result = eval_dynamic_class_name(owner.borrowed(), context, values);
        let cleanup = release_expr_result(owner.take(), context, values);
        result.and_then(|class_name| cleanup.map(|()| class_name))
    })
}

/// Returns the runtime class name for `$object::class` and rejects non-object dynamic receivers.
pub(super) fn eval_dynamic_class_name_fetch_result(
    class_name: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let tag = values.type_tag(class_name)?;
    if tag == EVAL_TAG_OBJECT {
        let class_name = eval_instanceof_object_target_name(class_name, context, values)?;
        return values.string(&class_name);
    }
    eval_throw_type_error(
        &format!(
            "Cannot use \"::class\" on {}",
            eval_class_name_fetch_type_error_name(tag)
        ),
        context,
        values,
    )
}

/// Returns PHP's type label for dynamic `::class` TypeError diagnostics.
fn eval_class_name_fetch_type_error_name(tag: u64) -> &'static str {
    match tag {
        EVAL_TAG_INT => "int",
        EVAL_TAG_FLOAT => "float",
        EVAL_TAG_STRING => "string",
        EVAL_TAG_BOOL => "bool",
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => "array",
        EVAL_TAG_RESOURCE => "resource",
        EVAL_TAG_NULL => "null",
        _ => "null",
    }
}

/// Evaluates PHP's `instanceof` operator over static and dynamic class targets.
pub(super) fn eval_instanceof_expr(
    value: &EvalExpr,
    target: &EvalInstanceOfTarget,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_eval_operands(&[value], context, scope, values, |operands, context, scope, values| match target {
        EvalInstanceOfTarget::ClassName(class_name) => {
            let target_class = eval_instanceof_static_target_name(class_name, context)?;
            eval_instanceof_value_result(operands[0], &target_class, context, values)
        }
        EvalInstanceOfTarget::Expr(target) => {
            with_eval_operands(&[target], context, scope, values, |targets, context, _, values| {
                let target_class = eval_instanceof_dynamic_target_name(targets[0], context, values)?;
                eval_instanceof_value_result(operands[0], &target_class, context, values)
            })
        }
    })
}

/// Tests borrowed operands after evaluation has acquired their normal and exceptional leases.
fn eval_instanceof_value_result(
    value: RuntimeCellHandle, target_class: &str,
    context: &ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let tag = values.type_tag(value)?;
    let result = if tag == EVAL_TAG_CALLABLE && target_class.eq_ignore_ascii_case("Closure") {
        true
    } else if tag == EVAL_TAG_OBJECT {
        eval_instanceof_object_result(value, target_class, context, values)?
    } else { false };
    values.bool_value(result)
}

/// Resolves a static `instanceof` target according to eval class aliases and scope keywords.
fn eval_instanceof_static_target_name(
    class_name: &str,
    context: &ElephcEvalContext,
) -> Result<String, EvalStatus> {
    match class_name.to_ascii_lowercase().as_str() {
        "self" | "parent" | "static" => resolve_eval_static_class_name(class_name, context),
        _ => Ok(eval_instanceof_resolved_target_name(class_name, context)),
    }
}

/// Resolves a dynamic `instanceof` target cell to the PHP class name it represents.
fn eval_instanceof_dynamic_target_name(
    target: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    match values.type_tag(target)? {
        EVAL_TAG_STRING => {
            let target = eval_instanceof_string_target_name(target, values)?;
            Ok(eval_instanceof_resolved_target_name(&target, context))
        }
        EVAL_TAG_OBJECT => eval_instanceof_object_target_name(target, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Reads and normalizes one string-valued dynamic `instanceof` target.
fn eval_instanceof_string_target_name(
    target: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let bytes = values.string_bytes(target)?;
    let target = String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
    Ok(target.trim_start_matches('\\').to_string())
}

/// Reads the runtime class of an object-valued dynamic `instanceof` target.
fn eval_instanceof_object_target_name(
    target: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let identity = values.object_identity(target)?;
    if let Some(class) = context.dynamic_object_class(identity) {
        return Ok(class.name().to_string());
    }
    let class_name = runtime_object_class_name(target, values)?;
    Ok(class_name.trim_start_matches('\\').to_string())
}

/// Applies eval alias resolution to a target class name without requiring it to exist.
fn eval_instanceof_resolved_target_name(target: &str, context: &ElephcEvalContext) -> String {
    context
        .resolve_class_name(target)
        .unwrap_or_else(|| target.trim_start_matches('\\').to_string())
}

/// Tests one object cell against a resolved `instanceof` target class/interface name.
pub(super) fn eval_instanceof_object_result(
    value: RuntimeCellHandle,
    target_class: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    dynamic_object_is_a(value, target_class, false, context, values)?
        .map_or_else(|| values.object_is_a(value, target_class, false), Ok)
}

/// Evaluates a PHP `match` expression with strict comparison and lazy arm values.
pub(in crate::interpreter) fn eval_match_expr(
    subject: &EvalExpr,
    arms: &[EvalMatchArm],
    default: Option<&EvalExpr>,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
    _own_result: bool,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_eval_operands(&[subject], context, scope, values, |args, context, scope, values| {
        for arm in arms {
            for pattern in &arm.patterns {
                let matched = with_eval_operands(&[pattern], context, scope, values, |patterns, _, _, values| {
                    values.compare(EvalBinOp::StrictEq, args[0], patterns[0])
                })?;
                let truthy = values.truthy(matched);
                let released = values.release(matched);
                let truthy = truthy?;
                released?;
                if truthy {
                    return eval_expr(&arm.value, context, scope, values);
                }
            }
        }
        default
            .map(|expr| eval_expr(expr, context, scope, values))
            .unwrap_or(Err(EvalStatus::RuntimeFatal))
    })
}
