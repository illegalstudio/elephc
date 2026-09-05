//! Purpose:
//! Home of the PHP `mb_strwidth` builtin: declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - The public signature matches PHP: `mb_strwidth(string $string, ?string $encoding = null)`.
//! - Omitted/null encoding uses UTF-8; explicit encodings follow the `mb_strlen` runtime
//!   contract, then each decoded code point contributes PHP 8.5 East Asian Width.

use crate::{
    builtins::spec::{BuiltinCheckCtx},
    errors::CompileError,
    types::PhpType,
};

builtin! {
    contract: "mb_strwidth",
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::MbStrwidth,
    ),
}

/// Validates PHP's string plus nullable optional encoding parameter surface.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let string_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if string_ty != PhpType::Str {
        return Err(CompileError::new(
            cx.args[0].span,
            "mb_strwidth() string argument must be string",
        ));
    }

    if let Some(encoding) = cx.args.get(1) {
        let encoding_ty = cx.checker.infer_type(encoding, cx.env)?;
        if !matches!(encoding_ty, PhpType::Str | PhpType::Void) {
            return Err(CompileError::new(
                encoding.span,
                "mb_strwidth() encoding argument must be string or null",
            ));
        }
    }

    Ok(PhpType::Int)
}
