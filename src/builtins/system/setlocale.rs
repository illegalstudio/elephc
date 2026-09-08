//! Purpose:
//! Home of the PHP `setlocale` builtin: its declaration, result type, and runtime target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - PHP returns the selected locale string or `false`, so the backend result is a boxed `Mixed`.
//! - Locale candidates are evaluated in source order and the first libc-accepted value wins.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "setlocale",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Setlocale,
    ),
}

/// Infers every candidate and returns PHP's `string|false` result type.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    for arg in cx.args {
        cx.checker.infer_type(arg, cx.env)?;
    }
    Ok(cx
        .checker
        .normalize_union_type(vec![PhpType::Str, PhpType::False]))
}
