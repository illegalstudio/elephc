//! Purpose:
//! Organizes parser tests for static calls, receivers, dynamic member names, and
//! static-property mutations.
//!
//! Called from:
//! - `crate::parser::tests` through Rust's test harness.
//!
//! Key details:
//! - Child modules preserve every original test function name.

mod calls_receivers;
mod dynamic_names;
mod property_mutations;
