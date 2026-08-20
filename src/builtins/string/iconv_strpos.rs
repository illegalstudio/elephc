//! Purpose:
//! Home of the PHP `iconv_strpos` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The reported position counts characters; an empty `$needle` never matches.
//! - An `$offset` outside `$haystack` raises PHP's catchable `ValueError` from the backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "iconv_strpos",
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::IconvStrpos,
    ),
}

/// Validates `iconv_strpos()`'s arguments and returns `PhpType::Union([Int, False])`.
///
/// The hook infers every argument itself so a container passed where PHP declares a
/// string is rejected here instead of reaching the backend.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    super::iconv_strlen::check_string_argument(cx, 0, "iconv_strpos", "haystack")?;
    super::iconv_strlen::check_string_argument(cx, 1, "iconv_strpos", "needle")?;
    if let Some(offset) = cx.args.get(2) {
        cx.checker.infer_type(offset, cx.env)?;
    }
    super::iconv_strlen::check_nullable_string_argument(cx, 3, "iconv_strpos", "encoding")?;
    Ok(PhpType::Union(vec![PhpType::Int, PhpType::False]))
}
