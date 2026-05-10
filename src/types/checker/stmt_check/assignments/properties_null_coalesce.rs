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

fn type_can_be_null(ty: &PhpType) -> bool {
    *ty == PhpType::Void || Checker::union_contains_void(ty) || matches!(ty, PhpType::Mixed)
}

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
