//! Purpose:
//! Shares explicit native Reflection invocation limits across direct and indirect calls.
//!
//! Called from:
//! - Reflection lowering and synthetic method fallback construction.
//!
//! Key details:
//! - Known direct targets retain their specialized lowering; generic shells must fail explicitly.

use elephc_semantics::{SemanticProfile, StrictFeature};

/// Selects the guarantee that cannot delegate an unresolved target to a permissive shell.
pub(crate) fn unresolved_invocation_feature(profile: SemanticProfile, user_source: bool) -> Option<StrictFeature> {
    if profile.contains(StrictFeature::ArrayKeys) { Some(StrictFeature::ArrayKeys) }
    else if user_source && profile.contains(StrictFeature::Nullability) { Some(StrictFeature::Nullability) }
    else { None }
}

/// Keeps direct diagnostics and callable fallback errors consistent about the supported subset.
pub(crate) fn unresolved_invocation_message(feature: StrictFeature, class: &str) -> String {
    format!("--{} requires a known {class} invocation target; targets selected at run time or lost at control flow joins are unsupported", feature.flag())
}
