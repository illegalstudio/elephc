//! Purpose:
//! Groups the expression modern PHP operators integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for logical and instanceof, assignment, ternary and null coalesce.

use super::*;

mod logical_and_instanceof;
mod assignment;
mod ternary_and_null_coalesce;
