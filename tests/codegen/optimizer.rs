//! Purpose:
//! Groups the optimizer integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for constant folding, constant propagation, dead-code elimination, and EIR identity arithmetic folding.

use crate::support::*;

#[path = "optimizer/constant_folding.rs"]
mod constant_folding;
#[path = "optimizer/constant_propagation.rs"]
mod constant_propagation;
#[path = "optimizer/dead_code_elimination.rs"]
mod dead_code_elimination;
#[path = "optimizer/identity_arithmetic.rs"]
mod identity_arithmetic;
