//! Purpose:
//! Groups eval registry entries and dispatch wrappers for JSON builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins` module loading.
//!
//! Key details:
//! - Leaf files register metadata through `eval_builtin!` and own their
//!   PHP-visible direct/by-value wrappers and implementation code.
//! - Helper reuse stays between builtin files instead of an area-level
//!   implementation module when one builtin naturally owns the behavior.

mod json_decode;
mod json_encode;
mod json_last_error;
mod json_last_error_msg;
mod json_validate;

pub(in crate::interpreter) use json_decode::*;
pub(in crate::interpreter) use json_encode::*;
pub(in crate::interpreter) use json_last_error::*;
pub(in crate::interpreter) use json_last_error_msg::*;
pub(in crate::interpreter) use json_validate::*;
