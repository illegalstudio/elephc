//! Purpose:
//! Groups the optimizer, dead-code elimination tries integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for try pruning, catch pruning, finally paths, try inlining, tail paths.

use super::*;

#[path = "tries/try_pruning.rs"]
mod try_pruning;
#[path = "tries/catch_pruning.rs"]
mod catch_pruning;
#[path = "tries/finally_paths.rs"]
mod finally_paths;
#[path = "tries/try_inlining.rs"]
mod try_inlining;
#[path = "tries/tail_paths.rs"]
mod tail_paths;
