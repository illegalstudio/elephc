//! Purpose:
//! Home of the PHP `iconv_mime_encode` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `$options` accepts `scheme`, `input-charset`, `output-charset`, `line-length`, and
//!   `line-break-chars`; the backend reads them out of the array at the call site.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "iconv_mime_encode",
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::IconvMimeEncode,
    ),
}

/// Validates `iconv_mime_encode()`'s arguments and returns `PhpType::Union([Str, False])`.
///
/// The hook infers every argument itself so a container passed where PHP declares a
/// string is rejected here instead of reaching the backend.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    super::iconv_strlen::check_string_argument(cx, 0, "iconv_mime_encode", "field_name")?;
    super::iconv_strlen::check_string_argument(cx, 1, "iconv_mime_encode", "field_value")?;
    check_options_argument(cx)?;
    Ok(PhpType::Union(vec![PhpType::Str, PhpType::False]))
}

/// Rejects an `$options` argument that cannot be a PHP array.
fn check_options_argument(cx: &mut BuiltinCheckCtx) -> Result<(), CompileError> {
    let Some(options) = cx.args.get(2) else {
        return Ok(());
    };
    let span = options.span;
    let inferred = cx.checker.infer_type(options, cx.env)?;
    if matches!(
        inferred.codegen_repr(),
        PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Mixed
            | PhpType::Union(_)
            | PhpType::Void
            | PhpType::Never
    ) {
        return Ok(());
    }
    Err(CompileError::new(
        span,
        "iconv_mime_encode() options argument must be array",
    ))
}
