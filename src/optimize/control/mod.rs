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

mod common;
mod cfg;
mod dce;
mod fold;
mod if_chain;
mod path;
mod prune;
mod switch;

pub(crate) use common::*;
pub(crate) use cfg::*;
pub(crate) use dce::*;
pub(crate) use fold::*;
pub(crate) use if_chain::*;
pub(crate) use path::*;
pub(crate) use prune::*;
pub(crate) use switch::*;
