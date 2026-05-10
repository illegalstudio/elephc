//! Purpose:
//! Groups the control flow integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for branches and loops, functions, booleans, assignments, nulls, and related suites.

use crate::support::*;

#[path = "control_flow/branches_and_loops.rs"]
mod branches_and_loops;
#[path = "control_flow/functions.rs"]
mod functions;
#[path = "control_flow/booleans.rs"]
mod booleans;
#[path = "control_flow/assignments/mod.rs"]
mod assignments;
#[path = "control_flow/nulls.rs"]
mod nulls;
#[path = "control_flow/ternary.rs"]
mod ternary;
#[path = "control_flow/closures.rs"]
mod closures;
