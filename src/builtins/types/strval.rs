//! Purpose:
//! Registers PHP's `strval` conversion with backend-neutral EIR semantics.
//!
//! Called from:
//! - The builtin registry through `crate::builtins::types`.
//!
//! Key details:
//! - Prepared lowering shares explicit-cast behavior and publishes an independent string owner.

use crate::builtins::semantics::BuiltinResultOwnership;
use crate::parser::ast::CastType;

builtin! {
    contract: "strval",
    semantics: super::scalar_conversion::semantics(CastType::String, BuiltinResultOwnership::Fresh),
}
