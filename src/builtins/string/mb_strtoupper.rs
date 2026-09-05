//! Purpose:
//! Home of the PHP `mb_strtoupper` builtin: declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - The public signature matches PHP: `mb_strtoupper(string $string, ?string $encoding = null)`.
//! - Omitted/null encoding uses UTF-8 full case mapping; explicit encodings follow the
//!   `mb_strlen` contract, including byte-oriented aliases and catchable unknown-name errors.

use crate::{
    builtins::spec::{BuiltinCheckCtx},
    errors::CompileError,
    types::PhpType,
};

builtin! {
    contract: "mb_strtoupper",
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::MbStrtoupper,
    ),
}

/// Validates PHP's string plus nullable optional encoding parameter surface.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let string_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if string_ty != PhpType::Str {
        return Err(CompileError::new(
            cx.args[0].span,
            "mb_strtoupper() string argument must be string",
        ));
    }

    if let Some(encoding) = cx.args.get(1) {
        let encoding_ty = cx.checker.infer_type(encoding, cx.env)?;
        if !matches!(encoding_ty, PhpType::Str | PhpType::Void) {
            return Err(CompileError::new(
                encoding.span,
                "mb_strtoupper() encoding argument must be string or null",
            ));
        }
    }

    Ok(PhpType::Str)
}
