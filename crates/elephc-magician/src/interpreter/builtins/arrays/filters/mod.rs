//! Purpose:
//! Groups array filtering, slicing, projection, iterator, and reversal eval
//! builtins by focused operation family.
//!
//! Called from:
//! - `crate::interpreter::builtins::arrays` re-exports.
//!
//! Key details:
//! - Helpers preserve PHP key normalization and iteration order through
//!   `RuntimeValueOps`.

mod chunk;
mod filter;
mod flip;
mod iterator;
mod pad;
mod projection;
mod reverse;
mod slice;
mod unique;

pub(in crate::interpreter) use chunk::*;
pub(in crate::interpreter) use filter::*;
pub(in crate::interpreter) use flip::*;
pub(in crate::interpreter) use iterator::*;
pub(in crate::interpreter) use pad::*;
pub(in crate::interpreter) use projection::*;
pub(in crate::interpreter) use reverse::*;
pub(in crate::interpreter) use slice::*;
pub(in crate::interpreter) use unique::*;
