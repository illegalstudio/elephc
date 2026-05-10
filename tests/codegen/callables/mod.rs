//! Purpose:
//! Groups the callables integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for closures, expr calls, language features, constants and system, state and variadics.

mod closures;
mod expr_calls;
mod language_features;
mod constants_and_system;
mod state_and_variadics;
