//! Purpose:
//! Runs constant folding over expressions and declaration defaults.
//! Delegates scalar, cast, and operator evaluation while preserving PHP-visible runtime failures.
//!
//! Called from:
//! - `crate::optimize::fold_constants()`
//!
//! Key details:
//! - Only fold results that are unambiguous PHP equivalents; division by zero and effectful expressions must remain runtime behavior.

mod casts;
mod expr;
mod ops;
mod scalar;

pub(super) use expr::{fold_enum_case, fold_expr, fold_method, fold_params, fold_property};
pub(super) use scalar::{assigned_scalar_value, scalar_value, ScalarValue};
