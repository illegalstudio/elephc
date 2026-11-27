//! Purpose:
//! Binds the shared `posix_isatty` contract to its typed terminal-probe EIR operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - Accepts PHP's `int|resource` operand: streams are probed through their native descriptor.
//! - The check hook replaces the PCNTL scalar validator so stream resources are admitted.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "posix_isatty",
    check: check,
    semantics: crate::builtins::semantics::pcntl_semantics(crate::ir::PcntlRuntime::PosixIsatty),
}

/// Admits integer-like scalars, stream resources and boxed values, returning `Bool`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let actual = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if matches!(
        actual.codegen_repr(),
        PhpType::Int
            | PhpType::Bool
            | PhpType::Void
            | PhpType::Float
            | PhpType::TaggedScalar
            | PhpType::Str
            | PhpType::Mixed
            | PhpType::Union(_)
            | PhpType::Resource(_)
    ) {
        return Ok(PhpType::Bool);
    }
    Err(CompileError::new(
        cx.args[0].span,
        &format!(
            "{}() expects parameter 1 to be of type int|resource, got {}",
            cx.name, actual
        ),
    ))
}
