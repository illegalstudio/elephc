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
//!   unpacking, enum property reads, static closures without `use`, and first-class callables
//!   of functions and static methods. Calls, variables, assignments, `@`, `throw`, `print`,
//!   `instanceof`, `match` and `static::` are refused.
//! - PHP folds the operand that decides a ternary, `?:`, `??`, `&&` or `||` first, so a
//!   forbidden form in a branch a literal discards is accepted. A deciding operand that only a
//!   predefined constant would fold is not folded here, so both branches are still checked.
//! - Undefined constants and `self::` outside a class fail when the statement runs, as in PHP.

use crate::eval_ir::{EvalArrayElement, EvalBinOp, EvalConst, EvalExpr};

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
        EvalExpr::NewObject { class_name, args } => {
            !is_static_receiver(class_name)
                && args.iter().all(|arg| !arg.is_spread() && is_constant_expression(arg.value()))
        }
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
            let decided = literal_truthiness(left) == Some(*op == EvalBinOp::LogicalOr);
            is_constant_expression(left) && (decided || is_constant_expression(right))
        }
        EvalExpr::Binary { left, right, .. } => is_constant_expression(left) && is_constant_expression(right),
        EvalExpr::NullCoalesce { value, default } => {
            let decided = matches!(value.as_ref(), EvalExpr::Const(constant) if *constant != EvalConst::Null);
            is_constant_expression(value) && (decided || is_constant_expression(default))
        }
        EvalExpr::Ternary { condition, then_branch, else_branch } => {
            is_constant_expression(condition)
                && match literal_truthiness(condition) {
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

/// Recognizes the late-static receiver, which PHP refuses in compile-time constants.
fn is_static_receiver(class_name: &str) -> bool {
    class_name.eq_ignore_ascii_case("static")
}

/// Returns the PHP truthiness of a literal operand, or `None` when it is not a literal.
fn literal_truthiness(expr: &EvalExpr) -> Option<bool> {
    let EvalExpr::Const(constant) = expr else { return None };
    Some(match constant {
        EvalConst::Null => false,
        EvalConst::Bool(value) => *value,
        EvalConst::Int(value) => *value != 0,
        EvalConst::Float(value) => *value != 0.0,
        EvalConst::String(value) => !(value.is_empty() || value == "0"),
        EvalConst::Bytes(value) => !(value.is_empty() || value.as_slice() == b"0"),
    })
}
