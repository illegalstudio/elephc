//! Purpose:
//! Groups the optimizer, dead-code elimination switches integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for switch case shadowing, guarded switch cases, exhaustive switch suffixes, normalization, tail paths.

use super::*;

#[path = "switches/case_shadowing.rs"]
mod case_shadowing;
#[path = "switches/guarded_cases.rs"]
mod guarded_cases;
#[path = "switches/exhaustive_suffixes.rs"]
mod exhaustive_suffixes;
#[path = "switches/normalization.rs"]
mod normalization;
#[path = "switches/tail_paths.rs"]
mod tail_paths;
