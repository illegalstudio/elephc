//! Purpose:
//! Groups the object-oriented PHP callables integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for functions and builtins, methods, variadics.

use super::*;

mod functions_and_builtins;
mod methods;
mod variadics;
