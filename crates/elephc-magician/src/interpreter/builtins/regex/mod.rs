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
mod mb_ereg_match;
mod preg_match;
mod preg_match_all;
mod pattern;
mod preg_replace;
mod preg_replace_callback;
mod replacement;
mod preg_split;
mod split_helpers;
mod targets;

pub(in crate::interpreter) use captures::*;
pub(in crate::interpreter) use engine::*;
pub(in crate::interpreter) use mb_ereg_match::*;
pub(in crate::interpreter) use preg_match::*;
pub(in crate::interpreter) use preg_match_all::*;
pub(in crate::interpreter) use pattern::*;
pub(in crate::interpreter) use preg_replace::*;
pub(in crate::interpreter) use preg_replace_callback::*;
pub(in crate::interpreter) use replacement::*;
pub(in crate::interpreter) use preg_split::*;
pub(in crate::interpreter) use split_helpers::*;
pub(in crate::interpreter) use targets::*;
