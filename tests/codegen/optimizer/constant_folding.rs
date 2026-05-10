//! Purpose:
//! Groups the optimizer constant folding integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for expressions, pruning.

use super::*;

#[path = "constant_folding/expressions.rs"]
mod expressions;
#[path = "constant_folding/pruning.rs"]
mod pruning;
