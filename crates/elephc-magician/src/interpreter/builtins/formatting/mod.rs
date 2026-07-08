//! Purpose:
//! Groups numeric formatting, printf-family, and scanf eval builtins.
//! Submodules are split by builtin family and shared formatting helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins` re-exports used by core call dispatch.
//!
//! Key details:
//! - Helpers stay scoped to the eval interpreter and preserve PHP-visible runtime
//!   behavior through `RuntimeValueOps`.

mod common;
mod declarations;
mod dispatch;
mod number_format;
mod printf;
mod sprintf_format;
mod sscanf;

pub(in crate::interpreter) use common::*;
pub(in crate::interpreter) use dispatch::*;
pub(in crate::interpreter) use number_format::*;
pub(in crate::interpreter) use printf::*;
pub(in crate::interpreter) use sprintf_format::*;
pub(in crate::interpreter) use sscanf::*;
