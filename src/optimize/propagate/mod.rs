//! Purpose:
//! Runs scalar constant propagation across statements and control-flow joins.
//! Coordinates expression substitution, write invalidation, branch simulation, and statement environment tracking.
//!
//! Called from:
//! - `crate::optimize::propagate_constants()`
//!
//! Key details:
//! - Environments merge conservatively across branches, loops, try/catch/finally, and unknown writes.

use super::*;

mod expr;
mod simulate;
mod stmt;
mod writes;

pub(crate) use expr::*;
pub(crate) use simulate::*;
pub(crate) use stmt::*;
pub(crate) use writes::*;
