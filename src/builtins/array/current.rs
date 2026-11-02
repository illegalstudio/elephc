//! Purpose:
//! Home of the PHP `current` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Returns `false` once the internal pointer has run past either end of the array.
//! - Plain locals retain their cursor between calls; other array expressions use the shared
//!   internal-pointer lowering contract in `crate::builtins::array::internal_pointer`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "current",
    check: check,
    semantics: crate::builtins::semantics::array_pointer_semantics(
        crate::builtins::semantics::ArrayPointerOp::Current,
        crate::ir::RuntimeFnId::ArrayPtrValue,
    ),
}

/// Validates the receiver shape and type for `current()` and returns `Mixed`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    super::internal_pointer::check_array_pointer_call(cx, "current")
}
