//! Purpose:
//! Home of the PHP `boolval` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Checked conversions reuse native boolean cast handlers; permissive truthiness stays intact.

use crate::builtins::semantics::BuiltinResultOwnership;
use crate::parser::ast::CastType;

builtin! {
    contract: "boolval",
    semantics: super::scalar_conversion::semantics(CastType::Bool, BuiltinResultOwnership::NonHeap),
}
