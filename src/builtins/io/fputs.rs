//! Purpose:
//! Home of the PHP `fputs` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `fputs` is an alias for `fwrite`; both share the same runtime target and the same `check`
//!   hook, so the stream argument is validated and the result is PHP's `int|false` union.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "fputs",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Fwrite,
    ),
}

/// Validates the stream argument and returns PHP's `int|false` result union.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    Ok(cx
        .checker
        .normalize_union_type(vec![PhpType::Int, PhpType::Bool]))
}
