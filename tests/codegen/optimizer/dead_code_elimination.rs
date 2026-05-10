//! Purpose:
//! Groups the optimizer dead-code elimination integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for basics, tries, switches, guards, tail sinking, and related suites.

use super::*;

#[path = "dead_code_elimination/basics.rs"]
mod basics;
#[path = "dead_code_elimination/tries.rs"]
mod tries;
#[path = "dead_code_elimination/switches.rs"]
mod switches;
#[path = "dead_code_elimination/guards.rs"]
mod guards;
#[path = "dead_code_elimination/tail_sinking.rs"]
mod tail_sinking;
#[path = "dead_code_elimination/normalization.rs"]
mod normalization;
