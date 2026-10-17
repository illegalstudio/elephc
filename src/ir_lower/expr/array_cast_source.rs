//! Purpose:
//! Predicts the PHP type of an expression used as the source of an `(array)` cast inside
//! an array literal, matching the type `lower_array_cast` reads off the lowered value.
//!
//! Called from:
//! - `array_cast_literal_element_type` in indexed-array literal typing, which associative
//!   literals share.
//!
//! Key details:
//! - Every cast goes through `cast_php_type`, including nested `(array)` and `(object)`.
//! - `null` is `Void` and a variable is `local_type`. Those are the types the load carries.
//! - A kind whose only syntactic answer is `Int` is `Mixed`, so the cast takes the boxed
//!   dynamic path instead of stamping `array<int>` over a property hash or another scalar.

use crate::ir_lower::context::{value_ir_type, LoweringContext};
use crate::parser::ast::{BinOp, CallableTarget, Expr, ExprKind, MagicConstant};
use crate::types::PhpType;

use super::assoc_array_literals::{
    assoc_array_literal_type_for_ir, scoped_constant_value_type_for_ir,
};
use super::indexed_array_literals::{
    array_literal_type_for_ir, assoc_array_literal_type_from_entries, builtin_call_result_type_for_ir,
    ir_array_storage_type,
};
use super::lazy_branches::{strip_void_from_union, wider_type_for_merge};
use super::merge_temps::{
    branch_merge_result_type, fallback_expr_type, match_merge_result_type,
    short_ternary_merge_result_type,
};
use super::numeric_binary::{
    array_union_value_type, assoc_union_key_type, indexed_array_union_element_type,
    numeric_binary_identity, numeric_binary_result_type,
};
use super::unary_logic::int_unary_result_type;
use super::{
    array_access_expr_value_type_for_ir, eir_user_function_return_type, method_call_expr_type_for_ir,
    property_access_expr_type_for_ir, static_method_call_expr_type_for_ir,
    static_property_result_type,
};

/// Returns the PHP type `ArrayCast` should classify for `expr`.
///
/// This is the type `lower_array_cast` reads off the lowered inner value, not the storage
/// type an array element of that value would use. The literal `null` is the split: as an
/// element it is stored as `Mixed`, but `(array) null` is an empty array.
pub(super) fn array_cast_source_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    expr: &Expr,
) -> PhpType {
    if let Some(storage) = super::nullsafe_chain::result_storage_type(expr) {
        return storage;
    }
    match &expr.kind {
        ExprKind::Null => PhpType::Void,
        ExprKind::Variable(name) => ctx.local_type(name),
        ExprKind::Cast { target, expr: inner } => super::ternary_cast::cast_php_type(
            target,
            &array_cast_source_type_for_ir(ctx, inner),
        ),
        ExprKind::ErrorSuppress(inner) => array_cast_source_type_for_ir(ctx, inner),
        ExprKind::Clone(inner) => clone_source_type(ctx, inner),
        ExprKind::Assignment { value, .. } => array_cast_source_type_for_ir(ctx, value),
        ExprKind::Negate(inner) => negate_source_type(ctx, inner),
        ExprKind::Not(_) => PhpType::Bool,
        ExprKind::BitNot(inner) => bitnot_source_type(ctx, inner),
        ExprKind::Print(_) => PhpType::Int,
        ExprKind::Throw(_) => PhpType::Void,
        ExprKind::InstanceOf { .. } => PhpType::Bool,
        ExprKind::StringLiteral(_) => PhpType::Str,
        ExprKind::IntLiteral(_) => PhpType::Int,
        ExprKind::FloatLiteral(_) => PhpType::Float,
        ExprKind::BoolLiteral(_) => PhpType::Bool,
        ExprKind::BinaryOp { left, op, right } => binary_source_type(ctx, left, op, right, expr),
        ExprKind::NullCoalesce { value, default } => null_coalesce_source_type(ctx, value, default),
        ExprKind::ShortTernary { value, default } => {
            short_ternary_merge_result_type(ctx, value, default)
        }
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => branch_merge_result_type(ctx, then_expr, else_expr, expr),
        ExprKind::Match { arms, default, .. } => {
            match_merge_result_type(ctx, arms, default.as_deref(), expr)
        }
        ExprKind::ArrayLiteral(items) => array_literal_type_for_ir(ctx, items, expr),
        ExprKind::ArrayLiteralAssoc(pairs) => assoc_array_literal_type_for_ir(ctx, pairs, expr),
        ExprKind::ArrayLiteralMixed(entries) => {
            assoc_array_literal_type_from_entries(ctx, entries, expr)
        }
        ExprKind::NewObject { class_name, .. } => PhpType::Object(class_name.as_str().to_string()),
        ExprKind::NewDynamic { .. } => PhpType::Mixed,
        ExprKind::NewDynamicObject { fallback_class, .. } => {
            PhpType::Object(fallback_class.as_str().to_string())
        }
        ExprKind::NewScopedObject { .. } => PhpType::Object(String::new()),
        ExprKind::This => PhpType::Object(ctx.current_class.clone().unwrap_or_default()),
        ExprKind::ClassConstant { .. } | ExprKind::ObjectClassName { .. } => PhpType::Str,
        ExprKind::MagicConstant(constant) => magic_constant_source_type(constant),
        ExprKind::PropertyAccess { object, property } => {
            property_access_expr_type_for_ir(ctx, object, property).unwrap_or(PhpType::Mixed)
        }
        ExprKind::DynamicPropertyAccess { .. } => PhpType::Mixed,
        ExprKind::StaticPropertyAccess { receiver, property } => {
            static_property_result_type(ctx, receiver, property, expr)
        }
        ExprKind::MethodCall { object, method, .. } => {
            method_call_expr_type_for_ir(ctx, object, method).unwrap_or(PhpType::Mixed)
        }
        ExprKind::StaticMethodCall {
            receiver, method, ..
        } => static_method_call_expr_type_for_ir(ctx, receiver, method).unwrap_or(PhpType::Mixed),
        ExprKind::FunctionCall { name, .. } => function_call_source_type(ctx, name.as_str(), expr),
        ExprKind::ClosureCall { var, .. } => callable_var_source_type(ctx, var),
        ExprKind::Pipe { callable, .. } => pipe_source_type(ctx, callable),
        ExprKind::ExprCall { callee, .. } => expr_call_source_type(ctx, callee),
        ExprKind::PreIncrement(name) | ExprKind::PreDecrement(name) => {
            inc_dec_source_type(ctx, name, false)
        }
        ExprKind::PostIncrement(name) | ExprKind::PostDecrement(name) => {
            inc_dec_source_type(ctx, name, true)
        }
        ExprKind::ConstRef(name) => ctx
            .constant_value(name.as_str())
            .map(|(_, ty)| ir_array_storage_type(ty))
            .unwrap_or(PhpType::Mixed),
        ExprKind::ScopedConstantAccess { receiver, name } => {
            scoped_constant_value_type_for_ir(ctx, receiver, name, expr)
        }
        ExprKind::ArrayAccess { array, .. } => {
            array_access_expr_value_type_for_ir(ctx, array).unwrap_or(PhpType::Mixed)
        }
        ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_) => PhpType::Callable,
        ExprKind::PtrCast { target_type, .. } => PhpType::Pointer(Some(target_type.clone())),
        _ => PhpType::Mixed,
    }
}

/// Returns the type of `clone`, which keeps an object's class and boxes anything else.
fn clone_source_type(ctx: &LoweringContext<'_, '_>, inner: &Expr) -> PhpType {
    match array_cast_source_type_for_ir(ctx, inner).codegen_repr() {
        PhpType::Object(class_name) => PhpType::Object(class_name),
        _ => PhpType::Mixed,
    }
}

/// Returns the type of numeric negation.
///
/// A float stays a float and an int or bool stays an int. Every other operand goes through
/// the mixed numeric helper, whose result is boxed.
fn negate_source_type(ctx: &LoweringContext<'_, '_>, inner: &Expr) -> PhpType {
    match array_cast_source_type_for_ir(ctx, inner).codegen_repr() {
        PhpType::Float => PhpType::Float,
        PhpType::Int | PhpType::Bool => PhpType::Int,
        _ => PhpType::Mixed,
    }
}

/// Returns the type of `~`, using the same rule as `lower_int_unary`.
///
/// An int or bool is an `i64` complement. A float is not coerced to int here, so the result
/// is the mixed runtime call rather than `int`.
fn bitnot_source_type(ctx: &LoweringContext<'_, '_>, inner: &Expr) -> PhpType {
    int_unary_result_type(value_ir_type(&array_cast_source_type_for_ir(ctx, inner)))
}

/// Returns the type of a binary operator from the operand types lowering will see.
fn binary_source_type(
    ctx: &LoweringContext<'_, '_>,
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    expr: &Expr,
) -> PhpType {
    match op {
        BinOp::Concat => PhpType::Str,
        BinOp::Eq
        | BinOp::NotEq
        | BinOp::StrictEq
        | BinOp::StrictNotEq
        | BinOp::Lt
        | BinOp::Gt
        | BinOp::LtEq
        | BinOp::GtEq
        | BinOp::And
        | BinOp::Or
        | BinOp::Xor => PhpType::Bool,
        BinOp::Spaceship => PhpType::Int,
        BinOp::Add => array_add_source_type(ctx, left, right, expr),
        BinOp::Sub
        | BinOp::Mul
        | BinOp::Div
        | BinOp::Mod
        | BinOp::Pow
        | BinOp::BitAnd
        | BinOp::BitOr
        | BinOp::BitXor
        | BinOp::ShiftLeft
        | BinOp::ShiftRight => numeric_binary_php_type(ctx, left, op, right, expr),
        BinOp::NullCoalesce => PhpType::Mixed,
    }
}

/// Returns the type of `+`, including array union when both operands are arrays.
fn array_add_source_type(
    ctx: &LoweringContext<'_, '_>,
    left: &Expr,
    right: &Expr,
    expr: &Expr,
) -> PhpType {
    let left_ty = array_cast_source_type_for_ir(ctx, left).codegen_repr();
    let right_ty = array_cast_source_type_for_ir(ctx, right).codegen_repr();
    let united = match (&left_ty, &right_ty) {
        (PhpType::Array(left_elem), PhpType::Array(right_elem)) => {
            indexed_array_union_element_type(left_elem, right_elem)
                .map(|elem_ty| PhpType::Array(Box::new(elem_ty)))
        }
        (
            PhpType::AssocArray {
                key: left_key,
                value: left_value,
            },
            PhpType::AssocArray {
                key: right_key,
                value: right_value,
            },
        ) => Some(PhpType::AssocArray {
            key: Box::new(assoc_union_key_type(left_key, right_key)),
            value: Box::new(array_union_value_type(left_value, right_value)),
        }),
        (PhpType::Array(left_elem), PhpType::AssocArray { key, value }) => {
            Some(PhpType::AssocArray {
                key: Box::new(crate::types::merge_array_key_types(
                    PhpType::Int,
                    key.codegen_repr(),
                )),
                value: Box::new(array_union_value_type(left_elem, value)),
            })
        }
        (PhpType::AssocArray { key, value }, PhpType::Array(right_elem)) => {
            Some(PhpType::AssocArray {
                key: Box::new(crate::types::merge_array_key_types(
                    key.codegen_repr(),
                    PhpType::Int,
                )),
                value: Box::new(array_union_value_type(value, right_elem)),
            })
        }
        _ => None,
    };
    united.unwrap_or_else(|| numeric_binary_php_type(ctx, left, &BinOp::Add, right, expr))
}

/// Returns the PHP type `lower_numeric_binary` stamps, from the operand types it will see.
///
/// `%` and the bitwise operators are decided before the float check, so a float operand still
/// produces `int`. `/` and `**` follow the same helper, including float division of two ints.
fn numeric_binary_php_type(
    ctx: &LoweringContext<'_, '_>,
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    expr: &Expr,
) -> PhpType {
    let lhs = value_ir_type(&array_cast_source_type_for_ir(ctx, left));
    let rhs = value_ir_type(&array_cast_source_type_for_ir(ctx, right));
    numeric_binary_result_type(
        op,
        lhs,
        rhs,
        &fallback_expr_type(expr),
        numeric_binary_identity(op, left, right),
    )
}

/// Returns the type of `??` after the null arm is removed.
fn null_coalesce_source_type(
    ctx: &LoweringContext<'_, '_>,
    value: &Expr,
    default: &Expr,
) -> PhpType {
    let value_ty = strip_void_from_union(array_cast_source_type_for_ir(ctx, value)).codegen_repr();
    let default_ty = array_cast_source_type_for_ir(ctx, default).codegen_repr();
    wider_type_for_merge(&value_ty, &default_ty)
}

/// Returns the type of a direct, extern, or builtin call.
///
/// An unknown callee is `Mixed`. The syntactic call fallback answers `int` for every name
/// outside its allowlist, which would wrap that guess instead of the real return value.
fn function_call_source_type(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    expr: &Expr,
) -> PhpType {
    if let Some(sig) = ctx.functions.get(name) {
        return ir_array_storage_type(eir_user_function_return_type(sig));
    }
    if let Some(sig) = ctx.extern_functions.get(name) {
        return ir_array_storage_type(sig.return_type.clone());
    }
    builtin_call_result_type_for_ir(ctx, expr.span).unwrap_or(PhpType::Mixed)
}

/// Returns the type of `$fn(...)` from the callable signature recorded for that local.
fn callable_var_source_type(ctx: &LoweringContext<'_, '_>, var: &str) -> PhpType {
    let signature = ctx.callable_param_signature(var).cloned().or_else(|| {
        ctx.static_callable_local(var)
            .and_then(|target| super::descriptor_invoke::signature_for_static_callable_binding(ctx, target))
    });
    super::descriptor_invoke::descriptor_invoker_result_type(signature.as_ref())
}

/// Returns the type of `value |> callable`.
fn pipe_source_type(ctx: &LoweringContext<'_, '_>, callable: &Expr) -> PhpType {
    match &callable.kind {
        ExprKind::Variable(name) => callable_var_source_type(ctx, name),
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => {
            function_call_source_type(ctx, name.as_str(), callable)
        }
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method }) => {
            static_method_call_expr_type_for_ir(ctx, receiver, method).unwrap_or(PhpType::Mixed)
        }
        ExprKind::FirstClassCallable(CallableTarget::Method { object, method }) => {
            method_call_expr_type_for_ir(ctx, object, method).unwrap_or(PhpType::Mixed)
        }
        _ => PhpType::Mixed,
    }
}

/// Returns the type of `expr(...)`.
fn expr_call_source_type(ctx: &LoweringContext<'_, '_>, callee: &Expr) -> PhpType {
    match &callee.kind {
        ExprKind::Variable(name) => callable_var_source_type(ctx, name),
        _ => {
            let signature =
                super::descriptor_invoke::callable_descriptor_signature_for_expr(ctx, callee);
            super::descriptor_invoke::descriptor_invoker_result_type(signature.as_ref())
        }
    }
}

/// Returns the type of `++`/`--` on a local, including the value the post-form gives back.
///
/// A float stays a float. A pre-increment of an int or a string is boxed, because overflow
/// and numeric-string increment can change the type. The post-form returns the previous value.
fn inc_dec_source_type(ctx: &LoweringContext<'_, '_>, name: &str, post: bool) -> PhpType {
    match ctx.local_type(name).codegen_repr() {
        PhpType::Float => PhpType::Float,
        PhpType::Str => {
            if post {
                PhpType::Str
            } else {
                PhpType::Mixed
            }
        }
        PhpType::Mixed => PhpType::Mixed,
        PhpType::Int => {
            if post {
                PhpType::Int
            } else {
                PhpType::Mixed
            }
        }
        PhpType::Bool => {
            if post {
                PhpType::Bool
            } else {
                PhpType::Int
            }
        }
        _ => PhpType::Mixed,
    }
}

/// Returns the type of a magic constant that survived until EIR lowering.
///
/// `__LINE__` is not a `MagicConstant` variant. The remaining constants are strings, and the
/// magic-constant pass normally rewrites them to literals before this point.
fn magic_constant_source_type(constant: &MagicConstant) -> PhpType {
    match constant {
        MagicConstant::Dir
        | MagicConstant::File
        | MagicConstant::Function
        | MagicConstant::Class
        | MagicConstant::Method
        | MagicConstant::Namespace
        | MagicConstant::Trait => PhpType::Str,
    }
}
