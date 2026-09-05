//! Purpose:
//! Home of the PHP `mb_strimwidth` builtin: declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - The public signature matches PHP: `mb_strimwidth(string $string, int $start,
//!   int $width, string $trim_marker = "", ?string $encoding = null)`.
//! - Omitted/null encoding uses UTF-8 display-width trimming; unknown encodings are
//!   rejected at runtime with a catchable `ValueError`.

use crate::{
    builtins::spec::BuiltinCheckCtx,
    errors::CompileError,
    types::PhpType,
};

builtin! {
    contract: "mb_strimwidth",
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::MbStrimwidth,
    ),
}

/// Validates PHP's string/start/width plus optional marker and nullable encoding surface.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let string_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if string_ty != PhpType::Str {
        return Err(CompileError::new(
            cx.args[0].span,
            "mb_strimwidth() string argument must be string",
        ));
    }

    if let Some(start) = cx.args.get(1) {
        cx.checker.infer_type(start, cx.env)?;
    }
    if let Some(width) = cx.args.get(2) {
        cx.checker.infer_type(width, cx.env)?;
    }

    if let Some(trim_marker) = cx.args.get(3) {
        let marker_ty = cx.checker.infer_type(trim_marker, cx.env)?;
        if marker_ty != PhpType::Str {
            return Err(CompileError::new(
                trim_marker.span,
                "mb_strimwidth() trim_marker argument must be string",
            ));
        }
    }

    if let Some(encoding) = cx.args.get(4) {
        let encoding_ty = cx.checker.infer_type(encoding, cx.env)?;
        if !matches!(encoding_ty, PhpType::Str | PhpType::Void) {
            return Err(CompileError::new(
                encoding.span,
                "mb_strimwidth() encoding argument must be string or null",
            ));
        }
    }

    Ok(PhpType::Str)
}
