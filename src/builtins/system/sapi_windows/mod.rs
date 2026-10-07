//! Purpose:
//! Groups the one-home-per-builtin implementations of PHP's Windows SAPI functions.
//!
//! Called from:
//! - `crate::builtins::system` during builtin inventory registration.
//!
//! Key details:
//! - Each child module owns exactly one shared-contract binding so generated docs can trace it.

mod cp_conv;
mod cp_get;
mod cp_is_utf8;
mod cp_set;
mod generate_ctrl_event;
mod set_ctrl_handler;
mod vt100_support;
