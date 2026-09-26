//! Purpose:
//! Groups the control flow integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for branches and loops, alternative (`:` … `endif;`)
//!   syntax, functions, booleans, assignments, nulls, guarded reassignment, and related suites.

use crate::support::*;

#[path = "control_flow/alternative_syntax.rs"]
mod alternative_syntax;
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
#[path = "control_flow/match_expressions.rs"]
mod match_expressions;
#[path = "control_flow/closures.rs"]
mod closures;
#[path = "control_flow/guarded_reassignment.rs"]
mod guarded_reassignment;
#[path = "control_flow/branch_join_locals.rs"]
mod branch_join_locals;
