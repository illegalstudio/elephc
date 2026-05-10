//! Purpose:
//! Groups the optimizer, dead-code elimination, tries try inlining integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for pure calls, callable aliases.

use super::*;

#[path = "try_inlining/pure_calls.rs"]
mod pure_calls;
#[path = "try_inlining/callable_aliases.rs"]
mod callable_aliases;
