//! Purpose:
//! Home of the PHP `iconv_get_encoding` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `$type` defaults to `all`, which reports the whole trio as an associative array; any
//!   other recognized name reports one charset as a string.
//! - The `mixed` contract covers the array, string, and `false` outcomes at once.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "iconv_get_encoding",
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::IconvGetEncoding,
    ),
}

/// Validates `iconv_get_encoding()`'s arguments and returns `PhpType::Mixed`.
///
/// The hook infers every argument itself so a container passed where PHP declares a
/// string is rejected here instead of reaching the backend.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    super::iconv_strlen::check_nullable_string_argument(cx, 0, "iconv_get_encoding", "type")?;
    Ok(PhpType::Mixed)
}
