//! Purpose:
//! Groups the optimizer constant propagation integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for straight-line programs, branching control paths, collections, loops.

use super::*;

#[path = "constant_propagation/straight_line.rs"]
mod straight_line;
#[path = "constant_propagation/control_paths.rs"]
mod control_paths;
#[path = "constant_propagation/collections.rs"]
mod collections;
#[path = "constant_propagation/loops.rs"]
mod loops;
