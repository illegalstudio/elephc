//! Purpose:
//! Home of the PHP `mb_convert_case` builtin: declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - The public signature matches PHP: `mb_convert_case(string $string, int $mode, ?string $encoding = null)`.
//! - `$mode` must be one of the `MB_CASE_*` constants; omitted/null encoding uses UTF-8.

use crate::{
    builtins::spec::{BuiltinCheckCtx},
    errors::CompileError,
    types::PhpType,
};

builtin! {
    contract: "mb_convert_case",
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::MbConvertCase,
    ),
}

/// Validates PHP's string, integer mode, and nullable optional encoding parameter surface.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let string_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if string_ty != PhpType::Str {
        return Err(CompileError::new(
            cx.args[0].span,
            "mb_convert_case() string argument must be string",
        ));
    }

    let mode_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if mode_ty != PhpType::Int {
        return Err(CompileError::new(
            cx.args[1].span,
            "mb_convert_case() mode argument must be int",
        ));
    }

    if let Some(encoding) = cx.args.get(2) {
        let encoding_ty = cx.checker.infer_type(encoding, cx.env)?;
        if !matches!(encoding_ty, PhpType::Str | PhpType::Void) {
            return Err(CompileError::new(
                encoding.span,
                "mb_convert_case() encoding argument must be string or null",
            ));
        }
    }

    Ok(PhpType::Str)
}
