//! Purpose:
//! Groups the optimizer, dead-code elimination guards integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for outer guards, excluded guards, composite guards.

use super::*;

#[path = "guards/outer_guards.rs"]
mod outer_guards;
#[path = "guards/excluded_guards.rs"]
mod excluded_guards;
#[path = "guards/composite_guards.rs"]
mod composite_guards;
