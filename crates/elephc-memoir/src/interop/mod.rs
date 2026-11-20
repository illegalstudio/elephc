//! Purpose:
//! Collects memory storage helpers used at native interoperability boundaries.
//!
//! Called from:
//! - Compiler pointer and zval runtime orchestration.
//!
//! Key details:
//! - These emit assembly inside the compiler and add no target-program library.

pub mod c_strings;
pub mod exported_bytes;
pub mod glob_storage;
pub mod zval;

pub mod cstr_scratch;
pub mod replacement_owners;

mod native_array_slots;
pub mod native_variable;
