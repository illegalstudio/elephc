//! Purpose:
//! Identifies native XML selections that PHP passes through an independent reference cell.
//!
//! Called from:
//! - Call validation and EIR argument lowering.
//!
//! Key details:
//! - Replacing the argument must never write back to the parent XML node.
//! - Ordinary object properties and unknown receiver classes keep their existing rules.

use std::collections::HashMap;

use crate::parser::ast::{Expr, ExprKind};
use crate::types::{ClassInfo, PhpType};

/// Returns the receiver of a selection that may use the native XML reference protocol.
pub(crate) fn native_selection_reference_receiver(expr: &Expr) -> Option<&Expr> {
    match &expr.kind {
        ExprKind::NamedArg { value, .. } =>
            native_selection_reference_receiver(value),
        ExprKind::PropertyAccess { object, property } if !property.starts_with("__elephc_") => Some(object),
        ExprKind::DynamicPropertyAccess { object, property }
            if !matches!(&property.kind, ExprKind::StringLiteral(name) if name.starts_with("__elephc_")) => Some(object),
        ExprKind::ArrayAccess { array, .. } => Some(array),
        _ => None,
    }
}

/// Accepts a known XML family, including a nullable view, without widening ordinary objects.
pub(crate) fn native_selection_reference_receiver_type(
    ty: &PhpType, classes: &HashMap<String, ClassInfo>,
) -> bool {
    match ty {
        PhpType::Object(class) => crate::types::builtin_classes::has_native_dimensions(class, classes),
        PhpType::Union(members) => members.iter().any(|ty| native_selection_reference_receiver_type(ty, classes))
            && members.iter().all(|ty| matches!(ty, PhpType::Void | PhpType::Never)
                || native_selection_reference_receiver_type(ty, classes)),
        _ => false,
    }
}
