//! Purpose:
//! Groups numeric formatting, printf-family, scanf, and math wrapper eval builtins.
//! Submodules are split by builtin family and shared formatting helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins` re-exports used by core call dispatch.
//!
//! Key details:
//! - Helpers stay scoped to the eval interpreter and preserve PHP-visible runtime
//!   behavior through `RuntimeValueOps`.

mod common;
mod dispatch;
mod math;
mod number_format;
mod printf;
mod sprintf_format;
mod sscanf;

pub(in crate::interpreter) use common::*;
pub(in crate::interpreter) use dispatch::*;
pub(in crate::interpreter) use math::*;
pub(in crate::interpreter) use number_format::*;
pub(in crate::interpreter) use printf::*;
pub(in crate::interpreter) use sprintf_format::*;
pub(in crate::interpreter) use sscanf::*;
