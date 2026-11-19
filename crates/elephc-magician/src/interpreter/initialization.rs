//! Purpose:
//! Shares ordinary missing-variable reads across the walker and bytecode VM.
//!
//! Called from:
//! - Expression LoadVar and VM load_local after visible binding lookup.
//!
//! Key details:
//! - Quiet language probes inspect presence directly and do not enter this path.
//! - Initialized null has a visible binding and never reaches this helper.

use super::*;

/// Throws the selected initialization error or preserves PHP's warning and null result.
pub(super) fn missing_variable(name: &str, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if context.semantic_profile().contains(elephc_semantics::StrictFeature::Initialization) {
        return throwables::eval_throw_error(
            &elephc_semantics::initialization::variable_message(name), context, values);
    }
    values.warning(&format!("Warning: Undefined variable ${name}\n"))?;
    values.null()
}
