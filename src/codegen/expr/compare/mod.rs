//! Purpose:
//! Groups comparison-specific expression helpers for strict comparison, casts, and null coalescing.
//! Keeps PHP comparison semantics separate from general binary operator dispatch.
//!
//! Called from:
//! - `crate::codegen::expr::binops` and `crate::codegen::expr::emit_expr()`
//!
//! Key details:
//! - Loose, strict, and null-sensitive paths have different runtime and register conventions.

mod casts;
mod null_coalesce;
mod strict;

pub(super) use casts::emit_cast;
pub(super) use null_coalesce::emit_null_coalesce;
pub(super) use strict::emit_strict_compare;
