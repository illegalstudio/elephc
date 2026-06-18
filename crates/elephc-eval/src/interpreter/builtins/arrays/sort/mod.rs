//! Purpose:
//! Groups eval array sorting builtins into direct-call binding, user-comparator
//! sorting, and standard sorting engines.
//!
//! Called from:
//! - `crate::interpreter::builtins::arrays` re-exports.
//!
//! Key details:
//! - Direct by-reference calls update scope cells, while dynamic by-value dispatch
//!   returns sorted replacement arrays plus PHP-compatible warnings.

mod direct;
mod standard;
mod user;

pub(in crate::interpreter) use direct::*;
pub(in crate::interpreter) use standard::*;
pub(in crate::interpreter) use user::*;
