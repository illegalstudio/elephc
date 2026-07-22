//! Purpose:
//! Home of the PHP `ob_get_flush` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Captures the contents, then flushes them to the parent sink and pops the buffer.
//! - `check` returns `Union(Str, False)`: the captured contents, or `false` when
//!   no output buffer is active.
//! - The typed runtime target marks both result branches as caller-owned fresh boxes.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "ob_get_flush",
    area: Io,
    params: [],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ObGetFlush,
    ),
    summary: "Flushes the output buffer, returns it as a string and turns off output buffering.",
    php_manual: "function.ob-get-flush",
}

/// Returns `Union(Str, False)`: the buffered bytes on success, `false` when no
/// output buffer is active.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(cx.checker.normalize_union_type(vec![PhpType::Str, PhpType::False]))
}
