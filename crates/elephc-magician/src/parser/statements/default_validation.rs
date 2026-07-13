//! Purpose:
//! Validates whether parameter and property defaults can be materialized safely.
//!
//! Called from:
//! - Parameter, method, and property declaration parsing.
//!
//! Key details:
//! - Constant expressions, arrays, calls, and class receivers are accepted conservatively.

use super::*;

/// Returns whether an eval method parameter default can be materialized safely.
pub(super) fn method_parameter_default_is_supported(default: &EvalExpr) -> bool {
    eval_constant_expression_default_is_supported(default)
}

/// Returns whether an EvalIR expression is safe to retain as a method default.
pub(super) fn eval_constant_expression_default_is_supported(expr: &EvalExpr) -> bool {
    match expr {
        EvalExpr::Array(elements) => elements.iter().all(eval_array_element_default_is_supported),
        EvalExpr::Const(_) => true,
        EvalExpr::Magic(_) => true,
        EvalExpr::ConstFetch(_) | EvalExpr::NamespacedConstFetch { .. } => true,
        EvalExpr::ClassConstantFetch { class_name, .. }
        | EvalExpr::ClassNameFetch { class_name } => {
            eval_default_class_receiver_is_supported(class_name)
        }
        EvalExpr::NewObject { class_name, args } => {
            eval_default_class_receiver_is_supported(class_name)
                && args.iter().all(eval_call_arg_default_is_supported)
        }
        EvalExpr::NewAnonymousClass { .. } => false,
        EvalExpr::NullCoalesce { value, default } => {
            eval_constant_expression_default_is_supported(value)
                && eval_constant_expression_default_is_supported(default)
        }
        EvalExpr::Ternary {
            condition,
            then_branch,
            else_branch,
        } => {
            eval_constant_expression_default_is_supported(condition)
                && then_branch
                    .as_deref()
                    .is_none_or(eval_constant_expression_default_is_supported)
                && eval_constant_expression_default_is_supported(else_branch)
        }
        EvalExpr::Cast { expr, .. } => eval_constant_expression_default_is_supported(expr),
        EvalExpr::Unary { expr, .. } => eval_constant_expression_default_is_supported(expr),
        EvalExpr::Binary { left, right, .. } => {
            eval_constant_expression_default_is_supported(left)
                && eval_constant_expression_default_is_supported(right)
        }
        _ => false,
    }
}

/// Returns whether one object-construction argument is safe inside a method default.
pub(super) fn eval_call_arg_default_is_supported(arg: &EvalCallArg) -> bool {
    !arg.is_spread() && eval_constant_expression_default_is_supported(arg.value())
}

/// Returns whether one array default element contains only supported constant expressions.
pub(super) fn eval_array_element_default_is_supported(element: &EvalArrayElement) -> bool {
    match element {
        EvalArrayElement::Value(value) => eval_constant_expression_default_is_supported(value),
        EvalArrayElement::Reference(_) => false,
        EvalArrayElement::KeyValue { key, value } => {
            eval_constant_expression_default_is_supported(key)
                && eval_constant_expression_default_is_supported(value)
        }
        EvalArrayElement::KeyReference { .. } => false,
    }
}

/// Returns whether a type list contains return-only standalone atoms.
pub(super) fn type_variants_contain_standalone_return_only_atoms(
    variants: &[EvalParameterTypeVariant],
) -> bool {
    variants.iter().any(|variant| {
        matches!(
            variant,
            EvalParameterTypeVariant::Never | EvalParameterTypeVariant::Void
        )
    })
}

/// Returns whether the type position accepts standalone return-only atoms.
pub(super) fn type_position_allows_return_only_atoms(position: EvalTypePosition) -> bool {
    matches!(
        position,
        EvalTypePosition::FunctionReturn | EvalTypePosition::MethodReturn
    )
}

/// Returns whether `self` and `parent` are legal in this type position.
pub(super) fn type_position_allows_class_scope_atoms(position: EvalTypePosition) -> bool {
    !matches!(
        position,
        EvalTypePosition::FunctionParameter | EvalTypePosition::FunctionReturn
    )
}

/// Returns whether a class-like receiver is legal in a compile-time method default.
pub(super) fn eval_default_class_receiver_is_supported(class_name: &str) -> bool {
    !class_name
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("static")
}
