//! Purpose:
//! Groups the types named arguments integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for direct calls and builtins, spread, variadics.

use super::*;

mod direct_and_builtins;
mod spread;
mod variadics;
