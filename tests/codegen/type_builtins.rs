//! Purpose:
//! Groups the type-related builtins integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for strict comparison semantics, includes, division, float checking builtins.

use crate::support::*;

// Strict equality (`===`) and inequality (`!==`) between integers, strings, bools, null, and floats.
#[path = "type_builtins/strict_comparison.rs"]
mod strict_comparison;
// `include`/`require`/`include_once`/`require_once` with multi-file fixtures, declaration discovery, function variants, and path resolution.
#[path = "type_builtins/includes/mod.rs"]
mod includes;
// Division (`/`) returning float, `intdiv()` returning int, and division-by-zero producing INF/NAN.
#[path = "type_builtins/division.rs"]
mod division;
// `INF`, `NAN`, `-INF` constants and `is_nan()` / `is_infinite()` / `is_finite()` predicates.
#[path = "type_builtins/float_checks.rs"]
mod float_checks;
