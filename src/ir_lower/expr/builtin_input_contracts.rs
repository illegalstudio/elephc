//! Purpose:
//! Selects exact builtin input contracts before source argument preparation.
//!
//! Called from:
//! - The common builtin argument lowering entry point.
//!
//! Key details:
//! - Neutral AOT metadata retains nullable alternatives erased by storage signatures.
//! - Shared argument planning, reference modes and ownership stay authoritative.

use super::*;

/// Projects checked value inputs without changing defaults, names or reference places.
pub(super) fn checked_signature(
    ctx: &LoweringContext<'_, '_>, definition: Option<&crate::builtins::registry::BuiltinDef>,
    signature: Option<&FunctionSig>,
) -> Option<FunctionSig> {
    if ctx.builtin_source_mode() == crate::source::SourceMode::Internal
        || !(ctx.semantic_profile.contains(elephc_semantics::StrictFeature::Coercions)
            || ctx.semantic_profile.contains(elephc_semantics::StrictFeature::Nullability))
    { return None; }
    let definition = definition?;
    let mut signature = signature?.clone();
    let regular = crate::types::call_args::regular_param_count(&signature);
    for (index, (_, ty)) in signature.params.iter_mut().take(regular).enumerate() {
        if let Some(contract) = crate::builtins::input_contracts::parameter_type(definition, index) {
            *ty = contract;
        }
    }
    Some(signature)
}
