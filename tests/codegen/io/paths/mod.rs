//! Purpose:
//! Groups the I/O paths integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for basename and dirname builtins, fnmatch path matching, realpath and pathinfo builtins.

use super::*;

mod basename_dirname;
mod fnmatch;
mod realpath_pathinfo;
