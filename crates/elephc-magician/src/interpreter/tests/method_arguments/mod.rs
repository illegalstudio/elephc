//! Purpose:
//! Organizes interpreter tests for eval and runtime method argument binding by
//! defaults, references, types, and fallback surface.
//!
//! Called from:
//! - `crate::interpreter::tests` through Rust's test harness.
//!
//! Key details:
//! - Child modules preserve every original test function name.

mod binding_defaults;
mod by_reference;
mod runtime_fallback;
mod types_errors;
