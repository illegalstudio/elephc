//! Purpose:
//! Keeps local value contracts stable independently of conversion and array element policies.
//!
//! Called from:
//! - Whole-local assignments and prefix/postfix update checking.
//!
//! Key details:
//! - Binding contracts survive flow narrowing and storage widening, and end at a binding kill.
//! - Runtime membership guards validate a candidate before the destination changes.

use super::Checker;
use crate::{errors::CompileError, span::Span, types::{LocalStoreContract, PhpType, TypeEnv}};
use elephc_semantics::{conversions::ConversionPlan, StrictFeature};

impl Checker {
    /// Captures initialized binding origins before an opaque eval can widen their views.
    pub(crate) fn record_eval_binding_contracts(&mut self, env: &TypeEnv, span: Span) {
        for (name, actual) in env {
            if self.definitely_bound_locals.contains(name) || self.local_parameter_names.contains(name) {
                self.record_local_reference_contract(name, actual, span);
            }
        }
    }

    /// Captures the original local contract independently of a callee's physical parameter ABI.
    pub(crate) fn record_reference_argument_contract(&mut self,
        arg: &crate::parser::ast::Expr, actual: &PhpType) {
        let mut source = arg;
        while let crate::parser::ast::ExprKind::NamedArg { value, .. }
            | crate::parser::ast::ExprKind::ErrorSuppress(value) = &source.kind { source = value; }
        if let crate::parser::ast::ExprKind::Variable(name) = &source.kind {
            self.record_local_reference_contract(name, actual, source.span);
        }
    }

    /// Carries the semantic binding contract across promotion to boxed reference storage.
    pub(crate) fn record_local_reference_contract(&mut self, name: &str, actual: &PhpType, span: Span) -> bool {
        if self.semantic_source_mode == crate::source::SourceMode::Internal
            || !self.semantic_profile.contains(StrictFeature::Locals) { return false; }
        let expected = self.typed_local_names.get(name)
            .or_else(|| self.inferred_local_contracts.get(name))
            .or_else(|| self.narrowed_local_origins.get(name).map(|origin| &origin.origin))
            .or_else(|| self.guarded_union_types.get(name).map(|origin| &origin.ty))
            .unwrap_or(actual);
        if let Some(expected) = binding_contract(expected, self.typed_local_names.contains_key(name)) {
            self.local_store_contracts.insert((self.current_loop_storage_scope.clone(), span, name.into()),
                LocalStoreContract { ty: expected, feature: StrictFeature::Locals });
            return true;
        }
        false
    }

    /// Retains original binding kinds and records an independently owned pre-store guard.
    pub(crate) fn check_local_binding_store(&mut self, name: &str, incoming: &PhpType,
        span: Span, env: &TypeEnv, source: Option<&crate::parser::ast::Expr>) -> Result<PhpType, CompileError> {
        if self.semantic_source_mode == crate::source::SourceMode::Internal
            || !self.semantic_profile.contains(StrictFeature::Locals) { return Ok(incoming.clone()); }
        let key = (self.current_loop_storage_scope.clone(), span, name.to_string());
        if self.local_store_contracts.get(&key).is_some_and(|contract| contract.feature == StrictFeature::Locals) {
            self.local_store_contracts.remove(&key);
        }
        let declared = self.typed_local_names.get(name);
        let original = declared.or_else(|| self.inferred_local_contracts.get(name))
            .or_else(|| self.narrowed_local_origins.get(name).map(|origin| &origin.origin))
            .or_else(|| self.guarded_union_types.get(name).map(|origin| &origin.ty))
            .or_else(|| env.get(name).filter(|_| self.definitely_bound_locals.contains(name)
                || self.local_binding_depth.contains_key(name) || self.active_globals.contains(name)
                || self.static_local_names.contains(name))).unwrap_or(incoming);
        let Some(expected) = binding_contract(original, declared.is_some()) else { return Ok(incoming.clone()); };
        if declared.is_none() { self.inferred_local_contracts.insert(name.to_string(), expected.clone()); }
        // Array element and storage changes do not change the PHP array value kind.
        if expected.is_php_array() && (incoming.is_php_array()
            || matches!(incoming, PhpType::Array(_) | PhpType::AssocArray { .. })) {
            return Ok(incoming.clone());
        }
        match crate::semantics::conversions::membership(self.semantic_profile,
            self.semantic_source_mode, StrictFeature::Locals, &expected, incoming) {
            ConversionPlan::Reject(_) => Err(CompileError::new(span, &format!(
                "--strict-locals: cannot reassign ${name} from {expected} to {incoming}"))),
            ConversionPlan::Guard { .. } => {
                self.local_store_contracts.insert(key, LocalStoreContract {
                    ty: expected.clone(), feature: StrictFeature::Locals,
                });
                let view = guarded_binding_view(self, &expected, incoming);
                let non_null = self.strip_void_from_union(&view);
                Ok(if non_null != view && source.is_some_and(|source|
                    self.stored_value_is_non_null(source, &non_null, env)) { non_null } else { view })
            }
            _ => Ok(incoming.clone()),
        }
    }
}

/// Leaves concrete declared element contracts with their existing structural/layout checker.
fn binding_contract(ty: &PhpType, declared: bool) -> Option<PhpType> {
    if declared && !crate::semantics::conversions::permits_guarded_unknown(ty) { return None; }
    crate::semantics::conversions::inferred_binding_contract(ty)
}

/// Preserves a successful source view without treating permitted null as a produced value.
fn guarded_binding_view(checker: &Checker, expected: &PhpType, incoming: &PhpType) -> PhpType {
    let PhpType::Union(members) = incoming else { return expected.clone(); };
    let mut accepted = Vec::new();
    for member in members {
        match crate::semantics::conversions::membership(checker.semantic_profile,
            checker.semantic_source_mode, StrictFeature::Locals, expected, member) {
            ConversionPlan::Identity => accepted.push(member.clone()),
            ConversionPlan::Reject(_) => {},
            _ => return expected.clone(),
        }
    }
    if accepted.is_empty() { expected.clone() } else { checker.normalize_union_type(accepted) }
}
