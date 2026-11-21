//! Purpose:
//! Describes comparison-consumer failures independently of callback-body effects.
//!
//! Called from:
//! - Shared builtin effects and the AST optimizer's precise callback effect summaries.
//!
//! Key details:
//! - Neutral contract metadata selects comparator roles without a second builtin-name table.
//! - Trusted implementation calls retain their original PHP callback semantics.

use super::{BuiltinSemanticInput, Effects};
use crate::builtins::registry::BuiltinDef;

/// Retains original-kind comparator failures even when the selected callback body cannot throw.
pub fn comparison_boundary_effects(definition: &BuiltinDef, input: &BuiltinSemanticInput<'_>) -> Effects {
    if crate::semantics::comparisons::active(input.semantic_profile, input.source_mode)
        && definition.spec.comparison_policy().is_some_and(|policy| policy.checks_callback_result())
    { Effects::MAY_THROW } else { Effects::PURE }
}
