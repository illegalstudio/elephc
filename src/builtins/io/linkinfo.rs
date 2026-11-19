//! Purpose:
//! Home of the PHP `linkinfo` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `linkinfo` is a pure-data builtin whose return type
//!   (`Int`) is fully determined by its declaration. The registry common path
//!   infers the argument and enforces arity before falling back to `returns`.


builtin! {
    contract: "linkinfo",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Linkinfo,
    ),
}

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

/// Rejects an empty literal path on the PHP 8.6 profile.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if crate::codegen_support::compile_php_version() >= crate::php_version::PhpVersion::Php86 {
        if let Some(arg) = cx.args.first() {
            if matches!(&arg.kind, ExprKind::StringLiteral(value) if value.is_empty()) {
                return Err(CompileError::new(
                    arg.span,
                    "linkinfo(): Argument #1 ($path) cannot be empty",
                ));
            }
        }
    }
    Ok(PhpType::Int)
}
