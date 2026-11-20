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
    let element = local_binding_origins::replacement_element_source(Some(&source.ty));
    for (position, argument) in arguments.iter_mut().enumerate() {
        let mut proof = if source.entries.is_empty() {
            let mut proof = ArgumentSource::capture(crate::eval_ir::parameter_type(&element), context);
            proof.write_epoch = source.write_epoch;
            Some(proof)
        } else {
            let key = values.array_iter_key(array, position)?;
            let normalized = eval_array_reference_key(key, context.semantic_profile(), values);
            let released = release_expr_result(key, context, values);
            let key = normalized.and_then(|key| released.map(|()| key))?;
            key.and_then(|key| source.entries.iter().find(|(entry, _)| entry == &key)
                .and_then(|(_, proof)| proof.clone()))
        };
        if let Some(reference) = reference_cell(argument, values)? {
            proof = proof.filter(|proof| context.source_view_is_current(reference, proof.write_epoch));
        }
        argument.source_type = proof;
    }
    Ok(())
}

/// Selects canonical reference storage even when unpacking materialized its current payload value.
pub(super) fn reference_cell(argument: &EvaluatedCallArg, values: &mut impl RuntimeValueOps)
    -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if let Some(EvalReferenceTarget::Cell { cell }) = argument.ref_target.as_ref() { return Ok(Some(*cell)); }
    Ok(values.is_reference(argument.value)?.then_some(argument.value))
}
