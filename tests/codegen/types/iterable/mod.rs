//! Purpose:
//! Groups the types iterable integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for foreach, builtins and casts.

use super::*;

mod foreach;
mod builtins_and_casts;
