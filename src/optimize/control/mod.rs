//! Purpose:
//! Groups optimizer control-flow normalization, pruning, CFG, and DCE helpers.
//! Provides shared utilities for paths, switches, if-chains, and terminal-flow reasoning.
//!
//! Called from:
//! - `crate::optimize`
//!
//! Key details:
//! - Control rewrites must preserve PHP evaluation order, fallthrough, break/continue depth, and finally semantics.

use super::*;

// Shadows the glob-imported `crate::optimize::binding_decisions` (the thread-local decision SETS)
// with the control-flow walker that answers "does this subtree carry one of those decisions?".
// Both DCE tail-sinking and the single-case switch rewrite clone AST nodes, so both ask it.
mod binding_decisions;
mod common;
mod cfg;
mod dce;
mod fold;
mod if_chain;
mod loops;
mod path;
mod prune;
mod switch;
mod terminators;
pub(crate) use terminators::block_continues_enclosing_loop;

pub(crate) use binding_decisions::{
    expr_carries_local_binding_decision, stmts_carry_local_binding_decision,
};
pub(crate) use common::*;
pub(crate) use cfg::*;
pub(crate) use dce::*;
pub(crate) use fold::*;
pub(crate) use if_chain::*;
pub(crate) use loops::*;
pub(crate) use path::*;
pub(crate) use prune::*;
pub(crate) use switch::*;
pub(crate) use terminators::*;
