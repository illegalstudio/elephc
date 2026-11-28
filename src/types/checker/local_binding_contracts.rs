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
    /// Selects boxed frame bindings whose first producer can differ across source paths.
    pub(crate) fn runtime_local_binding_origin(&self, name: &str) -> bool {
        self.semantic_profile.contains(StrictFeature::Locals)
            && (!self.body_contains_eval
                || self.local_store_contracts.has_runtime_origin(&self.current_loop_storage_scope, name))
            && !self.typed_local_names.contains_key(name)
            && (!self.active_ref_params.contains(name)
                || self.local_store_contracts.entry_is_reference(&self.current_loop_storage_scope, name))
            && (!self.local_parameter_names.contains(name)
                || self.local_store_contracts.entry_origin(&self.current_loop_storage_scope, name).is_some())
            && (!self.by_ref_captured_locals.contains(name)
                || self.local_store_contracts.has_runtime_origin(&self.current_loop_storage_scope, name))
            && (!self.active_globals.contains(name)
                || self.local_store_contracts.has_runtime_origin(&self.current_loop_storage_scope, name))
            && (!self.static_local_names.contains(name)
                || self.local_store_contracts.has_runtime_origin(&self.current_loop_storage_scope, name))
            && (self.mixed_storage_locals.contains(name) || self.local_store_contracts.has_runtime_origin(&self.current_loop_storage_scope, name))
    }

    /// Records a declaration's incoming producer independently of an older runtime origin.
    pub(crate) fn record_declared_binding_contract(&mut self, name: &str, original: &PhpType, span: Span) {
        if self.semantic_source_mode == crate::source::SourceMode::Internal
            || !self.semantic_profile.contains(StrictFeature::Locals) { return; }
        let key = (self.current_loop_storage_scope.clone(), span, name.into());
        if let Some(contract) = crate::semantics::conversions::inferred_binding_contract(original) {
            self.local_store_contracts.insert(key, LocalStoreContract { ty: contract, feature: StrictFeature::Locals });
        } else { self.local_store_contracts.remove(&key); }
    }

    /// Captures initialized binding origins before an opaque eval can widen their views.
    pub(crate) fn record_eval_binding_contracts(&mut self, env: &TypeEnv, span: Span) {
        for (name, actual) in env {
            if self.semantic_source_mode != crate::source::SourceMode::Internal
                && [StrictFeature::Mixed, StrictFeature::ArrayTypes].iter()
                    .any(|feature| self.semantic_profile.contains(*feature)) {
                let original = self.local_binding_origin(name, actual);
                let original = if crate::types::native_type_spec::logical_type(&original, false).is_some() {
                    original
                } else { PhpType::Mixed };
                self.local_store_contracts.eval_sources.insert(
                    (self.current_loop_storage_scope.clone(), span, name.clone()),
                    (original, self.typed_local_names.contains_key(name)));
            }
            if self.record_local_reference_contract(name, actual, span) {
                let key = (self.current_loop_storage_scope.clone(), span, name.clone());
                if let Some(contract) = self.local_store_contracts.get(&key) {
                    self.inferred_local_contracts.entry(name.clone()).or_insert_with(|| contract.ty.clone());
                }
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

    /// Resolves an original producer contract before flow narrowing or destination rebinding.
    pub(crate) fn local_binding_origin(&self, name: &str, actual: &PhpType) -> PhpType {
        if self.runtime_local_binding_origin(name) { return PhpType::Mixed; }
        self.typed_local_names.get(name)
            .or_else(|| self.inferred_local_contracts.get(name))
            .or_else(|| self.narrowed_local_origins.get(name).map(|origin| &origin.origin))
            .or_else(|| self.guarded_union_types.get(name).map(|origin| &origin.ty))
            .unwrap_or(actual).clone()
    }

    /// Captures a copied initializer's producer while its original binding metadata is available.
    pub(crate) fn copied_binding_origin(&self, source: &crate::parser::ast::Expr, env: &TypeEnv) -> Option<PhpType> {
        let name = copy_producer_local(source)?;
        env.get(name).map(|actual| self.local_binding_origin(name, actual))
    }

    /// Carries the semantic binding contract across promotion to boxed reference storage.
    pub(crate) fn record_local_reference_contract(&mut self, name: &str, actual: &PhpType, span: Span) -> bool {
        if self.semantic_source_mode == crate::source::SourceMode::Internal
            || !self.semantic_profile.contains(StrictFeature::Locals) { return false; }
        let expected = self.local_binding_origin(name, actual);
        if let Some(expected) = binding_contract(&expected, self.typed_local_names.contains_key(name)) {
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
        self.local_store_contracts.copies.remove(&key);
        let copy = source.and_then(copy_producer_local);
        if self.runtime_local_binding_origin(name)
            || (!env.contains_key(name)
                && copy.is_some_and(|source| self.runtime_local_binding_origin(source))) {
            if matches!(incoming.codegen_repr(), PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_)) {
                return Err(CompileError::new(span,
                    "--strict-locals: conditional native storage requires an explicit local type"));
            }
            let producer = source.and_then(|source| self.copied_binding_origin(source, env))
                .unwrap_or_else(|| incoming.clone());
            let contract = crate::semantics::conversions::inferred_binding_contract(&producer)
                .unwrap_or(PhpType::Mixed);
            if let Some(source) = copy {
                self.local_store_contracts.copies.insert(key.clone(), crate::types::LocalCopyContract {
                    source: source.into(), contract: contract.clone(),
                });
            }
            self.local_store_contracts.remove(&key);
            self.local_store_contracts.record_origin(key, contract);
            if self.definitely_bound_locals.contains(name) {
                if let Some(expected) = self.inferred_local_contracts.get(name) {
                    if matches!(crate::semantics::conversions::membership(self.semantic_profile,
                        self.semantic_source_mode, StrictFeature::Locals, expected, incoming),
                        ConversionPlan::Reject(_)) {
                        return Err(CompileError::new(span, &format!(
                            "--strict-locals: cannot reassign ${name} from {expected} to {incoming}")));
                    }
                }
            } else {
                self.inferred_local_contracts.insert(name.into(),
                    crate::semantics::conversions::inferred_binding_contract(incoming).unwrap_or(PhpType::Mixed));
            }
            return Ok(incoming.clone());
        }
        if self.eval_barrier_active {
            if let Some(source) = source.and_then(copy_producer_local) {
                let ty = self.typed_local_names.get(source).or_else(|| self.inferred_local_contracts.get(source))
                    .or_else(|| self.narrowed_local_origins.get(source).map(|origin| &origin.origin))
                    .or_else(|| self.guarded_union_types.get(source).map(|origin| &origin.ty))
                    .or_else(|| env.get(source)).unwrap_or(incoming);
                let contract = crate::semantics::conversions::inferred_binding_contract(ty).unwrap_or(PhpType::Mixed);
                self.local_store_contracts.copies.insert(key.clone(), crate::types::LocalCopyContract {
                    source: source.into(), contract,
                });
            }
        }
        if self.local_store_contracts.get(&key).is_some_and(|contract| contract.feature == StrictFeature::Locals) {
            self.local_store_contracts.remove(&key);
        }
        let declared = self.typed_local_names.get(name);
        // A store governed by an exact flow guard is checked against that guard's current view.
        // Family guards such as `is_array` can publish `Mixed` when the source has no matching
        // member; that is a conservative read view, not a replacement binding contract. Once the
        // first store replaces an exact view, later stores in the same region are checked against
        // the value the region itself published. The inferred binding contract remains untouched
        // so closing the region can restore the pre-guard binding.
        let guarded = self.narrowed_local_origins.get(name).and_then(|origin| {
            origin.view.as_ref()
                .filter(|view| crate::semantics::conversions::inferred_binding_contract(view).is_some())
                .or_else(|| origin.stored_in_region.then(|| env.get(name)).flatten())
        });
        let original = declared.or(guarded).or_else(|| self.inferred_local_contracts.get(name))
            .or_else(|| self.narrowed_local_origins.get(name).map(|origin| &origin.origin))
            .or_else(|| self.guarded_union_types.get(name).map(|origin| &origin.ty))
            .or_else(|| env.get(name).filter(|_| self.definitely_bound_locals.contains(name)
                || self.local_binding_depth.contains_key(name) || self.active_globals.contains(name)
                || self.static_local_names.contains(name))).unwrap_or(incoming);
        let Some(expected) = binding_contract(original, declared.is_some()) else { return Ok(incoming.clone()); };
        if declared.is_none() { self.inferred_local_contracts.insert(name.to_string(), expected.clone()); }
        if source.is_some() && self.eval_barrier_active && declared.is_none()
            && !self.active_ref_params.contains(name) && !self.ref_aliased_locals.contains(name)
            && !self.active_globals.contains(name) && !self.static_local_names.contains(name) {
            // Eval may have ended this origin and established another one. Keep the old
            // contract for the native-origin predecessor without narrowing the new RHS to it.
            self.local_store_contracts.insert(key, LocalStoreContract {
                ty: expected, feature: StrictFeature::Locals,
            });
            return Ok(incoming.clone());
        }
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

/// Finds the binding that produces a copied value before optimizer propagation can erase its read.
fn copy_producer_local(expr: &crate::parser::ast::Expr) -> Option<&str> {
    use crate::parser::ast::ExprKind;
    match &expr.kind {
        ExprKind::Variable(name) => Some(name),
        ExprKind::ErrorSuppress(value) | ExprKind::NamedArg { value, .. } => copy_producer_local(value),
        ExprKind::Assignment { target, result_target, .. } => copy_producer_local(result_target.as_deref().unwrap_or(target)),
        _ => None,
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
