//! Purpose:
//! Adapts shared strict guarantees to flow-sensitive checker types.
//!
//! Called from:
//! - Condition, operator, binding, and value-boundary checking.
//!
//! Key details:
//! - Known invalid kinds are diagnostics; dynamic values retain runtime guards.

use super::Checker;
use crate::errors::CompileError;
use crate::parser::ast::Expr;
use crate::span::Span;
use crate::types::{PhpType, TypeEnv};
use elephc_semantics::StrictFeature;

impl Checker {
    /// Rejects a statically impossible exact case while preserving runtime checks for open types.
    pub(crate) fn require_exact_switch_case(&self, subject: &PhpType, label: &PhpType,
        span: Span) -> Result<(), CompileError> {
        if !crate::semantics::comparisons::active(self.semantic_profile, self.semantic_source_mode)
            || self.exact_switch_types_overlap(subject, label) { return Ok(()); }
        Err(CompileError::new(span, &format!(
            "--strict-comparisons: switch label of type {label} cannot match subject of type {subject}")))
    }

    /// Refines PHP kind overlap with existing class inheritance and resource compatibility rules.
    fn exact_switch_types_overlap(&self, subject: &PhpType, label: &PhpType) -> bool {
        if let PhpType::Union(members) = subject {
            return members.iter().any(|member| self.exact_switch_types_overlap(member, label));
        }
        if let PhpType::Union(members) = label {
            return members.iter().any(|member| self.exact_switch_types_overlap(subject, member));
        }
        if !crate::semantics::comparisons::identity_kinds_overlap(subject, label) { return false; }
        match (subject, label) {
            (PhpType::Object(left), PhpType::Object(right)) => {
                if self.interfaces.contains_key(left) || self.interfaces.contains_key(right) { return true; }
                let (Some(left_class), Some(right_class)) = (self.classes.get(left), self.classes.get(right))
                    else { return true; };
                left_class.class_id == right_class.class_id || self.is_subclass_of(left, right)
                    || self.is_subclass_of(right, left)
            }
            (PhpType::Resource(_), PhpType::Resource(_)) => PhpType::resource_types_compatible(subject, label),
            _ => true,
        }
    }

    /// Checks an implicit scalar boundary using its source operation rather than physical storage.
    pub(crate) fn require_noncoercive_value_type(&self, expected: &PhpType, actual: &PhpType,
        source: crate::source::SourceMode, span: Span, context: &str) -> Result<(), CompileError> {
        self.require_mixed_value_type(expected, actual, source, span, context)?;
        if crate::semantics::nullability::non_null_contract(expected) {
            crate::semantics::nullability::validate_receiver(self.semantic_profile, source, actual, false, span)?;
        }
        if let elephc_semantics::conversions::ConversionPlan::Reject(feature) =
            crate::semantics::conversions::implicit(self.semantic_profile, source, expected, actual) {
            return Err(CompileError::new(span, &format!(
                "--{}: {context} requires {expected} without implicit conversion, got {actual}", feature.flag())));
        }
        Ok(())
    }

    /// Uses existing compatibility and total weak bindings for every dynamic source alternative.
    pub(crate) fn require_mixed_value_type(&self, expected: &PhpType, actual: &PhpType,
        source: crate::source::SourceMode, span: Span, context: &str) -> Result<(), CompileError> {
        if *expected == PhpType::Mixed { return Ok(()); }
        crate::semantics::mixed::validate_use(self.semantic_profile, source, actual, span, context,
            |member| Self::types_compatible(expected, member) || self.type_accepts(expected, member)
                || crate::types::param_binding::scalar_param_cast(expected, member).is_some()
                || *expected == PhpType::Str && matches!(member, PhpType::Object(_))
                    && self.valid_dynamic_property_name_type(member))
    }

    /// Requires a supported writable receiver and PHP key without rebuilding flow facts.
    pub(crate) fn require_mixed_dimension_write(&self, receiver: &PhpType,
        key: Option<&PhpType>, span: Span) -> Result<(), CompileError> {
        crate::semantics::mixed::validate_use(self.semantic_profile, self.semantic_source_mode,
            receiver, span, "offset writes", |member| match member {
                PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Void
                    | PhpType::False | PhpType::Bool => true,
                PhpType::Str => key.is_some(),
                PhpType::Object(name) => self.object_type_supports_dimensions(name),
                _ => false,
            })?;
        if let Some(key) = key.filter(|_| crate::semantics::mixed::uses_php_offset_key(receiver)
            || matches!(receiver, PhpType::Void | PhpType::False | PhpType::Bool)) {
            crate::semantics::mixed::validate_use(self.semantic_profile, self.semantic_source_mode,
                key, span, "PHP offset keys", crate::semantics::mixed::array_key_kind)?;
        }
        Ok(())
    }

    /// Validates countability using the current source policy and existing interface metadata.
    pub(crate) fn require_mixed_countable_type(&self, actual: &PhpType, span: Span) -> Result<(), CompileError> {
        crate::semantics::mixed::validate_use(self.semantic_profile, self.semantic_source_mode,
            actual, span, "counting", |member| match member {
                PhpType::Array(_) | PhpType::AssocArray { .. } => true,
                PhpType::Object(name) => self.object_type_implements_interface(name, "Countable"),
                _ => false,
            })
    }

    /// Proves the callable shape without rebuilding known-target resolution or argument planning.
    pub(crate) fn require_mixed_callable_type(&self, actual: &PhpType, span: Span) -> Result<(), CompileError> {
        crate::semantics::mixed::validate_use(self.semantic_profile, self.semantic_source_mode,
            actual, span, "callable invocation", |member| match member {
                PhpType::Callable | PhpType::DeclaredCallable | PhpType::Str
                    | PhpType::Array(_) | PhpType::AssocArray { .. } => true,
                PhpType::Object(class) => self.classes.get(class)
                    .is_some_and(|info| info.methods.contains_key("__invoke")),
                _ => false,
            })
    }

    /// Checks the PHP-visible boolean contract without restricting internal implementation tests.
    pub(crate) fn require_boolean_type(&self, ty: &PhpType, span: Span) -> Result<(), CompileError> {
        if self.semantic_source_mode == crate::source::SourceMode::Internal
            || !self.semantic_profile.contains(StrictFeature::Bools) { return Ok(()); }
        if boolean_possible(ty) { return Ok(()); }
        Err(CompileError::new(span, &format!("--strict-bools requires bool, got {ty}")))
    }

    /// Infers side effects once before validating a conditional operand.
    pub(crate) fn infer_condition_type_with_effects(&mut self, expr: &Expr, env: &mut TypeEnv) -> Result<PhpType, CompileError> {
        let ty = self.infer_type_with_assignment_effects(expr, env)?;
        self.require_boolean_type(&ty, expr.span)?;
        Ok(ty)
    }
}

/// Returns whether a value can be boolean without an implicit conversion.
fn boolean_possible(ty: &PhpType) -> bool {
    match ty {
        PhpType::Bool | PhpType::False | PhpType::Mixed | PhpType::Never => true,
        PhpType::Union(members) => members.iter().any(boolean_possible),
        _ => false,
    }
}
