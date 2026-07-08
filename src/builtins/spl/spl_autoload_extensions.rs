//! Purpose:
//! Home of the PHP `spl_autoload_extensions` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - A `check` hook is required to validate that the optional argument, when present,
//!   is a string literal or null (the runtime only handles AOT-known extension strings).
//! - Returns the current extension string (`Str`) in all cases.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    name: "spl_autoload_extensions",
    area: Spl,
    params: [file_extensions: Mixed = DefaultSpec::Null],
    returns: Str,
    check: check,
    lower: lower,
    summary: "Register and return default file extensions for spl_autoload.",
    php_manual: "https://www.php.net/manual/en/function.spl-autoload-extensions.php",
}

/// Validates the optional argument is a string literal or null; returns `Str`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if let Some(arg) = cx.args.first() {
        cx.checker.infer_type(arg, cx.env)?;
        if !matches!(arg.kind, ExprKind::StringLiteral(_) | ExprKind::Null) {
            return Err(CompileError::new(
                cx.span,
                "spl_autoload_extensions() argument must be a string literal or null",
            ));
        }
    }
    Ok(PhpType::Str)
}

/// Lowers `spl_autoload_extensions()` by delegating to the extension-globals emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::spl::lower_spl_autoload_extensions(ctx, inst)
}
