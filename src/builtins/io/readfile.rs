//! Purpose:
//! Home of the PHP `readfile` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `normalize_union_type([Int, Bool])` reflecting PHP behaviour
//!   where `readfile` outputs the file and returns the byte count or `false` on
//!   failure. A check hook is required because the union return cannot be expressed
//!   through the scalar `returns:` field.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "readfile",
    area: Io,
    params: [filename: Str],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Readfile,
    ),
    summary: "Outputs a file.",
    php_manual: "function.readfile",
}

/// Returns `Union(Int, Bool)` reflecting the byte count on success or `false` on failure.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    Ok(cx.checker.normalize_union_type(vec![PhpType::Int, PhpType::False]))
}
