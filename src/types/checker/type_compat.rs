//! Purpose:
//! Implements assignability and compatibility rules for `PhpType` values.
//! Delegates object, declaration, pointer, and union-specific checks to focused helpers.
//!
//! Called from:
//! - `crate::types::checker::Checker`
//! - `crate::types::traits`
//!
//! Key details:
//! - Compatibility must be conservative for Mixed, unions, nullable values, inheritance, and pointer-like extensions.

mod declarations;
mod object_types;
mod pointers;
mod unions;
