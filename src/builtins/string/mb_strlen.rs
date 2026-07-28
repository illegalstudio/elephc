//! Purpose:
//! Home of the PHP `mb_strlen` builtin: declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - The checker enforces PHP's `?string $encoding = null` contract while the registry keeps
//!   the concrete argument representation intact for the target runtime.
//! - Omitted/null encoding uses UTF-8; explicit encodings are handled by the target runtime,
//!   which keeps malformed-sequence counting aligned with mbstring and rejects unknown names.

use crate::{
    builtins::spec::{BuiltinCheckCtx, DefaultSpec},
    errors::CompileError,
    types::PhpType,
};

builtin! {
    name: "mb_strlen",
    area: String,
    params: [string: Str, encoding: Mixed = DefaultSpec::Null],
    returns: Int,
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::MbStrlen,
    ),
    summary: "Returns the character count of a string in the requested encoding.",
    php_manual: "https://www.php.net/manual/en/function.mb-strlen.php",
}

/// Validates PHP's string plus nullable optional encoding parameter surface.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let string_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if string_ty != PhpType::Str {
        return Err(CompileError::new(
            cx.args[0].span,
            "mb_strlen() string argument must be string",
        ));
    }

    if let Some(encoding) = cx.args.get(1) {
        let encoding_ty = cx.checker.infer_type(encoding, cx.env)?;
        if !matches!(encoding_ty, PhpType::Str | PhpType::Void) {
            return Err(CompileError::new(
                encoding.span,
                "mb_strlen() encoding argument must be string or null",
            ));
        }
    }

    Ok(PhpType::Int)
}
