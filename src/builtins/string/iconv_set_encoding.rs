//! Purpose:
//! Home of the PHP `iconv_set_encoding` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - php-src stores the new charset without validating it, so only an unrecognized `$type`
//!   makes the call report `false`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "iconv_set_encoding",
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::IconvSetEncoding,
    ),
}

/// Validates `iconv_set_encoding()`'s arguments and returns `PhpType::Bool`.
///
/// The hook infers every argument itself so a container passed where PHP declares a
/// string is rejected here instead of reaching the backend.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    super::iconv_strlen::check_string_argument(cx, 0, "iconv_set_encoding", "type")?;
    super::iconv_strlen::check_string_argument(cx, 1, "iconv_set_encoding", "encoding")?;
    Ok(PhpType::Bool)
}
