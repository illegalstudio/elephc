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

mod calls;

pub(in crate::interpreter) use calls::*;
mod evaluation;
mod owned;

use owned::*;
pub(in crate::interpreter) use owned::eval_output_expr;

pub(in crate::interpreter) use evaluation::{
    eval_array_access_object_matches, eval_array_get_result, eval_binary_result,
    eval_dynamic_class_name, eval_dynamic_member_name, eval_match_expr,
};
use evaluation::*;

/// Evaluates one expression to an opaque runtime-cell handle.
pub(in crate::interpreter) fn eval_expr(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_expr_with_result_ownership(expr, context, scope, values, false)
}

/// Evaluates a value that can be stored or returned independently of the current scope.
pub(in crate::interpreter) fn eval_owned_expr(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_expr_with_result_ownership(expr, context, scope, values, true)
}

/// Preserves expression evaluation order while optionally owning the selected result.
fn eval_expr_with_result_ownership(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
    own_result: bool,
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
            if own_result {
                eval_owned_array_index_read(array, index, context, scope, values)
            } else {
                let array = eval_expr(array, context, scope, values)?;
                let index = eval_expr(index, context, scope, values)?;
                eval_array_get_result(array, index, context, values)
            }
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
            eval_instance_method_call(object, &method, args, context, scope, values)
        }
        EvalExpr::DynamicNewObject { class_name, args } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            with_literal_call_arguments(args, context, scope, values, |args, context, scope, values| {
                eval_new_object_result(&class_name, args, context, scope, values)
            })
        }
        EvalExpr::DynamicPropertyGet { object, property } => {
            with_owned_receiver(object, context, scope, values, |object, context, scope, values| {
                let property = eval_dynamic_member_name(property, context, scope, values)?;
                eval_property_get_with_result_ownership(object, &property, context, values, true)
            })
        }
        EvalExpr::DynamicStaticMethodCall {
            class_name,
            method,
            args,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            let method = eval_dynamic_member_name(method, context, scope, values)?;
            eval_static_method_call(&class_name, &method, args, context, scope, values)
        }
        EvalExpr::DynamicStaticPropertyGet {
            class_name,
            property,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            eval_static_property_get_with_result_ownership(&class_name, property, context, values, own_result)
        }
        EvalExpr::DynamicStaticPropertyNameGet {
            class_name,
            property,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            let property = eval_dynamic_member_name(property, context, scope, values)?;
            eval_static_property_get_with_result_ownership(&class_name, &property, context, values, own_result)
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
            if let Some(value) = visible_scope_cell(context, scope, name) {
                if own_result { copy_scope_value(value, context, values) } else { Ok(value) }
            } else {
                values.warning(&format!("Warning: Undefined variable ${name}\n"))?;
                values.null()
            }
        }
        EvalExpr::Magic(magic) => eval_magic_const(magic, context, values),
        EvalExpr::Match {
            subject,
            arms,
            default,
        } => eval_match_expr(subject, arms, default.as_deref(), context, scope, values, own_result),
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
            with_literal_call_arguments(args, context, scope, values, |args, context, scope, values| {
                let class_name = eval_new_object_class_name(class_name, context)?;
                eval_new_object_result(&class_name, args, context, scope, values)
            })
        }
        EvalExpr::NewAnonymousClass { class, args } => {
            ensure_eval_anonymous_class_decl(class, context, scope, values)?;
            with_literal_call_arguments(args, context, scope, values, |args, context, scope, values| {
                let class = context.class(class.name()).cloned().ok_or(EvalStatus::RuntimeFatal)?;
                eval_dynamic_class_new_object(&class, args, context, scope, values)
            })
        }
        EvalExpr::StaticMethodCall {
            class_name,
            method,
            args,
        } => {
            eval_static_method_call(class_name, method, args, context, scope, values)
        }
        EvalExpr::StaticPropertyGet {
            class_name,
            property,
        } => eval_static_property_get_with_result_ownership(class_name, property, context, values, own_result),
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
            eval_instance_method_call(object, method, args, context, scope, values)
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
            eval_instance_method_call(object, method, args, context, scope, values)
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
            eval_instance_method_call(object, &method, args, context, scope, values)
        }
        EvalExpr::NullCoalesce { value, default } => {
            let value = if let EvalExpr::LoadVar(name) = value.as_ref() {
                match visible_scope_cell(context, scope, name) {
                    Some(value) if own_result => copy_scope_value(value, context, values)?,
                    Some(value) => value,
                    None => values.null()?,
                }
            } else {
                eval_expr_with_result_ownership(value, context, scope, values, own_result)?
            };
            if values.is_null(value)? {
                if own_result { eval_release_value(context, values, value)?; }
                eval_expr_with_result_ownership(default, context, scope, values, own_result)
            } else {
                Ok(value)
            }
        }
        EvalExpr::NullsafePropertyGet { object, property } => {
            with_owned_receiver(object, context, scope, values, |object, context, _scope, values| {
                if values.is_null(object)? { return values.null(); }
                eval_property_get_with_result_ownership(object, property, context, values, true)
            })
        }
        EvalExpr::NullsafeDynamicPropertyGet { object, property } => {
            with_owned_receiver(object, context, scope, values, |object, context, scope, values| {
                if values.is_null(object)? { return values.null(); }
                let property = eval_dynamic_member_name(property, context, scope, values)?;
                eval_property_get_with_result_ownership(object, &property, context, values, true)
            })
        }
        EvalExpr::PropertyGet { object, property } => {
            with_owned_receiver(object, context, scope, values, |object, context, _scope, values| {
                eval_property_get_with_result_ownership(object, property, context, values, true)
            })
        }
        EvalExpr::Print(inner) => {
            eval_output_expr(inner, context, scope, values)?;
            values.int(1)
        }
        EvalExpr::Ternary {
            condition,
            then_branch,
            else_branch,
        } => {
            let own_condition = own_result;
            let condition = eval_expr_with_result_ownership(condition, context, scope, values, own_condition)?;
            let truthy = match values.truthy(condition) {
                Ok(truthy) => truthy,
                Err(status) => {
                    if own_condition { let _ = eval_release_value(context, values, condition); }
                    return Err(status);
                }
            };
            if truthy {
                if let Some(then_branch) = then_branch {
                    if own_condition { eval_release_value(context, values, condition)?; }
                    eval_expr_with_result_ownership(then_branch, context, scope, values, own_result)
                } else {
                    Ok(condition)
                }
            } else {
                if own_condition { eval_release_value(context, values, condition)?; }
                eval_expr_with_result_ownership(else_branch, context, scope, values, own_result)
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
                    if values.type_tag(value)? == EVAL_TAG_FLOAT {
                        let bits = values.raw_value_word(value)? ^ (1_u64 << 63);
                        return values.raw_word_value(EVAL_TAG_FLOAT, bits);
                    }
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
            if own_result && *op == EvalBinOp::Concat {
                return eval_owned_concat(left, right, context, scope, values);
            }
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
