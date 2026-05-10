//! Purpose:
//! Groups the optimizer, constant propagation loops integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for for loops, while loops, foreach loops, loop-carried state.

use super::*;

#[path = "loops/for_loops.rs"]
mod for_loops;
#[path = "loops/while_loops.rs"]
mod while_loops;
#[path = "loops/foreach_loops.rs"]
mod foreach_loops;
#[path = "loops/loop_state.rs"]
mod loop_state;
