//! Purpose:
//! Keeps numeric local bindings stable without enabling arithmetic or overflow policies.
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
    /// Retains a numeric binding's original kinds and records an independently owned store guard.
    pub(crate) fn check_numeric_local_store(&mut self, name: &str, incoming: &PhpType,
        span: Span, env: &TypeEnv) -> Result<PhpType, CompileError> {
        if self.semantic_source_mode == crate::source::SourceMode::Internal
            || !self.semantic_profile.contains(StrictFeature::Locals) { return Ok(incoming.clone()); }
        let key = (self.current_loop_storage_scope.clone(), span, name.to_string());
        if self.local_store_contracts.get(&key).is_some_and(|contract| contract.feature == StrictFeature::Locals) {
            self.local_store_contracts.remove(&key);
        }
        let declared = self.typed_local_names.get(name);
        let original = declared.or_else(|| self.numeric_local_contracts.get(name))
            .or_else(|| self.narrowed_local_origins.get(name).map(|origin| &origin.origin))
            .or_else(|| self.guarded_union_types.get(name).map(|origin| &origin.ty))
            .or_else(|| env.get(name).filter(|_| self.definitely_bound_locals.contains(name)
                || self.local_binding_depth.contains_key(name) || self.active_globals.contains(name)
                || self.static_local_names.contains(name))).unwrap_or(incoming);
        if !numeric_contract(original) { return Ok(incoming.clone()); }
        let expected = original.clone();
        if declared.is_none() { self.numeric_local_contracts.insert(name.to_string(), expected.clone()); }
        match crate::semantics::conversions::membership(self.semantic_profile,
            self.semantic_source_mode, StrictFeature::Locals, &expected, incoming) {
            ConversionPlan::Reject(_) => Err(CompileError::new(span, &format!(
                "--strict-locals: cannot reassign ${name} from {expected} to {incoming}"))),
            ConversionPlan::Guard { .. } => {
                self.local_store_contracts.insert(key, LocalStoreContract {
                    ty: expected.clone(), feature: StrictFeature::Locals,
                });
                Ok(expected)
            }
            _ => Ok(incoming.clone()),
        }
    }
}

/// Recognizes numeric contracts without pinning array elements or consuming mixed declarations.
fn numeric_contract(ty: &PhpType) -> bool {
    match ty {
        PhpType::Int | PhpType::Float => true,
        PhpType::Union(members) => !members.is_empty() && members.iter().all(numeric_contract),
        _ => false,
    }
}
