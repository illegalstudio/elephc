//! Purpose:
//! Regression tests for optimizer dce switches behavior over parser AST fixtures.
//! Documents the pass contracts that must survive control-flow, effect, and scalar rewrites.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Fixtures are intentionally small and structural; expected AST equality captures observable optimizer semantics.

use super::*;

mod basics;
mod case_shadowing;
mod guarded_cases;
mod exhaustive_suffixes;
mod tail_paths;
