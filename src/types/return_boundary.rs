//! Purpose:
//! Shares the declared-int runtime return validation predicate.
//!
//! Called from:
//! - EIR return coercion, checker exception edges, and optimizer return-boundary facts.
//!
//! Key details:
//! - Inferred returns and native scalar sources do not acquire a declared boundary check.

use super::PhpType;

/// Identifies the boxed or floating sources verified by the declared-int return operation.
pub(crate) fn declared_int_return_requires_validation(
    declared: bool, expected: &PhpType, source: &PhpType,
) -> bool {
    declared && *expected == PhpType::Int
        && matches!(source.codegen_repr(), PhpType::Mixed | PhpType::Float)
}
