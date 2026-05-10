//! Purpose:
//! Groups the expression parsing integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for basics, operators, modern PHP operators, assignments, arrays, string offsets, and match expressions.

use super::*;

#[path = "expressions/basics.rs"]
mod basics;
#[path = "expressions/operators.rs"]
mod operators;
#[path = "expressions/modern_ops/mod.rs"]
mod modern_ops;
#[path = "expressions/assignments.rs"]
mod assignments;
#[path = "expressions/arrays_match.rs"]
mod arrays_match;
