//! Purpose:
//! Home of the `buffer_ptr` builtin (elephc extension): its declaration, checker contract, and
//! semantic target. It answers the address of a buffer's payload, so a scalar or width buffer
//! (`buffer<u16>`, `buffer<f32>`) can be handed to C like a buffer of packed records.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the argument is a `buffer<T>` and returns `ptr<T>` for a buffer of
//!   packed records (the address of element 0, which `$buf[0]` reads) and the untyped `ptr` for
//!   every scalar, width, and pointer element type.
//! - The address is the payload the descriptor owns at that moment: it stays valid until
//!   `buffer_free()`, and a stale handle reports the use-after-free diagnostic.
//! - `extension: true`: buffers have no PHP equivalent, so `--strict-php` hides this
//!   builtin from user programs.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "buffer_ptr",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::BufferPtr,
    ),
}

/// Validates that the argument is a `buffer<T>` and returns the payload pointer type:
/// `ptr<T>` for a packed record element type, the untyped `ptr` for every other element.
///
/// The registry's `check_arity` handles arity enforcement (exactly 1 argument).
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let PhpType::Buffer(elem_ty) = ty else {
        return Err(CompileError::new(
            cx.span,
            "buffer_ptr() argument must be buffer<T>",
        ));
    };
    Ok(match *elem_ty {
        PhpType::Packed(name) => PhpType::Pointer(Some(name)),
        _ => PhpType::Pointer(None),
    })
}
