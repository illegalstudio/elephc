//! Purpose:
//! Groups the generator integration-test modules into a single namespace
//! under `tests::codegen::generators`. Each submodule covers a distinct
//! feature axis so failures localise quickly.
//!
//! Called from:
//!  - `cargo test` through the integration test harness via
//!    `tests/codegen/mod.rs`.
//!
//! Key details:
//!  - Submodules are intentionally split by generator behavior so focused
//!    regressions can run by module name during development.

mod arithmetic;
mod basic;
mod control_flow;
mod eval_bridge;
mod get_return;
mod generator_identity;
mod interop;
mod keys;
mod methods;
mod send_throw;
mod yield_from;
