//! Purpose:
//! Groups PHP array and iterator builtins implemented by eval.
//! Submodules separate pure construction, mutating by-reference calls, sorting,
//! splicing, filtering, and key/value access helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins` array-related dispatch.
//!
//! Key details:
//! - Helpers preserve PHP key normalization and use `RuntimeValueOps` for all
//!   runtime cell allocation and coercion.

mod access;
mod callbacks;
mod core;
mod filters;
mod mutation;
mod push_pop;
mod sort;
mod splice;

pub(in crate::interpreter) use access::*;
pub(in crate::interpreter) use callbacks::*;
pub(in crate::interpreter) use core::*;
pub(in crate::interpreter) use filters::*;
pub(in crate::interpreter) use mutation::*;
pub(in crate::interpreter) use push_pop::*;
pub(in crate::interpreter) use sort::*;
pub(in crate::interpreter) use splice::*;
