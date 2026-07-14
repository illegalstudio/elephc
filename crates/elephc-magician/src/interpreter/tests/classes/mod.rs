//! Purpose:
//! Organizes interpreter tests for eval-declared class runtime behavior by
//! property, lifecycle, hook, and contract responsibility.
//!
//! Called from:
//! - `crate::interpreter::tests` through Rust's test harness.
//!
//! Key details:
//! - Child modules preserve the original test names for stable exact filtering.

mod asymmetric_properties;
mod attributes;
mod basics;
mod hook_contracts;
mod lifecycle;
mod magic_methods;
mod promoted_references;
mod property_hooks;
mod readonly;
