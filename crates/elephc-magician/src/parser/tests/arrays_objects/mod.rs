//! Purpose:
//! Organizes parser tests for arrays, object access/construction, calls, and
//! property mutations.
//!
//! Called from:
//! - `crate::parser::tests` through Rust's test harness.
//!
//! Key details:
//! - Child modules preserve every original test function name.

mod arrays_access;
mod construction_calls;
mod property_mutations;
