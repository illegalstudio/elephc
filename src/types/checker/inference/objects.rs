//! Purpose:
//! Infers objects type-system behavior.
//! Converts AST forms into `PhpType` facts used by validation, warnings, and codegen metadata.
//!
//! Called from:
//! - `crate::types::checker::inference`
//!
//! Key details:
//! - PHP compatibility matters for coercions, operator results, object access, and nullable/union handling.

mod access;
mod constructors;
mod methods;
#[cfg(test)]
pub(crate) use methods::INTERFACE_IMPLEMENTATION_SIG_COPIES;
mod reflection_filters;

pub(crate) use methods::php_declared_type_spelling;
