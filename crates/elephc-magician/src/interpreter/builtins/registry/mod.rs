//! Purpose:
//! Groups builtin registry lookup, argument binding, callable dispatch, and
//! evaluated-argument builtin dispatch.
//!
//! Called from:
//! - `crate::interpreter::builtins` re-exports used by core eval call paths.
//!
//! Key details:
//! - The large by-value dispatch match is isolated from argument planning and
//!   callable normalization.

mod binding;
mod callable;
mod dispatch;
mod names;

pub(in crate::interpreter) use binding::*;
pub(in crate::interpreter) use callable::*;
pub(in crate::interpreter) use dispatch::*;
pub(in crate::interpreter) use names::*;
