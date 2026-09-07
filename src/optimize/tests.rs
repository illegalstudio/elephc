//! Purpose:
//! Integration-style unit fixtures for optimizer passes over hand-built ASTs.
//! Provides shared imports and submodules for fold, propagate, prune, DCE, effects, and normalization tests.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Tests assert AST rewrites directly, so spans and statement ordering are part of the expected behavior.

use super::*;
use crate::names::Name;
use crate::parser::ast::{ClassProperty, StaticReceiver, Visibility};
use crate::span::Span;

mod effects;
mod propagate;
mod fold;
mod prune;
mod dce;
mod control;
mod normalize;
mod performance;
mod target_guards;

/// Runs DCE over a hand-built AST that no checker ever saw.
///
/// Shadows [`super::eliminate_dead_code`] for the test tree only. These fixtures build their ASTs
/// by hand, so there are no `CheckResult` local-binding decisions to keep singular and the empty
/// set is the honest argument — while the real callers keep having to pass
/// `CheckResult::local_binding_decision_spans()` explicitly, which is what stops a production
/// caller from quietly losing the tail-sinking guard.
fn eliminate_dead_code(program: Program) -> Program {
    super::eliminate_dead_code(program, HashSet::new())
}

/// Prunes constant control flow over a hand-built AST that no checker ever saw.
///
/// Shadows [`super::prune_constant_control_flow`] for the test tree only, on the same reasoning as
/// `eliminate_dead_code` above: with no `CheckResult` there is no decision the single-case switch
/// rewrite could clone, so the empty set is the honest argument. The fixtures that DO exercise the
/// rewrite's veto call `crate::optimize::normalize_control_flow` directly with a real span set.
fn prune_constant_control_flow(program: Program) -> Program {
    super::prune_constant_control_flow(program, HashSet::new())
}

/// Normalizes control flow over a hand-built AST that no checker ever saw.
///
/// Shadows [`super::normalize_control_flow`] for the test tree only, for the reason spelled out on
/// `prune_constant_control_flow` above.
fn normalize_control_flow(program: Program) -> Program {
    super::normalize_control_flow(program, HashSet::new())
}

/// Runs constant propagation over a hand-built AST that no checker ever saw.
///
/// Shadows [`super::propagate_constants`] for the test tree only, on the same reasoning as
/// `eliminate_dead_code` above: these fixtures have no `CheckResult`, so no local was marked as
/// boxed `mixed` storage and the empty set is the honest argument — while the real callers keep
/// having to pass `CheckResult::mixed_storage_local_names()` explicitly, which is what stops a
/// production caller from quietly substituting a literal into a boxed local's read.
///
/// The volatility fixtures that need a non-empty set call `PostTypecheckOptimizer::propagate`
/// directly instead of going through this.
fn propagate_constants(program: Program) -> Program {
    super::propagate_constants(program, HashSet::new(), HashSet::new())
}
