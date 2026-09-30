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
    contract: "readfile",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Readfile,
    ),
    // The same reader, so the same libraries: a `compress.*://` filename links the compression
    // library it decodes with, exactly as `file_get_contents()` does for the same URL.
    requirements: crate::builtins::semantics::file_get_contents_requirements,
}

/// Returns `Union(Int, Bool)` reflecting the byte count on success or `false` on failure.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    Ok(cx.checker.normalize_union_type(vec![PhpType::Int, PhpType::False]))
}