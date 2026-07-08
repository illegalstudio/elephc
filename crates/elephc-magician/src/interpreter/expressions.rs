//! Purpose:
//! Evaluates EvalIR expressions, match expressions, and function-like calls.
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

/// Evaluates registry-backed direct builtins and language constructs after positional-only validation.
pub(in crate::interpreter) fn eval_positional_expr_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if name == "eval" {
        return eval_nested_eval(args, context, scope, values);
    }

    if let Some(result) = eval_declared_builtin_direct_call(name, args, context, scope, values)? {
        return Ok(result);
    }

    Err(EvalStatus::UnsupportedConstruct)
}
