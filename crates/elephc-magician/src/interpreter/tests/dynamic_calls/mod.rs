//! Purpose:
//! Organizes interpreter tests for dynamic callable dispatch by callable form
//! and runtime bridge surface.
//!
//! Called from:
//! - `crate::interpreter::tests` through Rust's test harness.
//!
//! Key details:
//! - Child modules preserve every original test and helper function name.

mod call_user_func;
mod call_user_func_array;
mod first_class_objects;
mod runtime_callables;
