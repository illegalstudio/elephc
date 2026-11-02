//! Purpose:
//! Home of the PHP `key` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Returns `null` once the internal pointer has run past either end of the array.
//! - Plain locals retain their cursor between calls; other array expressions use the shared
//!   internal-pointer lowering contract in `crate::builtins::array::internal_pointer`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "key",
    check: check,
    semantics: crate::builtins::semantics::array_pointer_semantics(
        crate::builtins::semantics::ArrayPointerOp::Key,
        crate::ir::RuntimeFnId::ArrayPtrKey,
    ),
}

/// Validates the receiver shape and type for `key()` and returns `Mixed`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    super::internal_pointer::check_array_pointer_call(cx, "key")
}
