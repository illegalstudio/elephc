//! Purpose:
//! Unit tests for Phase 06 IR-level passes, built on hand-constructed EIR.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Functions are built with `crate::ir::Builder` so the tests exercise the
//!   real IR data model without going through AST lowering.

mod branch_simplify_test;
mod const_fold_test;
mod cse_test;
mod dead_inst_test;
mod dead_store_test;
mod dominance_test;
mod driver_test;
mod identity_arith_test;
mod inline_test;
mod intervals_test;
mod licm_test;
mod liveness_test;
mod loops_test;
mod peephole_test;
mod regalloc_test;
