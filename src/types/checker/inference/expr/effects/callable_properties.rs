//! Purpose:
//! Select native property reference storage for callable arguments before their later side effects.
//!
//! Called from:
//! - `super::Checker::infer_type_with_assignment_effects()` for named and expression callables.
//!
//! Key details:
//! - Storage promotion does not choose a runtime passing mode; by-value arguments still snapshot.
//! - Readonly storage is rejected only when a known target actually requires a reference.

use super::*;

/// Describes whether argument checking needs writable or potentially reference-capable storage.
#[derive(Clone, Copy)]
pub(super) enum CallablePropertyReferenceMode {
    LiveArrayReceiver,
    PotentialReference,
    ValueOnly,
}

impl Checker {
    /// Keeps target classification separate from source-order argument evaluation.
    pub(super) fn callable_property_reference_mode(
        &mut self,
        callee: &Expr,
        env: &TypeEnv,
    ) -> Result<CallablePropertyReferenceMode, CompileError> {
        let target = match &callee.kind {
            ExprKind::FirstClassCallable(target) => Some(target),
            ExprKind::Variable(name) => self.first_class_callable_targets.get(name),
            _ => None,
        };
        if matches!(target, Some(CallableTarget::Function(name))
            if !self.functions.contains_key(name.as_str())
                && builtin_captures_live_array_property(name.as_str()))
        {
            return Ok(CallablePropertyReferenceMode::LiveArrayReceiver);
        }
        let may_bind = self.resolve_expr_callable_sig(callee, env)?.is_none_or(|sig|
            sig.ref_params.iter().any(|is_ref| *is_ref) || sig.variadic.is_some());
        Ok(if may_bind {
            CallablePropertyReferenceMode::PotentialReference
        } else {
            CallablePropertyReferenceMode::ValueOnly
        })
    }

    /// Makes storage addressable at the argument's PHP evaluation point without evaluating it.
    pub(super) fn promote_callable_array_property_argument(
        &mut self,
        mode: CallablePropertyReferenceMode,
        source_index: usize,
        arg: &Expr,
        env: &TypeEnv,
    ) -> Result<(), CompileError> {
        let place = match &arg.kind {
            ExprKind::NamedArg { value, .. } => value.as_ref(),
            _ => arg,
        };
        match mode {
            CallablePropertyReferenceMode::LiveArrayReceiver if source_index == 0 =>
                self.promote_builtin_array_property_reference(place, env),
            CallablePropertyReferenceMode::PotentialReference =>
                self.promote_runtime_callable_array_property_reference(place, env),
            _ => Ok(()),
        }
    }
}

/// Selects property capture from the shared builtin semantics rather than a PHP-name list.
pub(super) fn builtin_captures_live_array_property(name: &str) -> bool {
    crate::builtins::registry::lookup(name).is_some_and(|definition| matches!(
        definition.spec.semantics.lowering,
        crate::builtins::semantics::BuiltinLowering::Runtime(
            crate::ir::RuntimeCallTarget::Function(crate::ir::RuntimeFnId::ArrayUnshift)
        )
    ))
}
