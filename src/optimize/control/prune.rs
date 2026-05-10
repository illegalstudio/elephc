//! Purpose:
//! Prunes constant control-flow structures before broader normalization and DCE.
//! Dispatches expression and statement pruning for ifs, loops, switches, and empty effect-only shells.
//!
//! Called from:
//! - `crate::optimize::prune_constant_control_flow()`
//!
//! Key details:
//! - Pruning must retain condition/subject evaluation when expressions can have PHP-visible effects.

mod expr;
mod loop_exit;
mod statements;

pub(crate) use expr::{
    callable_target_effect, expr_has_side_effects, prune_expr,
};
pub(crate) use statements::prune_block;
