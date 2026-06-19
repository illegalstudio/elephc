//! Purpose:
//! Groups the optimizer integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for constant folding, constant propagation, dead-code elimination, EIR identity arithmetic folding, EIR peephole patterns, EIR dead instruction elimination, EIR dead store elimination, and EIR branch simplification.

use crate::support::*;

#[path = "optimizer/branch_simplification.rs"]
mod branch_simplification;
#[path = "optimizer/constant_folding.rs"]
mod constant_folding;
#[path = "optimizer/constant_propagation.rs"]
mod constant_propagation;
#[path = "optimizer/dead_code_elimination.rs"]
mod dead_code_elimination;
#[path = "optimizer/dead_instruction_elimination.rs"]
mod dead_instruction_elimination;
#[path = "optimizer/dead_store_elimination.rs"]
mod dead_store_elimination;
#[path = "optimizer/identity_arithmetic.rs"]
mod identity_arithmetic;
#[path = "optimizer/peephole.rs"]
mod peephole;
