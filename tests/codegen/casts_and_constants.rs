//! Purpose:
//! Groups the casts, constants, and introspection integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for casts, introspection, predicates, math builtins, constants.

use crate::support::*;

#[path = "casts_and_constants/casts.rs"]
mod casts;
#[path = "casts_and_constants/introspection.rs"]
mod introspection;
#[path = "casts_and_constants/predicates.rs"]
mod predicates;
#[path = "casts_and_constants/math_builtins.rs"]
mod math_builtins;
#[path = "casts_and_constants/constants.rs"]
mod constants;
