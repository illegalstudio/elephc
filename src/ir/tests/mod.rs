//! Purpose:
//! Unit tests for the Phase 02 EIR data model, builder, validator, and printer.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Tests build EIR by hand because AST lowering is intentionally out of
//!   scope for Phase 02.

mod builder_test;
mod effects_test;
mod function_test;
mod local_load_ownership;
mod print_test;
mod types_test;
mod validator_test;
mod value_test;
