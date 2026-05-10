//! Purpose:
//! Groups the type-related builtins integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for strict comparison semantics, includes, division, float checking builtins.

use crate::support::*;

#[path = "type_builtins/strict_comparison.rs"]
mod strict_comparison;
#[path = "type_builtins/includes/mod.rs"]
mod includes;
#[path = "type_builtins/division.rs"]
mod division;
#[path = "type_builtins/float_checks.rs"]
mod float_checks;
