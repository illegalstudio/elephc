//! Purpose:
//! Groups the I/O builtin diagnostics integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for includes, streams, filesystem, paths, ownership and globals.

use super::*;

mod includes;
mod streams;
mod filesystem;
mod paths;
mod ownership_and_globals;
