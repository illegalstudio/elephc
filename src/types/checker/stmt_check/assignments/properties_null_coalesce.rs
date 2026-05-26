//! Purpose:
//! Type-checks assignment properties null coalesce forms.
//! Updates type environments and validates storage-specific rules for locals, arrays, and properties.
//!
//! Called from:
//! - `crate::types::checker::stmt_check::assignments`
//!
//! Key details:
//! - Assignment checking must distinguish value writes, by-reference mutation, nullable access, and declared property contracts.

use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

use super::super::super::Checker;

/// Returns `true` if a null-coalescing assignment to a property preserves the non-null type
/// of the property after the assignment.
///
/// This is used to determine whether a pattern like `$obj->prop ??= $val` can be considered
/// a non-null-preserving write. Returns `false` when the property type already includes null,
/// or when the right-hand side is not a null-coalesce expression targeting the same property.
///
/// Arguments:
/// - `object`: The object expression on the left side of the assignment
/// - `property`: The property name being assigned
/// - `value`: The full right-hand side expression
/// - `property_ty`: The declared or inferred type of the property before assignment
pub(super) fn null_coalesce_property_keeps_non_null(
    object: &Expr,
    property: &str,
    value: &Expr,
    property_ty: &PhpType,
) -> bool {
    if type_can_be_null(property_ty) {
        return false;
    }
    let ExprKind::NullCoalesce {
        value: current,
        default: _,
    } = &value.kind
    else {
        return false;
    };
    let ExprKind::PropertyAccess {
        object: current_object,
        property: current_property,
    } = &current.kind
    else {
        return false;
    };
    current_property == property && assignment_expr_equivalent(current_object, object)
}

/// Returns `true` if `ty` can represent a null value.
///
/// Checks whether the type is `Void`, contains `Void` in a union, or is `Mixed`,
/// all of which can hold a null-like value in PHP's type system.
fn type_can_be_null(ty: &PhpType) -> bool {
    *ty == PhpType::Void || Checker::union_contains_void(ty) || matches!(ty, PhpType::Mixed)
}

/// Returns `true` if `left` and `right` represent the same storage location for the purposes
/// of null-coalescing assignment analysis.
///
/// Compares variables (`$this` and named variables) and property accesses recursively.
/// Two property accesses are equivalent if they name the same property and their object
/// expressions are equivalent.
fn assignment_expr_equivalent(left: &Expr, right: &Expr) -> bool {
    match (&left.kind, &right.kind) {
        (ExprKind::Variable(a), ExprKind::Variable(b)) => a == b,
        (ExprKind::This, ExprKind::This) => true,
        (
            ExprKind::PropertyAccess {
                object: a_object,
                property: a_property,
            },
            ExprKind::PropertyAccess {
                object: b_object,
                property: b_property,
            },
        ) => a_property == b_property && assignment_expr_equivalent(a_object, b_object),
        _ => false,
    }
}
