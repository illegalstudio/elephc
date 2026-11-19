//! Purpose:
//! Home of the PHP `sleep` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `sleep` is a pure-data builtin whose return type
//!   (`Int`) is fully determined by its declaration.


builtin! {
    contract: "sleep",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Sleep,
    ),
}

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

/// Rejects a literal value outside the unsigned-int timeout range on PHP 8.6.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if crate::codegen_support::compile_php_version() >= crate::php_version::PhpVersion::Php86 {
        if let Some(arg) = cx.args.first() {
            let value = match &arg.kind {
                ExprKind::IntLiteral(value) => Some(*value),
                ExprKind::Negate(inner) => match &inner.kind {
                    ExprKind::IntLiteral(value) => Some(-*value),
                    _ => None,
                },
                _ => None,
            };
            if let Some(value) = value {
                if !(0..=i64::from(u32::MAX)).contains(&value) {
                    return Err(CompileError::new(
                        arg.span,
                        "sleep(): Argument #1 ($seconds) must be between 0 and 4294967295",
                    ));
                }
            }
        }
    }
    Ok(PhpType::Int)
}
