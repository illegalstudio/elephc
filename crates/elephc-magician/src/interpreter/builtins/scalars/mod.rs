//! Purpose:
//! Groups scalar, encoding, math, type, string-search, and trim/case eval builtins.
//! Each submodule owns one PHP builtin family while this module re-exports the callable surface.
//!
//! Called from:
//! - `crate::interpreter::builtins` re-exports used by core call dispatch.
//!
//! Key details:
//! - Helpers stay scoped to the eval interpreter and preserve PHP-visible runtime behavior.

mod base64;
mod common;
mod hex;
mod math;
mod search;
mod slashes;
mod trim_case;
mod types;

pub(in crate::interpreter) use base64::*;
pub(in crate::interpreter) use common::*;
pub(in crate::interpreter) use hex::*;
pub(in crate::interpreter) use math::*;
pub(in crate::interpreter) use search::*;
pub(in crate::interpreter) use slashes::*;
pub(in crate::interpreter) use trim_case::*;
pub(in crate::interpreter) use types::*;
