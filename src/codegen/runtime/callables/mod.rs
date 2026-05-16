//! Purpose:
//! Wires callable-introspection runtime helpers used by type builtins.
//! Keeps the helper surface narrow while data tables live in runtime/data.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()`.
//!
//! Key details:
//! - Helpers must match callable metadata tables for builtin functions, user functions, and class methods.

mod is_callable;

pub(crate) use is_callable::emit_is_callable_runtime;
