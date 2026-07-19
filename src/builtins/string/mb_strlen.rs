//! Purpose:
//! Home of the PHP `mb_strlen` builtin: declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), both via
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - The public signature matches PHP: `mb_strlen(string $string, ?string $encoding = null)`.
//! - Omitted/null encoding uses UTF-8; explicit encodings are handled by the target runtime,
//!   which keeps malformed-sequence counting aligned with mbstring and rejects unknown names.

use crate::{
    builtins::spec::{BuiltinCheckCtx, DefaultSpec},
    codegen::{context::FunctionContext, CodegenIrError},
    errors::CompileError,
    ir::Instruction,
    types::PhpType,
};

builtin! {
    name: "mb_strlen",
    area: String,
    params: [string: Str, encoding: Str = DefaultSpec::Null],
    returns: Int,
    check: check,
    lazy_check: true,
    lower: lower,
    summary: "Returns the character count of a string in the requested encoding.",
    php_manual: "https://www.php.net/manual/en/function.mb-strlen.php",
}

/// Validates PHP's string plus nullable optional encoding parameter surface.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.require_macos_builtin_library("iconv");
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

/// Lowers an `mb_strlen` call by dispatching to the shared `lower_mb_strlen` emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::strings::lower_mb_strlen(ctx, inst)
}
