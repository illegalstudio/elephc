//! Purpose:
//! Groups the control flow assignments integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for compound and values, evaluation order, optimizer and closures.

use super::*;

mod compound_and_values;
mod evaluation_order;
mod optimizer_and_closures;
