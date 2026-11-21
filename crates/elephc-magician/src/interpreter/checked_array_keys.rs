//! Purpose:
//! Validates original key kinds after eval selects a PHP array receiver.
//!
//! Called from:
//! - Array reads, literal construction and array mutation boundaries.
//!
//! Key details:
//! - Checks borrow caller-owned handles and never normalize or retain a key.
//! - String offsets and ArrayAccess keep their independent key contracts.

use super::*;

/// Checks an original key only when the selected receiver is a PHP array.
pub(super) fn for_receiver(
    receiver: RuntimeCellHandle, key: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !matches!(values.type_tag(receiver)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) { return Ok(()); }
    if context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayKeys) {
        return require_key(key, false, context, values);
    }
    let tag = values.type_tag(key)?;
    if matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC | EVAL_TAG_OBJECT) {
        let name = if tag == EVAL_TAG_OBJECT { eval_dynamic_class_name(key, context, values)? }
            else { "array".to_string() };
        return eval_throw_type_error(&format!("Cannot access offset of type {name} on array"), context, values);
    }
    Ok(())
}

/// Checks a key-producing builtin input; null may explicitly select a whole column.
pub(super) fn require_key(
    key: RuntimeCellHandle, nullable: bool, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayKeys) { return Ok(()); }
    let tag = values.type_tag(key)?;
    if matches!(tag, EVAL_TAG_INT | EVAL_TAG_STRING) || nullable && tag == EVAL_TAG_NULL { return Ok(()); }
    eval_throw_type_error(elephc_semantics::array_keys::EXACT_KEY_MESSAGE, context, values)
}

/// Rejects an exact-policy compound key before null or false can become an array parent.
pub(super) fn before_update_parent_materialization(
    receiver: RuntimeCellHandle, key: Option<RuntimeCellHandle>, update: bool,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !update { return Ok(()); }
    before_selected_parent_materialization(receiver, key, context, values)
}

/// Rejects an exact key before an unprepared null or false receiver becomes an array.
pub(super) fn before_selected_parent_materialization(
    receiver: RuntimeCellHandle, key: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(key) = key else { return Ok(()); };
    let tag = values.type_tag(receiver)?;
    let materializes = tag == EVAL_TAG_NULL
        || tag == EVAL_TAG_BOOL && !values.truthy(receiver)?;
    if materializes { require_key(key, false, context, values)?; }
    Ok(())
}
