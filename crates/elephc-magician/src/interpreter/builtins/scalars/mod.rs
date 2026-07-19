//! Purpose:
//! Groups scalar helper functions that are still shared by eval builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins` re-exports used by core call dispatch.
//!
//! Key details:
//! - PHP-visible string builtin implementations live in `builtins::string` leaf
//!   files; this module keeps cross-domain scalar helpers only.

mod common;
mod math;
mod types;

pub(in crate::interpreter) use common::*;
pub(in crate::interpreter) use math::*;
pub(in crate::interpreter) use types::*;
