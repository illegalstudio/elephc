//! Purpose:
//! Organizes parser tests for class-like declarations, members, attributes,
//! hooks, traits, and diagnostics by syntax responsibility.
//!
//! Called from:
//! - `crate::parser::tests` through Rust's test harness.
//!
//! Key details:
//! - Child modules preserve every original test function name.

mod attributes;
mod declarations;
mod interfaces_properties;
mod methods_traits_errors;
mod visibility_hooks;
