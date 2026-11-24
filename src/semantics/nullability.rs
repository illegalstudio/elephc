//! Purpose:
//! Shares logical null proofs and precise quiet access spines across compiler passes.
//!
//! Called from:
//! - Pre-fold source validation, receiver inference and EIR nullability guards.
//!
//! Key details:
//! - Keys and dynamic member names are ordinary expressions, even inside a probe.

use crate::{ast_visit::{walk_program, AstVisitor}, errors::CompileError,
    parser::ast::{Expr, ExprKind, Program, TypeExpr}, source::SourceMode,
    span::Span, types::PhpType};
use elephc_semantics::{SemanticProfile, nullability::{NullPlan, NullState, plan_non_null}};

pub(crate) use elephc_semantics::nullability::NON_NULL_MESSAGE;

/// Rejects literal-null declaration defaults before PHP compatibility widens their annotations.
pub(crate) fn validate_original_declarations(
    program: &Program,
    profile: SemanticProfile,
) -> Result<(), CompileError> {
    if !profile.contains(elephc_semantics::StrictFeature::Nullability) {
        return Ok(());
    }
    let mut validator = OriginalDeclarationValidator { error: None };
    walk_program(program, &mut validator);
    validator.error.map_or(Ok(()), Err)
}

/// Retains written declaration contracts independently of later PHP signature normalization.
struct OriginalDeclarationValidator {
    error: Option<CompileError>,
}

impl AstVisitor for OriginalDeclarationValidator {
    /// Rejects only an original user null literal whose written type excludes null.
    fn declared_value(&mut self, annotation: &TypeExpr, value: &Expr) {
        if self.error.is_some()
            || value.source_profile.mode == SourceMode::Internal
            || !matches!(value.kind, ExprKind::Null)
        {
            return;
        }
        let Some(expected) = super::conversions::context_free_hint(annotation) else {
            return;
        };
        if non_null_contract(&expected) {
            self.error = Some(CompileError::new(value.span, NON_NULL_MESSAGE));
        }
    }
}

/// Recognizes closed declarations that exclude null without imposing exact kind membership.
pub(crate) fn non_null_contract(ty: &PhpType) -> bool {
    super::conversions::implicit_contract(ty).is_some_and(|allowed|
        !allowed.allows(elephc_semantics::conversions::ValueKind::Null))
}

/// Selects original-null validation independently of exact-kind membership.
pub(crate) fn contract_active(profile: SemanticProfile, ty: &PhpType) -> bool {
    profile.contains(elephc_semantics::StrictFeature::Nullability) && non_null_contract(ty)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Original implicit-null syntax remains rejectable after PHP normalization would widen it.
    #[test]
    fn strict_nullability_rejects_original_implicit_nullable_parameters() {
        let source = "<?php function rejected(int $value = null) {}";
        let program = crate::parser::parse_with_mode(
            &crate::lexer::tokenize(source).unwrap(),
            SourceMode::Php,
        )
        .unwrap();
        let profile = SemanticProfile::PERMISSIVE
            .with(elephc_semantics::StrictFeature::Nullability, true);
        assert!(validate_original_declarations(&program, profile).is_err());

        let normalized = crate::magic_constants::substitute_file_and_scope_constants(
            program,
            std::path::Path::new("strict.php"),
        );
        assert!(crate::semantics::validate_source(&normalized, profile).is_ok());
    }
}
