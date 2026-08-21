//! Purpose:
//! Home of the PHP `iconv_strrpos` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - PHP's signature has no `$offset`, so the whole haystack is always scanned.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "iconv_strrpos",
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::IconvStrrpos,
    ),
}

/// Validates `iconv_strrpos()`'s arguments and returns `PhpType::Union([Int, False])`.
///
/// The hook infers every argument itself so a container passed where PHP declares a
/// string is rejected here instead of reaching the backend.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    super::iconv_strlen::check_string_argument(cx, 0, "iconv_strrpos", "haystack")?;
    super::iconv_strlen::check_string_argument(cx, 1, "iconv_strrpos", "needle")?;
    super::iconv_strlen::check_nullable_string_argument(cx, 2, "iconv_strrpos", "encoding")?;
    Ok(PhpType::Union(vec![PhpType::Int, PhpType::False]))
}
