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
/// Recognizes the conditional read of the same property in a desugared `??=` write.
/// Declared nullability does not prove whether the runtime value takes the keep branch.
pub(super) fn null_coalesce_property_targets_same_slot(
    object: &Expr,
    property: &str,
    value: &Expr,
) -> bool {
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
