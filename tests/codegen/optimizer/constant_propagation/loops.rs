//! Purpose:
//! Groups the optimizer, constant propagation loops integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for for loops, while loops, foreach loops, loop-carried state.

use super::*;

// Tests for constant propagation through `for` loops where the loop body may assign a
// constant that is used after the loop, or where the loop init/update allows folding.
#[path = "loops/for_loops.rs"]
mod for_loops;
// Tests for constant propagation through `while` and `do/while` loops where a
// `break` or loop condition determines what constant value flows out.
#[path = "loops/while_loops.rs"]
mod while_loops;
// Tests for constant propagation through `foreach` loops where the iterator
// does not modify a scalar that is used after the loop.
#[path = "loops/foreach_loops.rs"]
mod foreach_loops;
// Tests for constant propagation when a loop contains nested control structures
// (switch, try/catch, inner loops) and an unrelated scalar outside the loop
// must be preserved across loop iterations.
#[path = "loops/loop_state.rs"]
mod loop_state;
