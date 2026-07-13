//! Purpose:
//! Organizes interpreter expression tests by scalar/object execution, class
//! behavior, visibility, and callable contracts.
//!
//! Called from:
//! - `crate::interpreter::tests` through Rust's test harness.
//!
//! Key details:
//! - Child modules preserve every original test function name.

mod classes_traits;
mod interface_contracts;
mod method_contracts;
mod scalars_objects;
mod visibility;
