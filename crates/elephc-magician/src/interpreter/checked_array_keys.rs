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
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayKeys)
        || !matches!(values.type_tag(receiver)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC)
        || matches!(values.type_tag(key)?, EVAL_TAG_INT | EVAL_TAG_STRING)
    { return Ok(()); }
    eval_throw_type_error(elephc_semantics::array_keys::EXACT_KEY_MESSAGE, context, values)
}
