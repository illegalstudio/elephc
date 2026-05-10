//! Purpose:
//! Groups the type-related builtins includes integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for basic, discovery, include-loaded function variants, include paths and errors.

use super::*;

mod basic;
mod discovery;
mod function_variants;
mod paths_and_errors;
