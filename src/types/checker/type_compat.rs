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
#[cfg(test)]
pub(crate) use object_types::SUBCLASS_QUERIES;
mod pointers;
pub(crate) mod unions;

pub(crate) use declarations::set_program_evaluates_code;
pub(crate) use object_types::type_is_gradual_object_family;
