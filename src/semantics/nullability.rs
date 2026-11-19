//! Purpose:
//! Shares logical null proofs and precise quiet access spines across compiler passes.
//!
//! Called from:
//! - Pre-fold source validation, receiver inference and EIR nullability guards.
//!
//! Key details:
//! - Keys and dynamic member names are ordinary expressions, even inside a probe.

use crate::{errors::CompileError, parser::ast::{Expr, ExprKind}, source::SourceMode,
    span::Span, types::PhpType};
use elephc_semantics::{SemanticProfile, nullability::{NullPlan, NullState, plan_non_null}};

pub(crate) use elephc_semantics::nullability::NON_NULL_MESSAGE;

/// Recognizes closed declarations that exclude null without imposing exact kind membership.
pub(crate) fn non_null_contract(ty: &PhpType) -> bool {
    super::conversions::implicit_contract(ty).is_some_and(|allowed|
        !allowed.allows(elephc_semantics::conversions::ValueKind::Null))
}

/// Projects logical alternatives without treating a storage representation as a null proof.
pub(crate) fn null_state(ty: &PhpType) -> NullState {
    match ty {
        PhpType::Void => NullState::Null,
        PhpType::Mixed | PhpType::TaggedScalar => NullState::Unknown,
        PhpType::Union(members) => {
            let mut states = members.iter().map(null_state);
            let Some(first) = states.next() else { return NullState::NonNull; };
            if states.all(|state| state == first) { first } else { NullState::Unknown }
        }
        PhpType::Narrow(_) => null_state(&ty.storage_value_type()),
        _ => NullState::NonNull,
    }
}

/// Rejects a proven null while retaining dynamic alternatives as runtime obligations.
pub(crate) fn validate_receiver(profile: SemanticProfile, source: SourceMode,
    ty: &PhpType, probe: bool, span: Span) -> Result<(), CompileError> {
    if source != SourceMode::Internal
        && plan_non_null(profile, null_state(ty), probe) == NullPlan::Reject {
        return Err(CompileError::new(span, NON_NULL_MESSAGE));
    }
    Ok(())
}

/// Records only the access operations on a quiet operand's receiver spine.
pub(crate) fn quiet_spine(expr: &Expr, operations: &mut Vec<Span>) {
    match &expr.kind {
        ExprKind::ArrayAccess { array: receiver, .. }
        | ExprKind::PropertyAccess { object: receiver, .. }
        | ExprKind::DynamicPropertyAccess { object: receiver, .. }
        | ExprKind::NullsafePropertyAccess { object: receiver, .. }
        | ExprKind::NullsafeDynamicPropertyAccess { object: receiver, .. } => {
            operations.push(expr.span);
            quiet_spine(receiver, operations);
        }
        _ => {}
    }
}

/// Installs an expression's explicit probe or slot-creation permissions before traversal.
pub(crate) fn expression_quiet_spines(expr: &Expr, operations: &mut Vec<Span>) {
    match &expr.kind {
        ExprKind::NullCoalesce { value, .. } => quiet_spine(value, operations),
        ExprKind::FunctionCall { name, args } if matches!(
            crate::names::php_symbol_key(name.as_str().trim_start_matches('\\')).as_str(),
            "isset" | "empty" | "unset") => {
                for arg in args { quiet_spine(arg, operations); }
            }
        ExprKind::Assignment { target, .. } => quiet_spine(target, operations),
        _ => {}
    }
}
