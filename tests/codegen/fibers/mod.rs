//! Purpose:
//! Groups the fibers integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for basics, errors, arguments, captures, scenarios, and
//!   user methods named like Fiber state predicates.

use crate::support::*;

mod basics;
mod errors;
mod arguments;
mod captures;
mod scenarios;
mod predicate_names;
