//! Purpose:
//! Collects memory storage helpers used at native interoperability boundaries.
//!
//! Called from:
//! - Compiler pointer and zval runtime orchestration.
//!
//! Key details:
//! - These emit assembly inside the compiler and add no target-program library.

pub mod c_strings;
pub mod zval;

pub mod cstr_scratch;
