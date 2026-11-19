//! Purpose:
//! Home of the PHP `putenv` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin: return type (`Bool`) is fully determined by the declaration.


builtin! {
    contract: "putenv",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Putenv,
    ),
}

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

/// Rejects a literal NUL byte in `$assignment` on the PHP 8.6 profile.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if crate::codegen_support::compile_php_version() >= crate::php_version::PhpVersion::Php86 {
        if let Some(arg) = cx.args.first() {
            if matches!(&arg.kind, ExprKind::StringLiteral(value) if value.contains('\0')) {
                return Err(CompileError::new(
                    arg.span,
                    "putenv(): Argument #1 ($assignment) must not contain any null bytes",
                ));
            }
        }
    }
    Ok(PhpType::Bool)
}
