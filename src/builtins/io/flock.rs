//! Purpose:
//! Home of the PHP `flock` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates the stream resource and checks that `operation` is strictly `Int`
//!   (not just accepts_int), matching the legacy behaviour exactly.
//! - `would_block` is declared `ref(Int)`: PHP writes `0`/`1` into it, so the caller may pass it
//!   undeclared and the declaration is what requires a variable there.
//! - Arguments are pre-inferred by the registry before the hook runs, except `would_block`, which
//!   is written rather than read; `operation` is re-inferred inside the hook for validation.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "flock",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Flock,
    ),
}

/// Validates the stream resource and enforces a strict `Int` type for `operation`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    let op_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;  // re-infer to obtain the type
    if op_ty != PhpType::Int {                                  // STRICT eq (not accepts_int)
        return Err(CompileError::new(
            cx.args[1].span,
            "flock() operation must be int",
        ));
    }
    Ok(PhpType::Bool)
}
