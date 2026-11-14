//! Purpose:
//! Owns the internal capture-event builtin used by the debug-only PHP helpers.
//!
//! Called from:
//! - `crate::builtins` through the inventory registry.
//!
//! Key details:
//! - The public helpers are declared by `crate::debug_prelude`; this module is the
//!   single internal function that reaches the native event recorder.
//! - `internal: true`: never a PHP-visible builtin.

pub(crate) mod event;
