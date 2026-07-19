//! Purpose:
//! Groups the optimizer integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for constant folding, constant propagation, dead-code elimination, EIR identity arithmetic folding, EIR peephole patterns, EIR dead instruction elimination, EIR dead store elimination, EIR branch simplification, EIR constant folding, EIR common-subexpression elimination, EIR loop-invariant code motion, ownership cleanup, and memory-model-aware propagation hazards.

use crate::support::*;

#[path = "optimizer/branch_simplification.rs"]
mod branch_simplification;
#[path = "optimizer/call_result_alias.rs"]
mod call_result_alias;
#[path = "optimizer/constant_folding.rs"]
mod constant_folding;
#[path = "optimizer/constant_propagation.rs"]
mod constant_propagation;
#[path = "optimizer/eir_common_subexpression.rs"]
mod eir_common_subexpression;
#[path = "optimizer/dead_code_elimination.rs"]
mod dead_code_elimination;
#[path = "optimizer/dead_instruction_elimination.rs"]
mod dead_instruction_elimination;
#[path = "optimizer/dead_store_elimination.rs"]
mod dead_store_elimination;
#[path = "optimizer/eir_constant_propagation.rs"]
mod eir_constant_propagation;
#[path = "optimizer/eir_licm.rs"]
mod eir_licm;
#[path = "optimizer/identity_arithmetic.rs"]
mod identity_arithmetic;
#[path = "optimizer/peephole.rs"]
mod peephole;
#[path = "optimizer/property_receiver_ownership.rs"]
mod property_receiver_ownership;
#[path = "optimizer/read_result_cleanup.rs"]
mod read_result_cleanup;
#[path = "optimizer/release_local_slot.rs"]
mod release_local_slot;
#[path = "optimizer/inline.rs"]
mod inline;
#[path = "optimizer/memory_model_propagation.rs"]
mod memory_model_propagation;
