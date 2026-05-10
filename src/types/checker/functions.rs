//! Purpose:
//! Type-checks function declarations, calls, and function-like control-flow contracts.
//! Connects call resolution, shared argument validation, return analysis, and inferred function metadata.
//!
//! Called from:
//! - `crate::types::checker::driver::functions`
//! - `crate::types::checker::inference`
//!
//! Key details:
//! - User functions, builtins, externs, and callable aliases must share the same argument semantics.

mod call_validation;
mod resolution;
mod returns;
