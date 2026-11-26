//! Purpose:
//! Checks that a top-level `const` initializer is a PHP constant expression, so a fragment
//! PHP refuses to compile is refused before any of its statements runs.
//!
//! Called from:
//! - `crate::lower::lower_const_decl()`.
//!
//! Key details:
//! - Follows `zend_compile_const_expr` for a global constant: literals, constant and
//!   class-constant fetches, operators, casts, arrays, dimension reads, `new` without argument
//!   unpacking or a positional argument after a named one, enum property reads, static
//!   closures without `use`, and first-class callables of functions and static methods.
//!   Calls, variables, assignments, `@`, `throw`, `print`, `instanceof`, `match` and
//!   `static::` are refused. So is a first-class callable on a string-valued callee
//!   (`("strlen")(...)`), which PHP allows but eval cannot create anywhere yet.
//! - PHP folds the operand that decides a ternary, `?:`, `??`, `&&` or `||` first, so a
//!   forbidden form in a branch the folded operand discards is accepted. `constant_folding`
//!   folds literal operands exactly; one that only a predefined constant would fold is not
//!   folded, so both branches are still checked.
//! - Undefined constants and `self::` outside a class fail when the statement runs, as in PHP.
//! - Property fetches are allowed here only on enum cases, which PHP checks when it evaluates
//!   the fetch. `mark_property_receivers` wraps each receiver so the interpreter raises PHP's
//!   `Error` for any other receiver, without affecting code the initializer calls.

use crate::eval_ir::{EvalArrayElement, EvalBinOp, EvalCallArg, EvalConst, EvalExpr};

use super::constant_folding::{Folded, fold};

/// Returns whether `expr` is a valid initializer for a global `const` statement.
pub(super) fn is_constant_expression(expr: &EvalExpr) -> bool {
    match expr {
        EvalExpr::Const(_) | EvalExpr::Magic(_) | EvalExpr::ConstFetch(_)
        | EvalExpr::NamespacedConstFetch { .. } | EvalExpr::FunctionCallable { .. } => true,
        EvalExpr::ClassConstantFetch { class_name, .. } | EvalExpr::ClassNameFetch { class_name } => {
            !is_static_receiver(class_name)
        }
        EvalExpr::StaticMethodCallable { class_name, method } => {
            !is_static_receiver(class_name) && matches!(method.as_ref(), EvalExpr::Const(EvalConst::String(_)))
        }
        EvalExpr::Array(elements) => elements.iter().all(|element| match element {
            EvalArrayElement::Value(value) => is_constant_expression(value),
            EvalArrayElement::KeyValue { key, value } => is_constant_expression(key) && is_constant_expression(value),
            EvalArrayElement::Reference(_) | EvalArrayElement::KeyReference { .. } => false,
        }),
        EvalExpr::ArrayGet { array, index } => is_constant_expression(array) && is_constant_expression(index),
        EvalExpr::NewObject { class_name, args } => !is_static_receiver(class_name) && constant_arguments(args),
        EvalExpr::PropertyGet { object, .. } | EvalExpr::NullsafePropertyGet { object, .. } => {
            is_constant_expression(object)
        }
        EvalExpr::DynamicPropertyGet { object, property }
        | EvalExpr::NullsafeDynamicPropertyGet { object, property } => {
            is_constant_expression(object) && is_constant_expression(property)
        }
        EvalExpr::Closure { captures, is_static, .. } => *is_static && captures.is_empty(),
        EvalExpr::Cast { expr, .. } | EvalExpr::Unary { expr, .. } => is_constant_expression(expr),
        EvalExpr::Binary { op: op @ (EvalBinOp::LogicalAnd | EvalBinOp::LogicalOr), left, right } => {
            let decided = fold(left).map(|left| left.truthy()) == Some(*op == EvalBinOp::LogicalOr);
            is_constant_expression(left) && (decided || is_constant_expression(right))
        }
        EvalExpr::Binary { left, right, .. } => is_constant_expression(left) && is_constant_expression(right),
        EvalExpr::NullCoalesce { value, default } => {
            let decided = fold(value).is_some_and(|value| value != Folded::Null);
            is_constant_expression(value) && (decided || is_constant_expression(default))
        }
        EvalExpr::Ternary { condition, then_branch, else_branch } => {
            is_constant_expression(condition)
                && match fold(condition).map(|condition| condition.truthy()) {
                    Some(true) => then_branch.as_deref().is_none_or(is_constant_expression),
                    Some(false) => is_constant_expression(else_branch),
                    None => {
                        then_branch.as_deref().is_none_or(is_constant_expression)
                            && is_constant_expression(else_branch)
                    }
                }
        }
        _ => false,
    }
}

/// Wraps every property-fetch receiver of a validated initializer in the enum-case check.
///
/// Only the node kinds `is_constant_expression` accepts are traversed; a static closure's body
/// is ordinary code and is left alone.
pub(super) fn mark_property_receivers(expr: EvalExpr) -> EvalExpr {
    let mark = |expr: Box<EvalExpr>| Box::new(mark_property_receivers(*expr));
    let receiver = |object: Box<EvalExpr>, nullsafe: bool| {
        Box::new(EvalExpr::ConstantPropertyReceiver { object: Box::new(mark_property_receivers(*object)), nullsafe })
    };
    match expr {
        EvalExpr::PropertyGet { object, property } => EvalExpr::PropertyGet { object: receiver(object, false), property },
        EvalExpr::NullsafePropertyGet { object, property } => {
            EvalExpr::NullsafePropertyGet { object: receiver(object, true), property }
        }
        EvalExpr::DynamicPropertyGet { object, property } => {
            EvalExpr::DynamicPropertyGet { object: receiver(object, false), property: mark(property) }
        }
        EvalExpr::NullsafeDynamicPropertyGet { object, property } => {
            EvalExpr::NullsafeDynamicPropertyGet { object: receiver(object, true), property: mark(property) }
        }
        EvalExpr::Array(elements) => EvalExpr::Array(elements.into_iter().map(|element| match element {
            EvalArrayElement::Value(value) => EvalArrayElement::Value(mark_property_receivers(value)),
            EvalArrayElement::KeyValue { key, value } => EvalArrayElement::KeyValue {
                key: mark_property_receivers(key),
                value: mark_property_receivers(value),
            },
            element => element,
        }).collect()),
        EvalExpr::ArrayGet { array, index } => EvalExpr::ArrayGet { array: mark(array), index: mark(index) },
        EvalExpr::NewObject { class_name, args } => EvalExpr::NewObject {
            class_name,
            args: args.into_iter().map(|arg| {
                let value = mark_property_receivers(arg.value().clone());
                match arg.name() {
                    Some(name) => EvalCallArg::named(name, value),
                    None => EvalCallArg::positional(value),
                }
            }).collect(),
        },
        EvalExpr::Cast { target, expr } => EvalExpr::Cast { target, expr: mark(expr) },
        EvalExpr::Unary { op, expr } => EvalExpr::Unary { op, expr: mark(expr) },
        EvalExpr::Binary { op, left, right } => EvalExpr::Binary { op, left: mark(left), right: mark(right) },
        EvalExpr::NullCoalesce { value, default } => EvalExpr::NullCoalesce { value: mark(value), default: mark(default) },
        EvalExpr::Ternary { condition, then_branch, else_branch } => EvalExpr::Ternary {
            condition: mark(condition),
            then_branch: then_branch.map(mark),
            else_branch: mark(else_branch),
        },
        expr => expr,
    }
}

/// Recognizes the late-static receiver, which PHP refuses in compile-time constants.
fn is_static_receiver(class_name: &str) -> bool {
    class_name.eq_ignore_ascii_case("static")
}

/// Checks `new` arguments: constant values, no unpacking, no positional after a named one.
fn constant_arguments(args: &[EvalCallArg]) -> bool {
    let mut named = false;
    args.iter().all(|arg| {
        if arg.name().is_none() && named {
            return false;
        }
        named |= arg.name().is_some();
        !arg.is_spread() && is_constant_expression(arg.value())
    })
}
