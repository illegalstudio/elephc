//! Purpose:
//! Runs constant propagation across statements and control-flow joins: scalar
//! facts plus array-literal facts for heap-backed locals (COW value-semantics
//! snapshots). Coordinates expression substitution, targeted write
//! invalidation, branch simulation, and statement environment tracking.
//!
//! Called from:
//! - `crate::optimize::propagate_constants()`
//!
//! Key details:
//! - Environments merge conservatively across branches, loops, try/catch/finally, and unknown writes.

use super::*;

mod expr;
mod invalidation;
mod signatures;
mod simulate;
mod stmt;
mod writes;

pub(crate) use expr::*;
pub(crate) use invalidation::*;
pub(crate) use signatures::*;
pub(crate) use simulate::*;
pub(crate) use stmt::*;
pub(crate) use writes::*;
