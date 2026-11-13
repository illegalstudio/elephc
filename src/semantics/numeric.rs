//! Purpose:
//! Adapts checked numeric policy to compiler-owned source and type metadata.
//!
//! Called from:
//! - Arithmetic checking, EIR lowering and source validation.
//!
//! Key details:
//! - Trusted implementation statements retain their reviewed PHP arithmetic operations.
//! - User PHP and LFC operations inherit the same immutable compilation profile.

use crate::source::SourceMode;
use elephc_semantics::{arithmetic_policy::ArithmeticPolicy, SemanticProfile};

/// Resolves a source operation's policy without modifying the compilation's profile.
pub(crate) fn operation_policy(profile: SemanticProfile, source: SourceMode) -> ArithmeticPolicy {
    if source == SourceMode::Internal { ArithmeticPolicy::PERMISSIVE }
    else { ArithmeticPolicy::from_profile(profile) }
}
