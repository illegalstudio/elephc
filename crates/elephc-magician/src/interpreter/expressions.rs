//! Purpose:
//! Evaluates EvalIR expressions, match expressions, function-like calls, and positional builtin dispatch.
//!
//! Called from:
//! - `crate::interpreter::statements` for expression statements and expression-bearing statements.
//! - Eval builtin modules when they need to evaluate unevaluated argument expressions.
//!
//! Key details:
//! - PHP call argument evaluation order is preserved before binding or ABI-like materialization.
//! - Language constructs such as `eval`, `isset`, `empty`, and `unset` receive unevaluated expressions.

use super::*;

/// Evaluates one expression to an opaque runtime-cell handle.
pub(in crate::interpreter) fn eval_expr(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match expr {
        EvalExpr::Array(elements) => {
            if elements
                .iter()
                .any(|element| {
                    matches!(
                        element,
                        EvalArrayElement::KeyValue { .. }
                            | EvalArrayElement::KeyReference { .. }
                    )
                })
            {
                eval_assoc_array(elements, context, scope, values)
            } else {
                eval_indexed_array(elements, context, scope, values)
            }
        }
        EvalExpr::ArrayGet { array, index } => {
            let array = eval_expr(array, context, scope, values)?;
            let index = eval_expr(index, context, scope, values)?;
            eval_array_get_result(array, index, context, values)
        }
        EvalExpr::Call { name, args } => eval_call(name, args, context, scope, values),
        EvalExpr::Cast { target, expr } => eval_cast_expr(target, expr, context, scope, values),
        EvalExpr::Const(value) => eval_const(value, values),
        EvalExpr::ConstFetch(name) => eval_const_fetch(name, context, values),
        EvalExpr::Closure {
            function,
            captures,
            is_static,
        } => eval_closure_expr(function, captures, *is_static, context, scope, values),
        EvalExpr::FunctionCallable {
            name,
            fallback_name,
        } => eval_function_callable_expr(name, fallback_name.as_deref(), context, values),
        EvalExpr::InvokableCallable { object } => {
            eval_invokable_callable_expr(object, context, scope, values)
        }
        EvalExpr::MethodCallable { object, method } => {
            eval_method_callable_expr(object, method, context, scope, values)
        }
        EvalExpr::StaticMethodCallable { class_name, method } => {
            eval_static_method_callable_expr(class_name, method, context, scope, values)
        }
        EvalExpr::DynamicStaticMethodCallable { class_name, method } => {
            eval_dynamic_static_method_callable_expr(class_name, method, context, scope, values)
        }
        EvalExpr::DynamicCall { callee, args } => {
            eval_dynamic_call(callee, args, context, scope, values)
        }
        EvalExpr::DynamicMethodCall {
            object,
            method,
            args,
        } => {
            let object = eval_expr(object, context, scope, values)?;
            let method = eval_dynamic_member_name(method, context, scope, values)?;
            let evaluated_args = eval_method_call_arg_values(args, context, scope, values)?;
            eval_method_call_result_with_evaluated_args(
                object,
                &method,
                evaluated_args,
                context,
                values,
            )
        }
        EvalExpr::DynamicNewObject { class_name, args } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            let args = eval_method_call_arg_values(args, context, scope, values)?;
            eval_new_object_result(&class_name, args, context, scope, values)
        }
        EvalExpr::DynamicPropertyGet { object, property } => {
            let object = eval_expr(object, context, scope, values)?;
            let property = eval_dynamic_member_name(property, context, scope, values)?;
            eval_property_get_result(object, &property, context, values)
        }
        EvalExpr::DynamicStaticMethodCall {
            class_name,
            method,
            args,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            let method = eval_dynamic_member_name(method, context, scope, values)?;
            let evaluated_args = eval_method_call_arg_values(args, context, scope, values)?;
            eval_static_method_call_result_from_scope(
                &class_name,
                &method,
                evaluated_args,
                scope,
                context,
                values,
            )
        }
        EvalExpr::DynamicStaticPropertyGet {
            class_name,
            property,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            eval_static_property_get_result(&class_name, property, context, values)
        }
        EvalExpr::DynamicStaticPropertyNameGet {
            class_name,
            property,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            let property = eval_dynamic_member_name(property, context, scope, values)?;
            eval_static_property_get_result(&class_name, &property, context, values)
        }
        EvalExpr::DynamicClassConstantFetch {
            class_name,
            constant,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            eval_class_constant_fetch_result(&class_name, constant, context, values)
        }
        EvalExpr::DynamicClassConstantNameFetch {
            class_name,
            constant,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            let constant = eval_dynamic_member_name(constant, context, scope, values)?;
            eval_class_constant_fetch_result(&class_name, &constant, context, values)
        }
        EvalExpr::DynamicClassNameFetch { class_name } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            eval_dynamic_class_name_fetch_result(class_name, context, values)
        }
        EvalExpr::Include {
            path,
            required,
            once,
        } => eval_include_expr(path, *required, *once, context, scope, values),
        EvalExpr::InstanceOf { value, target } => {
            eval_instanceof_expr(value, target, context, scope, values)
        }
        EvalExpr::LoadVar(name) => {
            visible_scope_cell(context, scope, name).map_or_else(|| values.null(), Ok)
        }
        EvalExpr::Magic(magic) => eval_magic_const(magic, context, values),
        EvalExpr::Match {
            subject,
            arms,
            default,
        } => eval_match_expr(subject, arms, default.as_deref(), context, scope, values),
        EvalExpr::Clone(object) => {
            let object = eval_expr(object, context, scope, values)?;
            eval_object_clone_result(object, context, values)
        }
        EvalExpr::NamespacedCall {
            name,
            fallback_name,
            args,
        } => eval_namespaced_call(name, fallback_name, args, context, scope, values),
        EvalExpr::NamespacedConstFetch {
            name,
            fallback_name,
        } => eval_namespaced_const_fetch(name, fallback_name, context, values),
        EvalExpr::NewObject { class_name, args } => {
            let args = eval_method_call_arg_values(args, context, scope, values)?;
            let class_name = eval_new_object_class_name(class_name, context)?;
            eval_new_object_result(&class_name, args, context, scope, values)
        }
        EvalExpr::NewAnonymousClass { class, args } => {
            ensure_eval_anonymous_class_decl(class, context, scope, values)?;
            let evaluated_args = eval_method_call_arg_values(args, context, scope, values)?;
            let class = context
                .class(class.name())
                .cloned()
                .ok_or(EvalStatus::RuntimeFatal)?;
            eval_dynamic_class_new_object(&class, evaluated_args, context, scope, values)
        }
        EvalExpr::StaticMethodCall {
            class_name,
            method,
            args,
        } => {
            let evaluated_args = eval_method_call_arg_values(args, context, scope, values)?;
            eval_static_method_call_result_from_scope(
                class_name,
                method,
                evaluated_args,
                scope,
                context,
                values,
            )
        }
        EvalExpr::StaticPropertyGet {
            class_name,
            property,
        } => eval_static_property_get_result(class_name, property, context, values),
        EvalExpr::ClassConstantFetch {
            class_name,
            constant,
        } => eval_class_constant_fetch_result(class_name, constant, context, values),
        EvalExpr::ClassNameFetch { class_name } => {
            eval_class_name_fetch_result(class_name, context, values)
        }
        EvalExpr::MethodCall {
            object,
            method,
            args,
        } => {
            let object = eval_expr(object, context, scope, values)?;
            let evaluated_args = eval_method_call_arg_values(args, context, scope, values)?;
            eval_method_call_result_with_evaluated_args(
                object,
                method,
                evaluated_args,
                context,
                values,
            )
        }
        EvalExpr::NullsafeMethodCall {
            object,
            method,
            args,
        } => {
            let object = eval_expr(object, context, scope, values)?;
            if values.is_null(object)? {
                return values.null();
            }
            let evaluated_args = eval_method_call_arg_values(args, context, scope, values)?;
            eval_method_call_result_with_evaluated_args(
                object,
                method,
                evaluated_args,
                context,
                values,
            )
        }
        EvalExpr::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => {
            let object = eval_expr(object, context, scope, values)?;
            if values.is_null(object)? {
                return values.null();
            }
            let method = eval_dynamic_member_name(method, context, scope, values)?;
            let evaluated_args = eval_method_call_arg_values(args, context, scope, values)?;
            eval_method_call_result_with_evaluated_args(
                object,
                &method,
                evaluated_args,
                context,
                values,
            )
        }
        EvalExpr::NullCoalesce { value, default } => {
            let value = eval_expr(value, context, scope, values)?;
            if values.is_null(value)? {
                eval_expr(default, context, scope, values)
            } else {
                Ok(value)
            }
        }
        EvalExpr::NullsafePropertyGet { object, property } => {
            let object = eval_expr(object, context, scope, values)?;
            if values.is_null(object)? {
                return values.null();
            }
            eval_property_get_result(object, property, context, values)
        }
        EvalExpr::NullsafeDynamicPropertyGet { object, property } => {
            let object = eval_expr(object, context, scope, values)?;
            if values.is_null(object)? {
                return values.null();
            }
            let property = eval_dynamic_member_name(property, context, scope, values)?;
            eval_property_get_result(object, &property, context, values)
        }
        EvalExpr::PropertyGet { object, property } => {
            let object = eval_expr(object, context, scope, values)?;
            eval_property_get_result(object, property, context, values)
        }
        EvalExpr::Print(inner) => {
            let value = eval_expr(inner, context, scope, values)?;
            let value = eval_string_context_value(value, context, values)?;
            values.echo(value)?;
            values.int(1)
        }
        EvalExpr::Ternary {
            condition,
            then_branch,
            else_branch,
        } => {
            let condition = eval_expr(condition, context, scope, values)?;
            if values.truthy(condition)? {
                if let Some(then_branch) = then_branch {
                    eval_expr(then_branch, context, scope, values)
                } else {
                    Ok(condition)
                }
            } else {
                eval_expr(else_branch, context, scope, values)
            }
        }
        EvalExpr::Unary { op, expr } => {
            let value = eval_expr(expr, context, scope, values)?;
            match op {
                EvalUnaryOp::Plus => {
                    let zero = values.int(0)?;
                    values.add(zero, value)
                }
                EvalUnaryOp::Negate => {
                    let zero = values.int(0)?;
                    values.sub(zero, value)
                }
                EvalUnaryOp::LogicalNot => {
                    let truthy = values.truthy(value)?;
                    values.bool_value(!truthy)
                }
                EvalUnaryOp::BitNot => values.bit_not(value),
            }
        }
        EvalExpr::Binary { op, left, right } => {
            if *op == EvalBinOp::LogicalAnd {
                let left = eval_expr(left, context, scope, values)?;
                if !values.truthy(left)? {
                    return values.bool_value(false);
                }
                let right = eval_expr(right, context, scope, values)?;
                let truthy = values.truthy(right)?;
                return values.bool_value(truthy);
            }
            if *op == EvalBinOp::LogicalOr {
                let left = eval_expr(left, context, scope, values)?;
                if values.truthy(left)? {
                    return values.bool_value(true);
                }
                let right = eval_expr(right, context, scope, values)?;
                let truthy = values.truthy(right)?;
                return values.bool_value(truthy);
            }
            let left = eval_expr(left, context, scope, values)?;
            let right = eval_expr(right, context, scope, values)?;
            eval_binary_result(*op, left, right, context, values)
        }
    }
}

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
        | EvalBinOp::ShiftRight => values.bitwise(op, left, right),
        EvalBinOp::Concat => {
            let left = eval_string_context_value(left, context, values)?;
            let right = eval_string_context_value(right, context, values)?;
            values.concat(left, right)
        }
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

/// Evaluates a runtime property or method name expression and returns its PHP string bytes as UTF-8.
pub(in crate::interpreter) fn eval_dynamic_member_name(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let value = eval_expr(expr, context, scope, values)?;
    let value = eval_string_context_value(value, context, values)?;
    let bytes = values.string_bytes(value)?;
    String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)
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
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_method_call_result(array, "offsetGet", vec![index], context, values)
}

/// Returns whether an object value satisfies PHP's `ArrayAccess` interface.
pub(in crate::interpreter) fn eval_array_access_object_matches(
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    dynamic_object_is_a(value, "ArrayAccess", false, context, values)?
        .map_or_else(|| values.object_is_a(value, "ArrayAccess", false), Ok)
}

/// Evaluates one PHP scalar cast expression through the runtime conversion hooks.
fn eval_cast_expr(
    target: &EvalCastType,
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = eval_expr(expr, context, scope, values)?;
    match target {
        EvalCastType::Int => values.cast_int(value),
        EvalCastType::Float => values.cast_float(value),
        EvalCastType::String => {
            let value = eval_string_context_value(value, context, values)?;
            values.cast_string(value)
        }
        EvalCastType::Bool => values.cast_bool(value),
    }
}

/// Constructs an object after the target class name and constructor arguments have been evaluated.
fn eval_new_object_result(
    class_name: &str,
    args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(object) =
        eval_reflection_owner_new_object(class_name, args.clone(), context, values)?
    {
        return Ok(object);
    }
    if let Some(class) = context.class(class_name).cloned() {
        return eval_dynamic_class_new_object(&class, args, context, scope, values);
    }
    let object = values.new_object(class_name)?;
    if let Err(err) =
        eval_native_constructor_with_evaluated_args(class_name, object, args, context, values)
    {
        let _ = values.release(object);
        return Err(err);
    }
    Ok(object)
}

/// Resolves special class names used by `new` while preserving AOT fallback names.
fn eval_new_object_class_name(
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

/// Returns the runtime class name for `$object::class` and rejects non-object dynamic receivers.
fn eval_dynamic_class_name_fetch_result(
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
fn eval_instanceof_expr(
    value: &EvalExpr,
    target: &EvalInstanceOfTarget,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = eval_expr(value, context, scope, values)?;
    let result = match target {
        EvalInstanceOfTarget::ClassName(class_name) => {
            if values.type_tag(value)? != EVAL_TAG_OBJECT {
                return values.bool_value(false);
            }
            let target_class = eval_instanceof_static_target_name(class_name, context)?;
            eval_instanceof_object_result(value, &target_class, context, values)?
        }
        EvalInstanceOfTarget::Expr(target) => {
            let target = eval_expr(target, context, scope, values)?;
            let target_class = eval_instanceof_dynamic_target_name(target, context, values)?;
            if values.type_tag(value)? == EVAL_TAG_OBJECT {
                eval_instanceof_object_result(value, &target_class, context, values)?
            } else {
                false
            }
        }
    };
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
    let class_name = values.object_class_name(target)?;
    let bytes = values.string_bytes(class_name);
    values.release(class_name)?;
    let class_name = String::from_utf8(bytes?).map_err(|_| EvalStatus::RuntimeFatal)?;
    Ok(class_name.trim_start_matches('\\').to_string())
}

/// Applies eval alias resolution to a target class name without requiring it to exist.
fn eval_instanceof_resolved_target_name(target: &str, context: &ElephcEvalContext) -> String {
    context
        .resolve_class_name(target)
        .unwrap_or_else(|| target.trim_start_matches('\\').to_string())
}

/// Tests one object cell against a resolved `instanceof` target class/interface name.
fn eval_instanceof_object_result(
    value: RuntimeCellHandle,
    target_class: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    dynamic_object_is_a(value, target_class, false, context, values)?
        .map_or_else(|| values.object_is_a(value, target_class, false), Ok)
}

/// Materializes one eval closure literal as a PHP-visible `Closure` object.
fn eval_closure_expr(
    function: &EvalFunction,
    captures: &[crate::eval_ir::EvalClosureCapture],
    is_static: bool,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut bindings = Vec::with_capacity(captures.len());
    for capture in captures {
        bindings.push(eval_closure_capture(capture, context, scope, values)?);
    }
    let closure = EvalClosure::new(function.clone(), bindings, is_static);
    let name = context.define_closure(closure);
    eval_closure_object_expr(EvalClosureObjectTarget::Named(name), context, values)
}

/// Materializes one PHP-visible `Closure` object for an eval callable target.
fn eval_closure_object_expr(
    target: EvalClosureObjectTarget,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let object = values.new_object("stdClass")?;
    let identity = values.object_identity(object)?;
    context.register_closure_object_target(identity, target);
    Ok(object)
}

/// Evaluates one closure capture from the defining scope.
fn eval_closure_capture(
    capture: &crate::eval_ir::EvalClosureCapture,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalClosureCaptureBinding, EvalStatus> {
    if capture.by_ref() {
        let expr = EvalExpr::LoadVar(capture.name().to_string());
        let (value, target) = eval_call_arg_value(&expr, context, scope, values)?;
        return Ok(EvalClosureCaptureBinding::new(
            capture.name(),
            value,
            target,
        ));
    }
    let value = if let Some(value) = visible_scope_cell(context, scope, capture.name()) {
        values.retain(value)?
    } else {
        values.null()?
    };
    Ok(EvalClosureCaptureBinding::new(capture.name(), value, None))
}

/// Evaluates a PHP `match` expression with strict comparison and lazy arm values.
pub(in crate::interpreter) fn eval_match_expr(
    subject: &EvalExpr,
    arms: &[EvalMatchArm],
    default: Option<&EvalExpr>,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let subject = eval_expr(subject, context, scope, values)?;
    for arm in arms {
        for pattern in &arm.patterns {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let matched = values.compare(EvalBinOp::StrictEq, subject, pattern)?;
            if values.truthy(matched)? {
                return eval_expr(&arm.value, context, scope, values);
            }
        }
    }
    default
        .map(|expr| eval_expr(expr, context, scope, values))
        .unwrap_or(Err(EvalStatus::RuntimeFatal))
}

/// Returns cloned positional argument expressions, rejecting named arguments.
pub(in crate::interpreter) fn positional_call_arg_exprs(
    args: &[EvalCallArg],
) -> Result<Vec<EvalExpr>, EvalStatus> {
    if args
        .iter()
        .any(|arg| arg.name().is_some() || arg.is_spread())
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(args.iter().map(|arg| arg.value().clone()).collect())
}

/// Evaluates method-call arguments, preserving named metadata for eval method binding.
pub(in crate::interpreter) fn eval_method_call_arg_values(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    eval_call_arg_values(args, context, scope, values)
}

/// Evaluates supported function-like calls from a runtime eval fragment.
pub(in crate::interpreter) fn eval_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if eval_expr_language_construct_name(name) {
        let args = positional_call_arg_exprs(args)?;
        return eval_positional_expr_call(name, &args, context, scope, values);
    }
    if name == "flock" {
        return eval_builtin_flock(args, context, scope, values);
    }
    if name == "preg_match" {
        return eval_builtin_preg_match_call(args, context, scope, values);
    }
    if name == "preg_match_all" {
        return eval_builtin_preg_match_all_call(args, context, scope, values);
    }
    if name == "is_callable" {
        return eval_builtin_is_callable_call(args, context, scope, values);
    }
    if matches!(name, "fsockopen" | "pfsockopen") {
        return eval_builtin_fsockopen_call(args, context, scope, values);
    }
    if let Some(result) = eval_date_procedural_alias_call(name, args, context, scope, values)? {
        return Ok(result);
    }
    if name == "stream_select" {
        return eval_builtin_stream_select_call(args, context, scope, values);
    }
    if name == "stream_socket_accept" {
        return eval_builtin_stream_socket_accept_call(args, context, scope, values);
    }
    if name == "stream_socket_recvfrom" {
        return eval_builtin_stream_socket_recvfrom_call(args, context, scope, values);
    }
    if matches!(
        name,
        "array_pop"
            | "array_push"
            | "array_shift"
            | "array_splice"
            | "array_unshift"
            | "array_walk"
            | "arsort"
            | "asort"
            | "krsort"
            | "ksort"
            | "natcasesort"
            | "natsort"
            | "rsort"
            | "shuffle"
            | "sort"
            | "settype"
            | "uasort"
            | "uksort"
            | "usort"
    ) {
        return eval_builtin_array_pop_shift_call(name, args, context, scope, values);
    }
    if eval_php_visible_builtin_exists(name) {
        if eval_call_args_are_plain_positional(args) {
            let args = positional_call_arg_exprs(args)?;
            return eval_positional_expr_call(name, &args, context, scope, values);
        }
        return eval_builtin_call(name, args, context, scope, values);
    }

    if let Some(function) = context.function(name).cloned() {
        return eval_dynamic_function(&function, args, context, scope, values);
    }
    if let Some(function) = context.native_function(name) {
        return eval_native_function(function, args, context, scope, values);
    }
    Err(EvalStatus::UnsupportedConstruct)
}

/// Evaluates an unqualified namespaced function call with PHP's global fallback.
pub(in crate::interpreter) fn eval_namespaced_call(
    name: &str,
    fallback_name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(function) = context.function(name).cloned() {
        return eval_dynamic_function(&function, args, context, scope, values);
    }
    if let Some(function) = context.native_function(name) {
        return eval_native_function(function, args, context, scope, values);
    }
    eval_call(fallback_name, args, context, scope, values)
}

/// Resolves a first-class function callable name with PHP namespace fallback rules.
fn eval_function_callable_expr(
    name: &str,
    fallback_name: Option<&str>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if eval_function_probe_exists(context, name) {
        return eval_closure_object_expr(
            EvalClosureObjectTarget::Named(name.trim_start_matches('\\').to_ascii_lowercase()),
            context,
            values,
        );
    }
    if let Some(fallback_name) = fallback_name {
        if eval_function_probe_exists(context, fallback_name) {
            return eval_closure_object_expr(
                EvalClosureObjectTarget::Named(
                    fallback_name.trim_start_matches('\\').to_ascii_lowercase(),
                ),
                context,
                values,
            );
        }
    }
    eval_throw_error(
        &format!("Call to undefined function {}()", name.trim_start_matches('\\')),
        context,
        values,
    )
}

/// Materializes an invokable-object first-class callable as a PHP `Closure` object.
fn eval_invokable_callable_expr(
    object: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let object = eval_expr(object, context, scope, values)?;
    eval_invokable_object_precheck(object, context, values)?;
    eval_closure_object_expr(
        EvalClosureObjectTarget::InvokableObject { object },
        context,
        values,
    )
}

/// Materializes an object method first-class callable and records captured AOT bridge scope.
fn eval_method_callable_expr(
    object: &EvalExpr,
    method: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let object = eval_expr(object, context, scope, values)?;
    let method = eval_dynamic_member_name(method, context, scope, values)?;
    let target = eval_method_callable_target(object, method, context, values)?;
    eval_closure_object_expr(target, context, values)
}

/// Validates and builds the retained target for an object method first-class callable.
fn eval_method_callable_target(
    object: RuntimeCellHandle,
    method_name: String,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalClosureObjectTarget, EvalStatus> {
    let Ok(identity) = values.object_identity(object) else {
        return Ok(EvalClosureObjectTarget::ObjectMethod {
            object,
            method: method_name,
            called_class: None,
            native_class: None,
            bridge_scope: None,
        });
    };
    let Some(class) = context.dynamic_object_class(identity) else {
        let class_name = runtime_object_class_name(object, values)?;
        return eval_native_object_method_callable_target(
            object,
            &class_name,
            &class_name,
            method_name,
            context,
            values,
        );
    };
    let called_class_name = class.name().to_string();
    if let Some((declaring_class, method)) =
        eval_dynamic_method_for_call(&called_class_name, &method_name, context)
    {
        if method.is_abstract() {
            return eval_first_class_abstract_method_error(
                &declaring_class,
                method.name(),
                context,
                values,
            );
        }
        if validate_eval_member_access(&declaring_class, method.visibility(), context).is_err()
            && !eval_instance_magic_callable_for_class(&called_class_name, context)
        {
            return eval_first_class_method_access_error(
                &declaring_class,
                method.name(),
                method.visibility(),
                context,
                values,
            );
        }
        return Ok(EvalClosureObjectTarget::ObjectMethod {
            object,
            method: method_name,
            called_class: Some(called_class_name),
            native_class: None,
            bridge_scope: None,
        });
    }
    if let Some(parent) = context.class_native_parent_name(&called_class_name) {
        return eval_native_object_method_callable_target(
            object,
            &parent,
            &called_class_name,
            method_name,
            context,
            values,
        );
    }
    if eval_instance_magic_callable_for_class(&called_class_name, context) {
        return Ok(EvalClosureObjectTarget::ObjectMethod {
            object,
            method: method_name,
            called_class: Some(called_class_name),
            native_class: None,
            bridge_scope: None,
        });
    }
    eval_first_class_undefined_method_error(&called_class_name, &method_name, context, values)
}

/// Validates generated/AOT object-method first-class callable metadata.
fn eval_native_object_method_callable_target(
    object: RuntimeCellHandle,
    native_class: &str,
    called_class_name: &str,
    method_name: String,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalClosureObjectTarget, EvalStatus> {
    let Some((declaring_class, visibility, _, is_abstract)) =
        eval_aot_method_dispatch_metadata_in_hierarchy(native_class, &method_name, context, values)?
    else {
        if eval_native_instance_magic_callable_for_class(native_class, context, values)?
            || !values.class_exists(native_class)?
        {
            return Ok(EvalClosureObjectTarget::ObjectMethod {
                object,
                method: method_name,
                called_class: Some(called_class_name.to_string()),
                native_class: None,
                bridge_scope: None,
            });
        }
        return eval_first_class_undefined_method_error(
            called_class_name,
            &method_name,
            context,
            values,
        );
    };
    if is_abstract {
        return eval_first_class_abstract_method_error(
            &declaring_class,
            &method_name,
            context,
            values,
        );
    }
    if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
        if eval_native_instance_magic_callable_for_class(native_class, context, values)? {
            return Ok(EvalClosureObjectTarget::ObjectMethod {
                object,
                method: method_name,
                called_class: Some(called_class_name.to_string()),
                native_class: None,
                bridge_scope: None,
            });
        }
        return eval_first_class_method_access_error(
            &declaring_class,
            &method_name,
            visibility,
            context,
            values,
        );
    }
    Ok(EvalClosureObjectTarget::ObjectMethod {
        object,
        method: method_name,
        called_class: Some(called_class_name.to_string()),
        native_class: Some(native_class.to_string()),
        bridge_scope: Some(declaring_class),
    })
}

/// Materializes a first-class static method callable while retaining late-static metadata.
fn eval_static_method_callable_expr(
    class_name: &str,
    method: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let receiver = resolve_eval_static_method_receiver(class_name, context)?;
    let method = eval_dynamic_member_name(method, context, scope, values)?;
    let target = eval_static_method_callable_target(
        receiver.dispatch_class,
        method,
        Some(receiver.called_class),
        Some(scope),
        context,
        values,
    )?;
    eval_closure_object_expr(target, context, values)
}

/// Materializes a runtime-class static first-class callable as a PHP `Closure` object.
fn eval_dynamic_static_method_callable_expr(
    class_name: &EvalExpr,
    method: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let class_name = eval_expr(class_name, context, scope, values)?;
    let class_name = eval_dynamic_class_name(class_name, context, values)?;
    let receiver = resolve_eval_static_method_receiver(&class_name, context)?;
    let method = eval_dynamic_member_name(method, context, scope, values)?;
    let target = eval_static_method_callable_target(
        receiver.dispatch_class,
        method,
        Some(receiver.called_class),
        Some(scope),
        context,
        values,
    )?;
    eval_closure_object_expr(target, context, values)
}

/// Validates and builds the retained target for a static-method first-class callable.
fn eval_static_method_callable_target(
    dispatch_class: String,
    method_name: String,
    called_class: Option<String>,
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalClosureObjectTarget, EvalStatus> {
    if let Some((declaring_class, method)) =
        eval_dynamic_static_method_for_call(&dispatch_class, &method_name, context)
    {
        if method.is_abstract() {
            return eval_first_class_abstract_method_error(
                &declaring_class,
                method.name(),
                context,
                values,
            );
        }
        if validate_eval_member_access(&declaring_class, method.visibility(), context).is_err()
            && !eval_static_magic_callable_for_class(&dispatch_class, context)
        {
            return eval_first_class_method_access_error(
                &declaring_class,
                method.name(),
                method.visibility(),
                context,
                values,
            );
        }
        if !method.is_static() {
            if let Some(object) = eval_static_syntax_instance_receiver(
                &dispatch_class,
                lexical_scope,
                context,
                values,
            )? {
                return Ok(EvalClosureObjectTarget::ObjectMethod {
                    object,
                    method: method_name,
                    called_class,
                    native_class: None,
                    bridge_scope: None,
                });
            }
            return eval_first_class_non_static_method_error(
                &declaring_class,
                method.name(),
                context,
                values,
            );
        }
        return Ok(EvalClosureObjectTarget::StaticMethod {
            class_name: dispatch_class,
            method: method_name,
            called_class,
            native_class: None,
            bridge_scope: None,
        });
    }
    if context.has_class(&dispatch_class) {
        if let Some(parent) = context.class_native_parent_name(&dispatch_class) {
            return eval_native_static_method_callable_target(
                dispatch_class,
                parent,
                method_name,
                called_class,
                lexical_scope,
                context,
                values,
            );
        }
        if eval_static_magic_callable_for_class(&dispatch_class, context) {
            return Ok(EvalClosureObjectTarget::StaticMethod {
                class_name: dispatch_class,
                method: method_name,
                called_class,
                native_class: None,
                bridge_scope: None,
            });
        }
        return eval_first_class_undefined_method_error(
            &dispatch_class,
            &method_name,
            context,
            values,
        );
    }
    if context.has_interface(&dispatch_class)
        || context.has_trait(&dispatch_class)
        || context.has_enum(&dispatch_class)
    {
        if eval_static_magic_callable_for_class(&dispatch_class, context) {
            return Ok(EvalClosureObjectTarget::StaticMethod {
                class_name: dispatch_class,
                method: method_name,
                called_class,
                native_class: None,
                bridge_scope: None,
            });
        }
        return eval_first_class_undefined_method_error(
            &dispatch_class,
            &method_name,
            context,
            values,
        );
    }
    eval_native_static_method_callable_target(
        dispatch_class.clone(),
        dispatch_class,
        method_name,
        called_class,
        lexical_scope,
        context,
        values,
    )
}

/// Validates generated/AOT static-method first-class callable metadata.
fn eval_native_static_method_callable_target(
    dispatch_class: String,
    native_class: String,
    method_name: String,
    called_class: Option<String>,
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalClosureObjectTarget, EvalStatus> {
    let Some((declaring_class, visibility, is_static, is_abstract)) =
        eval_aot_method_dispatch_metadata_in_hierarchy(
            &native_class,
            &method_name,
            context,
            values,
        )?
    else {
        if context
            .native_static_method_signature(&native_class, &method_name)
            .is_some()
        {
            return Ok(EvalClosureObjectTarget::StaticMethod {
                class_name: dispatch_class,
                method: method_name,
                called_class,
                native_class: None,
                bridge_scope: None,
            });
        }
        if eval_native_static_magic_callable_for_class(&native_class, context, values)?
            || !values.class_exists(&native_class)?
        {
            return Ok(EvalClosureObjectTarget::StaticMethod {
                class_name: dispatch_class,
                method: method_name,
                called_class,
                native_class: None,
                bridge_scope: None,
            });
        }
        return eval_first_class_undefined_method_error(
            &dispatch_class,
            &method_name,
            context,
            values,
        );
    };
    if is_abstract {
        return eval_first_class_abstract_method_error(
            &declaring_class,
            &method_name,
            context,
            values,
        );
    }
    if validate_eval_member_access(&declaring_class, visibility, context).is_err() {
        if eval_native_static_magic_callable_for_class(&native_class, context, values)? {
            return Ok(EvalClosureObjectTarget::StaticMethod {
                class_name: dispatch_class,
                method: method_name,
                called_class,
                native_class: None,
                bridge_scope: None,
            });
        }
        return eval_first_class_method_access_error(
            &declaring_class,
            &method_name,
            visibility,
            context,
            values,
        );
    }
    if !is_static {
        if let Some(object) = eval_static_syntax_instance_receiver(
            &dispatch_class,
            lexical_scope,
            context,
            values,
        )? {
            return Ok(EvalClosureObjectTarget::ObjectMethod {
                object,
                method: method_name,
                called_class,
                native_class: Some(native_class),
                bridge_scope: Some(declaring_class),
            });
        }
        return eval_first_class_non_static_method_error(
            &declaring_class,
            &method_name,
            context,
            values,
        );
    }
    Ok(EvalClosureObjectTarget::StaticMethod {
        class_name: dispatch_class,
        method: method_name,
        called_class,
        native_class: Some(native_class),
        bridge_scope: Some(declaring_class),
    })
}

/// Returns whether an eval class has an instance magic-call fallback for a callable.
fn eval_instance_magic_callable_for_class(
    class_name: &str,
    context: &ElephcEvalContext,
) -> bool {
    context
        .class_method(class_name, "__call")
        .is_some_and(|(_, method)| !method.is_static() && !method.is_abstract())
}

/// Returns whether an eval class has a static magic-call fallback for a callable.
fn eval_static_magic_callable_for_class(
    class_name: &str,
    context: &ElephcEvalContext,
) -> bool {
    context
        .class_method(class_name, "__callStatic")
        .is_some_and(|(_, method)| method.is_static() && !method.is_abstract())
}

/// Returns whether an AOT class has an instance magic-call fallback for a callable.
fn eval_native_instance_magic_callable_for_class(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(eval_aot_method_dispatch_metadata_in_hierarchy(class_name, "__call", context, values)?
        .is_some_and(|(_, _, is_static, is_abstract)| !is_static && !is_abstract))
}

/// Returns whether an AOT class has a static magic-call fallback for a callable.
fn eval_native_static_magic_callable_for_class(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(
        eval_aot_method_dispatch_metadata_in_hierarchy(
            class_name,
            "__callStatic",
            context,
            values,
        )?
        .is_some_and(|(_, _, is_static, is_abstract)| is_static && !is_abstract),
    )
}

/// Throws PHP's first-class callable error for an inaccessible method.
fn eval_first_class_method_access_error<T>(
    declaring_class: &str,
    method_name: &str,
    visibility: EvalVisibility,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Call to {} method {}::{}() from {}",
            eval_callable_visibility_label(visibility),
            declaring_class.trim_start_matches('\\'),
            method_name,
            eval_callable_scope_label(context)
        ),
        context,
        values,
    )
}

/// Throws PHP's first-class callable error for an instance method used statically.
fn eval_first_class_non_static_method_error<T>(
    declaring_class: &str,
    method_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Non-static method {}::{}() cannot be called statically",
            declaring_class.trim_start_matches('\\'),
            method_name
        ),
        context,
        values,
    )
}

/// Throws PHP's first-class callable error for an abstract method target.
fn eval_first_class_abstract_method_error<T>(
    declaring_class: &str,
    method_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Cannot call abstract method {}::{}()",
            declaring_class.trim_start_matches('\\'),
            method_name
        ),
        context,
        values,
    )
}

/// Throws PHP's first-class callable error for a missing method target.
fn eval_first_class_undefined_method_error<T>(
    class_name: &str,
    method_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!(
            "Call to undefined method {}::{}()",
            class_name.trim_start_matches('\\'),
            method_name
        ),
        context,
        values,
    )
}

/// Returns the current PHP scope label used in callable access errors.
fn eval_callable_scope_label(context: &ElephcEvalContext) -> String {
    context.current_class_scope().map_or_else(
        || String::from("global scope"),
        |class_name| format!("scope {}", class_name.trim_start_matches('\\')),
    )
}

/// Returns PHP's lowercase visibility label for callable access errors.
fn eval_callable_visibility_label(visibility: EvalVisibility) -> &'static str {
    match visibility {
        EvalVisibility::Public => "public",
        EvalVisibility::Protected => "protected",
        EvalVisibility::Private => "private",
    }
}

/// Evaluates a variable or expression callable and dispatches it with source-order arguments.
pub(in crate::interpreter) fn eval_dynamic_call(
    callee: &EvalExpr,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let callback = eval_expr(callee, context, scope, values)?;
    if values.type_tag(callback)? == EVAL_TAG_OBJECT {
        let is_closure_object = values
            .object_identity(callback)
            .ok()
            .and_then(|identity| context.closure_object_target(identity))
            .is_some();
        if !is_closure_object {
            eval_invokable_object_precheck(callback, context, values)?;
            let evaluated_args = eval_call_arg_values(args, context, scope, values)?;
            return eval_invokable_object_call_result(callback, evaluated_args, context, values);
        }
    }
    let callback = eval_callable(callback, context, values)?;
    let evaluated_args = eval_call_arg_values(args, context, scope, values)?;
    eval_evaluated_callable_with_call_array_args(&callback, evaluated_args, context, values)
}

/// Returns true for language constructs that need unevaluated argument expressions.
pub(in crate::interpreter) fn eval_expr_language_construct_name(name: &str) -> bool {
    matches!(name, "empty" | "eval" | "isset" | "unset")
}

/// Returns true when every source argument is plain positional.
pub(in crate::interpreter) fn eval_call_args_are_plain_positional(args: &[EvalCallArg]) -> bool {
    args.iter()
        .all(|arg| arg.name().is_none() && !arg.is_spread())
}

/// Evaluates builtins and language constructs after positional-only argument validation.
pub(in crate::interpreter) fn eval_positional_expr_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(result) = eval_declared_builtin_direct_call(name, args, context, scope, values)? {
        return Ok(result);
    }

    match name {
        "array_combine" => eval_builtin_array_combine(args, context, scope, values),
        "array_chunk" => eval_builtin_array_chunk(args, context, scope, values),
        "array_column" => eval_builtin_array_column(args, context, scope, values),
        "array_fill" => eval_builtin_array_fill(args, context, scope, values),
        "array_fill_keys" => eval_builtin_array_fill_keys(args, context, scope, values),
        "array_filter" => eval_builtin_array_filter(args, context, scope, values),
        "array_flip" => eval_builtin_array_flip(args, context, scope, values),
        "array_map" => eval_builtin_array_map(args, context, scope, values),
        "array_reduce" => eval_builtin_array_reduce(args, context, scope, values),
        "array_walk" => eval_builtin_array_walk(args, context, scope, values),
        "array_keys" | "array_values" => {
            eval_builtin_array_projection(name, args, context, scope, values)
        }
        "array_key_exists" => eval_builtin_array_key_exists(args, context, scope, values),
        "array_diff" | "array_intersect" => {
            eval_builtin_array_value_set(name, args, context, scope, values)
        }
        "array_diff_key" | "array_intersect_key" => {
            eval_builtin_array_key_set(name, args, context, scope, values)
        }
        "array_merge" => eval_builtin_array_merge(args, context, scope, values),
        "array_product" | "array_sum" => {
            eval_builtin_array_aggregate(name, args, context, scope, values)
        }
        "array_pad" => eval_builtin_array_pad(args, context, scope, values),
        "array_rand" => eval_builtin_array_rand(args, context, scope, values),
        "array_reverse" => eval_builtin_array_reverse(args, context, scope, values),
        "array_search" | "in_array" => {
            eval_builtin_array_search(name, args, context, scope, values)
        }
        "array_slice" => eval_builtin_array_slice(args, context, scope, values),
        "array_unique" => eval_builtin_array_unique(args, context, scope, values),
        "basename" => eval_builtin_basename(args, context, scope, values),
        "chdir" | "mkdir" | "rmdir" => {
            eval_builtin_unary_path_bool(name, args, context, scope, values)
        }
        "chmod" => eval_builtin_chmod(args, context, scope, values),
        "clearstatcache" => eval_builtin_clearstatcache(args, context, scope, values),
        "call_user_func" => eval_builtin_call_user_func(args, context, scope, values),
        "call_user_func_array" => eval_builtin_call_user_func_array(args, context, scope, values),
        "class_alias" => eval_builtin_class_alias(args, context, scope, values),
        "class_attribute_args" => {
            eval_builtin_class_attribute_metadata(name, args, context, scope, values)
        }
        "class_attribute_names" | "class_get_attributes" => {
            eval_builtin_class_attribute_metadata(name, args, context, scope, values)
        }
        "class_exists" => eval_builtin_class_exists(args, context, scope, values),
        "class_implements" | "class_parents" | "class_uses" => {
            eval_builtin_class_relation(name, args, context, scope, values)
        }
        "method_exists" | "property_exists" => {
            eval_builtin_member_exists(name, args, context, scope, values)
        }
        "get_class_methods" => eval_builtin_get_class_methods(args, context, scope, values),
        "get_class_vars" => eval_builtin_get_class_vars(args, context, scope, values),
        "get_object_vars" => eval_builtin_get_object_vars(args, context, scope, values),
        "interface_exists" => eval_builtin_interface_exists(args, context, scope, values),
        "trait_exists" | "enum_exists" => {
            eval_builtin_class_like_exists(name, args, context, scope, values)
        }
        "is_a" | "is_subclass_of" => eval_builtin_is_a_relation(name, args, context, scope, values),
        "closedir" | "readdir" | "rewinddir" => {
            eval_builtin_unary_directory(name, args, context, scope, values)
        }
        "chop" => eval_builtin_trim_like(name, args, context, scope, values),
        "count" => eval_builtin_count(args, context, scope, values),
        "copy" | "link" | "rename" | "symlink" => {
            eval_builtin_binary_path_bool(name, args, context, scope, values)
        }
        "checkdate" => eval_builtin_checkdate(args, context, scope, values),
        "date" | "gmdate" => eval_builtin_date_like(name, args, context, scope, values),
        "date_default_timezone_get" => {
            eval_builtin_date_default_timezone_get(args, context, values)
        }
        "date_default_timezone_set" => {
            eval_builtin_date_default_timezone_set(args, context, scope, values)
        }
        "define" => eval_builtin_define(args, context, scope, values),
        "defined" => eval_builtin_defined(args, context, scope, values),
        "dirname" => eval_builtin_dirname(args, context, scope, values),
        "die" | "exit" => eval_builtin_exit(args, context, scope, values),
        "disk_free_space" | "disk_total_space" => {
            eval_builtin_disk_space(name, args, context, scope, values)
        }
        "empty" => eval_builtin_empty(args, context, scope, values),
        "exec" | "shell_exec" | "system" | "passthru" => {
            eval_builtin_process_command(name, args, context, scope, values)
        }
        "eval" => eval_nested_eval(args, context, scope, values),
        "explode" => eval_builtin_explode(args, context, scope, values),
        "file" => eval_builtin_file(args, context, scope, values),
        "file_exists" => eval_builtin_file_probe(name, args, context, scope, values),
        "fileatime" | "filectime" | "filegroup" | "fileinode" | "filemtime" | "fileowner"
        | "fileperms" => eval_builtin_file_stat_scalar(name, args, context, scope, values),
        "file_get_contents" => eval_builtin_file_get_contents(args, context, scope, values),
        "file_put_contents" => eval_builtin_file_put_contents(args, context, scope, values),
        "fclose"
        | "fgetc"
        | "fgets"
        | "feof"
        | "fflush"
        | "fpassthru"
        | "fsync"
        | "fdatasync"
        | "ftell"
        | "rewind"
        | "fstat"
        | "stream_get_meta_data" => eval_builtin_unary_stream(name, args, context, scope, values),
        "filesize" => eval_builtin_filesize(args, context, scope, values),
        "filetype" => eval_builtin_filetype(args, context, scope, values),
        "fnmatch" => eval_builtin_fnmatch(args, context, scope, values),
        "fgetcsv" => eval_builtin_fgetcsv(args, context, scope, values),
        "fopen" => eval_builtin_fopen(args, context, scope, values),
        "fputcsv" => eval_builtin_fputcsv(args, context, scope, values),
        "fprintf" => eval_builtin_fprintf(args, context, scope, values),
        "fread" => eval_builtin_fread(args, context, scope, values),
        "fsockopen" | "pfsockopen" => eval_builtin_fsockopen(args, context, scope, values),
        "fscanf" => eval_builtin_fscanf(args, context, scope, values),
        "fseek" => eval_builtin_fseek(args, context, scope, values),
        "ftruncate" => eval_builtin_ftruncate(args, context, scope, values),
        "fwrite" => eval_builtin_fwrite(args, context, scope, values),
        "stat" | "lstat" => eval_builtin_stat_array(name, args, context, scope, values),
        "function_exists" | "is_callable" => {
            eval_builtin_function_probe(name, args, context, scope, values)
        }
        "getdate" => eval_builtin_getdate(args, context, scope, values),
        "gethostbyaddr" => eval_builtin_gethostbyaddr(args, context, scope, values),
        "gethostbyname" => eval_builtin_gethostbyname(args, context, scope, values),
        "gethostname" => eval_builtin_gethostname(args, values),
        "getprotobyname" => eval_builtin_getprotobyname(args, context, scope, values),
        "getprotobynumber" => eval_builtin_getprotobynumber(args, context, scope, values),
        "getservbyname" => eval_builtin_getservbyname(args, context, scope, values),
        "getservbyport" => eval_builtin_getservbyport(args, context, scope, values),
        "get_called_class" => eval_builtin_get_called_class(args, context, values),
        "get_class" => eval_builtin_get_class(args, context, scope, values),
        "get_declared_classes" | "get_declared_interfaces" | "get_declared_traits" => {
            eval_builtin_get_declared_symbols(name, args, context, values)
        }
        "get_parent_class" => eval_builtin_get_parent_class(args, context, scope, values),
        "get_resource_id" | "get_resource_type" => {
            eval_builtin_resource_introspection(name, args, context, scope, values)
        }
        "getcwd" => eval_builtin_getcwd(args, values),
        "getenv" => eval_builtin_getenv(args, context, scope, values),
        "glob" => eval_builtin_glob(args, context, scope, values),
        "grapheme_strrev" => eval_builtin_grapheme_strrev(args, context, scope, values),
        "gzcompress" | "gzdeflate" | "gzinflate" | "gzuncompress" => {
            eval_builtin_gzip(name, args, context, scope, values)
        }
        "hash" | "hash_file" | "hash_hmac" | "md5" | "sha1" => {
            eval_builtin_hash_one_shot(name, args, context, scope, values)
        }
        "header" => eval_builtin_header(args, context, scope, values),
        "chown" | "chgrp" | "lchown" | "lchgrp" => {
            eval_builtin_chown_like(name, args, context, scope, values)
        }
        "hash_algos" => eval_builtin_hash_algos(args, values),
        "hash_copy" => eval_builtin_hash_copy(args, context, scope, values),
        "hash_equals" => eval_builtin_hash_equals(args, context, scope, values),
        "hash_final" => eval_builtin_hash_final(args, context, scope, values),
        "hash_init" => eval_builtin_hash_init(args, context, scope, values),
        "hash_update" => eval_builtin_hash_update(args, context, scope, values),
        "html_entity_decode" | "htmlentities" | "htmlspecialchars" => {
            eval_builtin_html_entity(name, args, context, scope, values)
        }
        "implode" => eval_builtin_implode(args, context, scope, values),
        "inet_ntop" => eval_builtin_inet_ntop(args, context, scope, values),
        "inet_pton" => eval_builtin_inet_pton(args, context, scope, values),
        "iterator_apply" => eval_builtin_iterator_apply(args, context, scope, values),
        "iterator_count" => eval_builtin_iterator_count(args, context, scope, values),
        "iterator_to_array" => eval_builtin_iterator_to_array(args, context, scope, values),
        "is_dir" | "is_executable" | "is_file" | "is_link" | "is_readable" | "is_writable"
        | "is_writeable" => eval_builtin_file_probe(name, args, context, scope, values),
        "hrtime" => eval_builtin_hrtime(args, context, scope, values),
        "http_response_code" => eval_builtin_http_response_code(args, context, scope, values),
        "ip2long" => eval_builtin_ip2long(args, context, scope, values),
        "json_decode" => eval_builtin_json_decode(args, context, scope, values),
        "json_encode" => eval_builtin_json_encode(args, context, scope, values),
        "json_last_error" => eval_builtin_json_last_error(args, context, values),
        "json_last_error_msg" => eval_builtin_json_last_error_msg(args, context, values),
        "json_validate" => eval_builtin_json_validate(args, context, scope, values),
        "linkinfo" => eval_builtin_linkinfo(args, context, scope, values),
        "ltrim" | "rtrim" => eval_builtin_trim_like(name, args, context, scope, values),
        "localtime" => eval_builtin_localtime(args, context, scope, values),
        "microtime" => eval_builtin_microtime(args, context, scope, values),
        "mktime" | "gmmktime" => eval_builtin_mktime_like(name, args, context, scope, values),
        "nl2br" => eval_builtin_nl2br(args, context, scope, values),
        "opendir" => eval_builtin_opendir(args, context, scope, values),
        "pathinfo" => eval_builtin_pathinfo(args, context, scope, values),
        "php_uname" => eval_builtin_php_uname(args, context, scope, values),
        "phpversion" => eval_builtin_phpversion(args, values),
        "pclose" => eval_builtin_pclose(args, context, scope, values),
        "popen" => eval_builtin_popen(args, context, scope, values),
        "preg_match" => eval_builtin_preg_match(args, context, scope, values),
        "preg_match_all" => eval_builtin_preg_match_all(args, context, scope, values),
        "preg_replace" => eval_builtin_preg_replace(args, context, scope, values),
        "preg_replace_callback" => eval_builtin_preg_replace_callback(args, context, scope, values),
        "preg_split" => eval_builtin_preg_split(args, context, scope, values),
        "buffer_free" | "buffer_len" | "buffer_new" | "ptr" | "ptr_get" | "ptr_is_null"
        | "ptr_null" | "ptr_offset" | "ptr_read8" | "ptr_read16" | "ptr_read32"
        | "ptr_read_string" | "ptr_set" | "ptr_sizeof" | "ptr_write8" | "ptr_write16"
        | "ptr_write32" | "ptr_write_string" => {
            eval_builtin_raw_memory(name, args, context, scope, values)
        }
        "print_r" => eval_builtin_print_r(args, context, scope, values),
        "putenv" => eval_builtin_putenv(args, context, scope, values),
        "rand" | "mt_rand" => eval_builtin_rand(args, context, scope, values),
        "random_int" => eval_builtin_random_int(args, context, scope, values),
        "range" => eval_builtin_range(args, context, scope, values),
        "readfile" => eval_builtin_readfile(args, context, scope, values),
        "readline" => eval_builtin_readline(args, context, scope, values),
        "readlink" => eval_builtin_readlink(args, context, scope, values),
        "realpath" => eval_builtin_realpath(args, context, scope, values),
        "realpath_cache_get" => eval_builtin_realpath_cache_get(args, values),
        "realpath_cache_size" => eval_builtin_realpath_cache_size(args, values),
        "scandir" => eval_builtin_scandir(args, context, scope, values),
        "isset" => eval_builtin_isset(args, context, scope, values),
        "sleep" => eval_builtin_sleep(args, context, scope, values),
        "spl_autoload_register" | "spl_autoload_unregister" => {
            eval_builtin_spl_autoload_bool(name, args, context, scope, values)
        }
        "spl_autoload" | "spl_autoload_call" => {
            eval_builtin_spl_autoload_void(name, args, context, scope, values)
        }
        "spl_autoload_functions" => {
            eval_builtin_spl_autoload_functions(args, context, scope, values)
        }
        "spl_autoload_extensions" => {
            eval_builtin_spl_autoload_extensions(args, context, scope, values)
        }
        "spl_classes" => eval_builtin_spl_classes(args, values),
        "spl_object_id" | "spl_object_hash" => {
            eval_builtin_spl_object_identity(name, args, context, scope, values)
        }
        "sscanf" => eval_builtin_sscanf(args, context, scope, values),
        "sprintf" | "printf" => eval_builtin_sprintf_like(name, args, context, scope, values),
        "sys_get_temp_dir" => eval_builtin_sys_get_temp_dir(args, values),
        "tempnam" => eval_builtin_tempnam(args, context, scope, values),
        "time" => eval_builtin_time(args, values),
        "touch" => eval_builtin_touch(args, context, scope, values),
        "tmpfile" => eval_builtin_tmpfile(args, context, values),
        "stream_is_local" | "stream_supports_lock" => {
            eval_builtin_stream_bool_predicate(name, args, context, scope, values)
        }
        "stream_get_filters" | "stream_get_transports" | "stream_get_wrappers" => {
            eval_builtin_stream_introspection(name, args, context, values)
        }
        "stream_resolve_include_path" => {
            eval_builtin_stream_resolve_include_path(args, context, scope, values)
        }
        "stream_copy_to_stream" => eval_builtin_stream_copy_to_stream(args, context, scope, values),
        "stream_context_create" => eval_builtin_stream_context_create(args, context, scope, values),
        "stream_context_get_default" => {
            eval_builtin_stream_context_get_default(args, context, scope, values)
        }
        "stream_context_get_options" => {
            eval_builtin_stream_context_get_options(args, context, scope, values)
        }
        "stream_context_get_params" => {
            eval_builtin_stream_context_get_params(args, context, scope, values)
        }
        "stream_context_set_default" => {
            eval_builtin_stream_context_set_default(args, context, scope, values)
        }
        "stream_context_set_option" => {
            eval_builtin_stream_context_set_option(args, context, scope, values)
        }
        "stream_context_set_params" => {
            eval_builtin_stream_context_set_params(args, context, scope, values)
        }
        "stream_wrapper_register" | "stream_wrapper_unregister" | "stream_wrapper_restore" => {
            eval_builtin_stream_wrapper_registry(name, args, context, scope, values)
        }
        "stream_filter_register" => {
            eval_builtin_stream_filter_register(args, context, scope, values)
        }
        "stream_filter_append" | "stream_filter_prepend" => {
            eval_builtin_stream_filter_attach(name, args, context, scope, values)
        }
        "stream_filter_remove" => eval_builtin_stream_filter_remove(args, context, scope, values),
        "stream_bucket_new" => eval_builtin_stream_bucket_new(args, context, scope, values),
        "stream_bucket_make_writeable" => {
            eval_builtin_stream_bucket_make_writeable(args, context, scope, values)
        }
        "stream_bucket_append" | "stream_bucket_prepend" => {
            eval_builtin_stream_bucket_push(name, args, context, scope, values)
        }
        "stream_select" => eval_builtin_stream_select(args, context, scope, values),
        "stream_socket_server" => eval_builtin_stream_socket_server(args, context, scope, values),
        "stream_socket_client" => eval_builtin_stream_socket_client(args, context, scope, values),
        "stream_socket_accept" => eval_builtin_stream_socket_accept(args, context, scope, values),
        "stream_socket_get_name" => {
            eval_builtin_stream_socket_get_name(args, context, scope, values)
        }
        "stream_socket_shutdown" => {
            eval_builtin_stream_socket_shutdown(args, context, scope, values)
        }
        "stream_socket_enable_crypto" => {
            eval_builtin_stream_socket_enable_crypto(args, context, scope, values)
        }
        "stream_socket_sendto" => eval_builtin_stream_socket_sendto(args, context, scope, values),
        "stream_socket_recvfrom" => {
            eval_builtin_stream_socket_recvfrom(args, context, scope, values)
        }
        "stream_socket_pair" => eval_builtin_stream_socket_pair(args, context, scope, values),
        "stream_get_contents" => eval_builtin_stream_get_contents(args, context, scope, values),
        "stream_get_line" => eval_builtin_stream_get_line(args, context, scope, values),
        "stream_isatty" => eval_builtin_stream_isatty(args, context, scope, values),
        "stream_set_blocking" => eval_builtin_stream_set_blocking(args, context, scope, values),
        "stream_set_chunk_size" | "stream_set_read_buffer" | "stream_set_write_buffer" => {
            eval_builtin_stream_set_buffer_like(name, args, context, scope, values)
        }
        "stream_set_timeout" => eval_builtin_stream_set_timeout(args, context, scope, values),
        "strtotime" => eval_builtin_strtotime(args, context, scope, values),
        "unlink" => eval_builtin_unlink(args, context, scope, values),
        "strrev" => eval_builtin_strrev(args, context, scope, values),
        "str_replace" | "str_ireplace" => {
            eval_builtin_str_replace(name, args, context, scope, values)
        }
        "str_pad" => eval_builtin_str_pad(args, context, scope, values),
        "str_split" => eval_builtin_str_split(args, context, scope, values),
        "strstr" => eval_builtin_strstr(args, context, scope, values),
        "substr" => eval_builtin_substr(args, context, scope, values),
        "substr_replace" => eval_builtin_substr_replace(args, context, scope, values),
        "str_contains" | "str_starts_with" | "str_ends_with" => {
            eval_builtin_string_search(name, args, context, scope, values)
        }
        "strcmp" | "strcasecmp" => eval_builtin_string_compare(name, args, context, scope, values),
        "strlen" => eval_builtin_strlen(args, context, scope, values),
        "strpos" | "strrpos" => eval_builtin_string_position(name, args, context, scope, values),
        "lcfirst" | "strtolower" | "strtoupper" | "ucfirst" => {
            eval_builtin_string_case(name, args, context, scope, values)
        }
        "long2ip" => eval_builtin_long2ip(args, context, scope, values),
        "trim" => eval_builtin_trim_like(name, args, context, scope, values),
        "ucwords" => eval_builtin_ucwords(args, context, scope, values),
        "unset" => eval_builtin_unset(args, context, scope, values),
        "umask" => eval_builtin_umask(args, context, scope, values),
        "usleep" => eval_builtin_usleep(args, context, scope, values),
        "var_dump" => eval_builtin_var_dump(args, context, scope, values),
        "vfprintf" => eval_builtin_vfprintf(args, context, scope, values),
        "vsprintf" | "vprintf" => eval_builtin_vsprintf_like(name, args, context, scope, values),
        "wordwrap" => eval_builtin_wordwrap(args, context, scope, values),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}
