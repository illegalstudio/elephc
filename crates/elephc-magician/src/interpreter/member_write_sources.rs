//! Purpose:
//! Carries evaluated member producer proofs independently from validated internal writes.
//!
//! Called from:
//! - Property operand staging and instance/static backing-slot publication.
//!
//! Key details:
//! - Proof packets retain original reference epochs and own no PHP values.
//! - Inferred contents use producer alternatives, while scalar binding shares call validation.

use super::*;
use crate::eval_ir::{generic_type, parameter_type};

/// Distinguishes a previously validated internal store from an evaluated user producer.
#[derive(Clone, Copy)]
pub(in crate::interpreter) enum MemberWriteSource<'a> {
    Internal,
    Evaluated(Option<&'a ArgumentSource>),
}

impl MemberWriteSource<'_> {
    /// Validates logical alternatives in the selected declaring and called scopes before conversion.
    pub(in crate::interpreter) fn validate(self, declared: Option<&EvalParameterType>, owner: &str,
        called: &str, value: RuntimeCellHandle, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
    ) -> Result<(), EvalStatus> {
        let Self::Evaluated(source) = self else { return Ok(()); };
        if !context.semantic_profile().contains(elephc_semantics::StrictFeature::Mixed) { return Ok(()); }
        let Some(declared) = declared else { return Ok(()); };
        let expected = local_binding_origins::resolve_declared_source_type(&generic_type(declared), Some(owner), Some(called), context)?;
        mixed_source_proofs::require_argument(&parameter_type(&expected), source, value, false, context, values)
    }

    /// Copies a proof for a reached setter hook without replacing missing evidence with value tags.
    pub(in crate::interpreter) fn argument(self) -> Option<ArgumentSource> {
        match self { Self::Internal => None, Self::Evaluated(source) => source.cloned() }
    }

    /// Prepares only absent or pending inferred contents, preserving established binding obligations.
    pub(in crate::interpreter) fn array_origin(self, existing: Option<&EvalParameterType>,
        declared: Option<&EvalParameterType>, name: &str, value: RuntimeCellHandle,
        context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
    ) -> Result<Option<EvalParameterType>, EvalStatus> {
        let Self::Evaluated(source) = self else { return Ok(None); };
        if existing.is_some_and(|ty| !crate::eval_ir::pending_array_contents(&generic_type(ty))) { return Ok(None); }
        local_binding_origins::prepare_array_value_origin(declared, source.map(|source| &source.ty), value, name, context, values)
    }
}

/// Evaluates one member RHS once, separating literal data schemas from optional stored proof packets.
pub(in crate::interpreter) fn evaluate_member_value(expr: &EvalExpr, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<ArgumentSource>), EvalStatus> {
    let selected = context.semantic_profile().contains(elephc_semantics::StrictFeature::Mixed)
        || context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayTypes);
    if !selected { return eval_owned_expr(expr, context, scope, values).map(|value| (value, None)); }
    if let EvalExpr::Array(elements) = expr {
        return array_literals::evaluate_argument_array(elements, context, scope, values).map(|(value, source)| (value, Some(source)));
    }
    let packet = local_binding_origins::argument_source(expr, context, scope);
    let (value, source) = if context.semantic_profile().contains(elephc_semantics::StrictFeature::Mixed) {
        let (value, source) = mixed_eval::evaluate(expr, context, scope, values)?;
        (value, source.as_ref().map(parameter_type))
    } else { array_literals::evaluate_value(expr, context, scope, values)? };
    Ok((value, packet.or_else(|| source.map(|source| ArgumentSource::capture(source, context)))))
}

/// Publishes an inferred reference origin after successful canonical replacement and before old-value callbacks.
pub(in crate::interpreter) fn write_member_reference_origin<V: RuntimeValueOps>(target: &EvalReferenceTarget,
    value: RuntimeCellHandle, origin: Option<&EvalParameterType>, context: &mut ElephcEvalContext, values: &mut V,
    publish: impl FnOnce(&mut ElephcEvalContext, &mut V, &EvalParameterType, RuntimeCellHandle) -> Result<(), EvalStatus>,
) -> Result<bool, EvalStatus> {
    let Some(origin) = origin else { return Ok(false); };
    let EvalReferenceTarget::Cell { cell } = target else { return Ok(false); };
    if !values.is_reference(*cell)? { return Ok(false); }
    array_reference_contracts::validate_new_owner(*cell, value, std::slice::from_ref(origin), context, values)?;
    let previous = eval_replace_reference_value(*cell, value, context, values)?;
    let published = publish(context, values, origin, *cell);
    let retired = eval_release_value(context, values, previous);
    published.and(retired).map(|()| true)
}
