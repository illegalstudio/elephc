//! Purpose:
//! Groups string, hash, ctype, SPL, and stream-introspection eval builtins.
//! Submodules are split by PHP builtin family and re-exported for call dispatch.
//!
//! Called from:
//! - `crate::interpreter::builtins` re-exports used by core call dispatch.
//!
//! Key details:
//! - Helpers stay scoped to the eval interpreter and preserve PHP-visible runtime behavior.

mod ctype;
mod grapheme_strrev;
mod gzip;
mod hash;
mod hash_context;
mod html;
mod introspection;
mod nl2br;
mod pad;
mod repeat;
mod replace;
mod simple;
mod split;
mod substr;
mod url;

pub(in crate::interpreter) use ctype::*;
pub(in crate::interpreter) use grapheme_strrev::*;
pub(in crate::interpreter) use gzip::*;
pub(in crate::interpreter) use hash::*;
pub(in crate::interpreter) use hash_context::*;
pub(in crate::interpreter) use html::*;
pub(in crate::interpreter) use introspection::*;
pub(in crate::interpreter) use nl2br::*;
pub(in crate::interpreter) use pad::*;
pub(in crate::interpreter) use repeat::*;
pub(in crate::interpreter) use replace::*;
pub(in crate::interpreter) use simple::*;
pub(in crate::interpreter) use split::*;
pub(in crate::interpreter) use substr::*;
pub(in crate::interpreter) use url::*;
