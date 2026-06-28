//! Purpose:
//! Groups PCRE-style preg eval builtins by entrypoint and shared helper domain.
//! Submodules keep `preg_match`, `preg_replace`, and `preg_split` behavior
//! separated while this module re-exports the interpreter-visible surface.
//!
//! Called from:
//! - `crate::interpreter::builtins` re-exports used by core call dispatch.
//!
//! Key details:
//! - Helpers stay scoped to the eval interpreter and preserve PHP-visible runtime
//!   behavior through `RuntimeValueOps`.

mod captures;
mod engine;
mod match_all;
mod match_one;
mod pattern;
mod replace;
mod replacement;
mod split;
mod split_helpers;
mod targets;

pub(in crate::interpreter) use captures::*;
pub(in crate::interpreter) use engine::*;
pub(in crate::interpreter) use match_all::*;
pub(in crate::interpreter) use match_one::*;
pub(in crate::interpreter) use pattern::*;
pub(in crate::interpreter) use replace::*;
pub(in crate::interpreter) use replacement::*;
pub(in crate::interpreter) use split::*;
pub(in crate::interpreter) use split_helpers::*;
pub(in crate::interpreter) use targets::*;
