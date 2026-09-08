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
mod temporaries;
pub(in crate::interpreter) use temporaries::{
    eval_condition, record_scalar_argument_temporary,
    EvalExprResult, finish_evaluated_result,
};

pub(in crate::interpreter) use evaluation::{
    eval_array_access_object_matches, eval_array_get_result, eval_binary_result,
    eval_closure_object_expr, eval_dynamic_class_name, eval_dynamic_member_name, eval_match_expr,
};
use evaluation::*;

/// Evaluates one expression to an opaque runtime-cell handle.
pub(in crate::interpreter) fn eval_expr(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_expr_result(expr, context, scope, values).map(|result| result.value)
}

/// Evaluates once while retaining the result's proven ownership for borrowing consumers.
pub(in crate::interpreter) fn eval_expr_result(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalExprResult, EvalStatus> {
    let mut owned = temporaries::expression_owns_temporary(expr);
    let value = eval_expr_with_ownership(expr, context, scope, values, &mut owned)?;
    Ok(EvalExprResult { value, owned })
}

/// Dispatches an expression and updates ownership when the selected call supplies stronger evidence.
fn eval_expr_with_ownership(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
    owned: &mut bool,
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
        EvalExpr::Call { name, args } => {
            let result = eval_call(name, args, context, scope, values)?;
            if name.eq_ignore_ascii_case("eval") {
                *owned = true;
            }
            Ok(result)
        }
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
            let result = eval_method_expression_result(object, &method, args, context, scope, values)?;
            *owned = result.owned;
            Ok(result.value)
        }
        EvalExpr::DynamicNewObject { class_name, args } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            eval_with_method_call_args(args, context, scope, values, |evaluated, context, scope, values| {
                eval_new_object_result(&class_name, evaluated, context, scope, values)
            })
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
            eval_with_method_call_args(args, context, scope, values, |evaluated, context, scope, values| {
                eval_static_method_call_result_from_scope(
                    &class_name, &method, evaluated, scope, context, values,
                )
            })
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
            if let Some(value) = visible_scope_cell(context, scope, name) {
                Ok(value)
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
            eval_with_method_call_args(args, context, scope, values, |evaluated, context, scope, values| {
                let class_name = eval_new_object_class_name(class_name, context)?;
                eval_new_object_result(&class_name, evaluated, context, scope, values)
            })
        }
        EvalExpr::NewAnonymousClass { class, args } => {
            ensure_eval_anonymous_class_decl(class, context, scope, values)?;
            eval_with_method_call_args(args, context, scope, values, |evaluated, context, scope, values| {
                let class = context.class(class.name()).cloned().ok_or(EvalStatus::RuntimeFatal)?;
                eval_dynamic_class_new_object(&class, evaluated, context, scope, values)
            })
        }
        EvalExpr::StaticMethodCall {
            class_name,
            method,
            args,
        } => {
            eval_with_method_call_args(args, context, scope, values, |evaluated, context, scope, values| {
                eval_static_method_call_result_from_scope(
                    class_name, method, evaluated, scope, context, values,
                )
            })
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
            let result = eval_method_expression_result(object, method, args, context, scope, values)?;
            *owned = result.owned;
            Ok(result.value)
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
            let result = eval_method_expression_result(object, method, args, context, scope, values)?;
            *owned = result.owned;
            Ok(result.value)
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
            let result = eval_method_expression_result(object, &method, args, context, scope, values)?;
            *owned = result.owned;
            Ok(result.value)
        }
        EvalExpr::NullCoalesce { value, default } => {
            let value = if let EvalExpr::LoadVar(name) = value.as_ref() {
                match visible_scope_cell(context, scope, name) {
                    Some(value) => EvalExprResult::unclassified(value),
                    None => EvalExprResult { value: values.null()?, owned: true },
                }
            } else {
                eval_expr_result(value, context, scope, values)?
            };
            let is_null = match values.is_null(value.value) {
                Ok(is_null) => is_null,
                Err(status) => return finish_evaluated_result(value, Err(status), context, values),
            };
            if !is_null {
                *owned = value.owned;
                return Ok(value.value);
            }
            finish_evaluated_result(value, Ok(()), context, values)?;
            let result = eval_expr_result(default, context, scope, values)?;
            *owned = result.owned;
            Ok(result.value)
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
            let condition = eval_expr_result(condition, context, scope, values)?;
            let truthy = match values.truthy(condition.value) {
                Ok(truthy) => truthy,
                Err(status) => return finish_evaluated_result(condition, Err(status), context, values),
            };
            if truthy && then_branch.is_none() {
                *owned = condition.owned;
                return Ok(condition.value);
            }
            finish_evaluated_result(condition, Ok(()), context, values)?;
            let selected = if truthy { then_branch.as_ref().expect("full ternary branch") } else { else_branch };
            let result = eval_expr_result(selected, context, scope, values)?;
            *owned = result.owned;
            Ok(result.value)
        }
        EvalExpr::Unary { op, expr } => {
            if *op == EvalUnaryOp::Suppress {
                let previous = values.begin_error_suppression()?;
                let result = eval_expr_result(expr, context, scope, values);
                values.end_error_suppression(previous)?;
                let result = result?;
                *owned = result.owned;
                return Ok(result.value);
            }
            let operand = eval_expr_result(expr, context, scope, values)?;
            let value = operand.value;
            let result = (|| { match op {
                EvalUnaryOp::Suppress => unreachable!("silence is evaluated before its operand"),
                EvalUnaryOp::Plus => {
                    let zero = values.int(0)?;
                    let result = values.add(zero, value);
                    let cleanup = values.release(zero);
                    result.and_then(|value| cleanup.map(|()| value))
                }
                EvalUnaryOp::Negate => {
                    let zero = values.int(0)?;
                    let result = values.sub(zero, value);
                    let cleanup = values.release(zero);
                    result.and_then(|value| cleanup.map(|()| value))
                }
                EvalUnaryOp::LogicalNot => {
                    let truthy = values.truthy(value)?;
                    values.bool_value(!truthy)
                }
                EvalUnaryOp::BitNot => values.bit_not(value),
            } })();
            finish_evaluated_result(operand, result, context, values)
        }
        EvalExpr::Binary { op, left, right } => {
            if *op == EvalBinOp::LogicalAnd {
                if !eval_condition(left, context, scope, values)? {
                    return values.bool_value(false);
                }
                let truthy = eval_condition(right, context, scope, values)?;
                return values.bool_value(truthy);
            }
            if *op == EvalBinOp::LogicalOr {
                if eval_condition(left, context, scope, values)? {
                    return values.bool_value(true);
                }
                let truthy = eval_condition(right, context, scope, values)?;
                return values.bool_value(truthy);
            }
            let left_value = eval_expr_result(left, context, scope, values)?;
            let result = (|| {
                let right_value = eval_expr_result(right, context, scope, values)?;
                let result = eval_binary_result(*op, left_value.value, right_value.value, context, values);
                finish_evaluated_result(right_value, result, context, values)
            })();
            finish_evaluated_result(left_value, result, context, values)
        }
    }
}
