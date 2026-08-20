//! Purpose:
//! Home of the PHP `iconv_substr` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `$offset` and `$length` count characters, and follow PHP's `substr()` conventions
//!   for negative values and an omitted length.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "iconv_substr",
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::IconvSubstr,
    ),
}

/// Validates `iconv_substr()`'s arguments and returns `PhpType::Union([Str, False])`.
///
/// The hook infers every argument itself so a container passed where PHP declares a
/// string is rejected here instead of reaching the backend.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    super::iconv_strlen::check_string_argument(cx, 0, "iconv_substr", "string")?;
    check_optional_int_argument(cx, 1)?;
    check_optional_int_argument(cx, 2)?;
    super::iconv_strlen::check_nullable_string_argument(cx, 3, "iconv_substr", "encoding")?;
    Ok(PhpType::Union(vec![PhpType::Str, PhpType::False]))
}

/// Infers one optional numeric argument so its side effects and narrowing still run.
fn check_optional_int_argument(
    cx: &mut BuiltinCheckCtx,
    index: usize,
) -> Result<(), CompileError> {
    if let Some(argument) = cx.args.get(index) {
        cx.checker.infer_type(argument, cx.env)?;
    }
    Ok(())
}
