//! Purpose:
//! Home of the `buffer_len` builtin (elephc extension): its declaration, checker contract, and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the argument is a `buffer<T>` and returns `PhpType::Int`,
//!   preserving the legacy checker-resident arm's message verbatim.
//! - `extension: true`: buffers have no PHP equivalent, so `--strict-php` hides this
//!   builtin from user programs.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "buffer_len",
    area: Pointers,
    params: [buffer: Mixed],
    returns: Int,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::BufferLen,
    ),
    summary: "Returns the logical element count of a buffer<T>.",
    extension: true,
}

/// Validates that the argument is a `buffer<T>` and returns `PhpType::Int`.
///
/// The registry's `check_arity` handles arity enforcement (exactly 1 argument).
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(ty, PhpType::Buffer(_)) {
        return Err(CompileError::new(
            cx.span,
            "buffer_len() argument must be buffer<T>",
        ));
    }
    Ok(PhpType::Int)
}
