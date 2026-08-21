//! Purpose:
//! Home of the PHP `iconv` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `$to_encoding` may carry libc's `//TRANSLIT` and `//IGNORE` suffixes, which the bridge
//!   forwards to the platform iconv exactly as php-src does.
//! - An unusable charset pair is a warning and an undecodable byte is a notice; both
//!   return `false`, so the checker contract is the `string|false` union.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "iconv",
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Iconv,
    ),
}

/// Validates `iconv()`'s arguments and returns `PhpType::Union([Str, False])`.
///
/// The hook infers every argument itself so a container passed where PHP declares a
/// string is rejected here instead of reaching the backend.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    super::iconv_strlen::check_string_argument(cx, 0, "iconv", "from_encoding")?;
    super::iconv_strlen::check_string_argument(cx, 1, "iconv", "to_encoding")?;
    super::iconv_strlen::check_string_argument(cx, 2, "iconv", "string")?;
    Ok(PhpType::Union(vec![PhpType::Str, PhpType::False]))
}
