//! Purpose:
//! Home of the PHP `ob_get_contents` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Read-only query: the buffer stays active and untouched.
//! - `check` returns `Union(Str, False)`: the captured contents, or `false` when
//!   no output buffer is active.
//! - The typed runtime target marks both result branches as caller-owned fresh boxes.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "ob_get_contents",
    area: Io,
    params: [],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ObGetContents,
    ),
    summary: "Returns the contents of the output buffer.",
    php_manual: "function.ob-get-contents",
}

/// Returns `Union(Str, False)`: the buffered bytes on success, `false` when no
/// output buffer is active.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(cx.checker.normalize_union_type(vec![PhpType::Str, PhpType::False]))
}
