//! Purpose:
//! Projects per-entry argument proofs from an owned tuple without inferring them from value tags.
//!
//! Called from:
//! - Callback-array and Reflection invokeArgs argument extraction.
//!
//! Key details:
//! - Explicit missing entry proofs remain unknown even when ArrayTypes inferred a data schema.
//! - Reference entries retain their original write stamp across later tuple effects.

use super::*;

/// Applies literal entry snapshots or a declared homogeneous element source in runtime key order.
pub(super) fn apply(array: RuntimeCellHandle, source: Option<&ArgumentSource>,
    arguments: &mut [EvaluatedCallArg], context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(source) = source else { return Ok(()); };
    for (position, argument) in arguments.iter_mut().enumerate() {
        let key = values.array_iter_key(array, position)?;
        let proof = entry(Some(source), key, context, values);
        let released = release_expr_result(key, context, values);
        let mut proof = proof.and_then(|proof| released.map(|()| proof))?;
        if let Some(reference) = reference_cell(argument, values)? {
            proof = proof.filter(|proof| context.source_view_is_current(reference, proof.write_epoch));
        }
        argument.source_type = proof;
    }
    Ok(())
}

/// Reads one tuple entry, preserving missing proofs and the original root stamp for declared schemas.
pub(super) fn entry(source: Option<&ArgumentSource>, key: RuntimeCellHandle,
    context: &ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<Option<ArgumentSource>, EvalStatus> {
    let Some(source) = source else { return Ok(None); };
    if source.entries.is_empty() {
        let element = local_binding_origins::replacement_element_source(Some(&source.ty));
        let mut proof = ArgumentSource::capture(crate::eval_ir::parameter_type(&element), context);
        proof.write_epoch = source.write_epoch;
        return Ok(Some(proof));
    }
    let key = eval_array_reference_key(key, context.semantic_profile(), values)?;
    Ok(key.and_then(|key| source.entries.iter().find(|(entry, _)| entry == &key)
        .and_then(|(_, proof)| proof.clone())))
}

/// Selects canonical reference storage even when unpacking materialized its current payload value.
pub(super) fn reference_cell(argument: &EvaluatedCallArg, values: &mut impl RuntimeValueOps)
    -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if let Some(EvalReferenceTarget::Cell { cell }) = argument.ref_target.as_ref() { return Ok(Some(*cell)); }
    Ok(values.is_reference(argument.value)?.then_some(argument.value))
}
