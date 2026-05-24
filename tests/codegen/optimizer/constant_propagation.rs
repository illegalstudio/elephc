//! Purpose:
//! Groups the optimizer constant propagation integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for straight-line programs, branching control paths, collections, loops.

use super::*;

// Tests for constant folding of expressions where all operands are known at compile time,
// e.g. `$x = 2; $y = 3; echo $x ** $y;` folding to a constant `8`.
#[path = "constant_propagation/straight_line.rs"]
mod straight_line;
// Tests for constant propagation through branching control structures that can merge
// identical constant assignments from different paths (if/else, ternary, match, switch, try/catch).
#[path = "constant_propagation/control_paths.rs"]
mod control_paths;
// Tests for constant propagation tracking scalar values through collection literals
// and list unpacking where all elements are compile-time constants.
#[path = "constant_propagation/collections.rs"]
mod collections;
// Tests for constant propagation through loop constructs where a variable written
// inside a loop can be treated as constant after the loop if all paths through the
// loop converge on the same constant value.
#[path = "constant_propagation/loops.rs"]
mod loops;
