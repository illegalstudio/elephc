//! Purpose:
//! Wires callable-introspection runtime helpers used by type builtins.
//! Keeps the helper surface narrow while data tables live in runtime/data.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//!
//! Key details:
//! - Helpers must match callable metadata tables for builtin functions, user functions, and class methods.

mod closure_bind;
mod closure_debug;
mod function_exists;
mod is_callable;
mod descriptor_release;

pub(crate) use closure_bind::emit_closure_bind;
pub(crate) use closure_debug::emit_closure_debug;
pub(crate) use descriptor_release::emit_callable_descriptor_release;
pub(crate) use function_exists::emit_function_exists_lookup;
pub(crate) use is_callable::emit_is_callable_runtime;
