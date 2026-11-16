//! Purpose:
//! Checks original string operand and formatting kinds for both eval engines.
//!
//! Called from:
//! - The shared evaluated concatenation, interpolation and output boundaries.
//!
//! Key details:
//! - Checks borrow existing handles and never invoke conversions or Stringable hooks.
//! - Existing argument/output owner finishers retain responsibility for cleanup.

use super::*;
use elephc_semantics::{StrictFeature, conversions::{STRING_CONCAT_MESSAGE, STRING_FORMAT_MESSAGE}};

/// Requires two actual strings before either operand can run a conversion hook.
pub(super) fn concatenation(left: RuntimeCellHandle, right: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !context.semantic_profile().contains(StrictFeature::Strings) { return Ok(()); }
    if values.type_tag(left)? == EVAL_TAG_STRING && values.type_tag(right)? == EVAL_TAG_STRING { return Ok(()); }
    eval_throw_type_error(STRING_CONCAT_MESSAGE, context, values)
}

/// Refuses implicit Array text without changing explicit casts or valid scalar/object formatting.
pub(super) fn formatting(value: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if context.semantic_profile().contains(StrictFeature::Strings)
        && matches!(values.type_tag(value)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC)
    { return eval_throw_type_error(STRING_FORMAT_MESSAGE, context, values); }
    Ok(())
}
