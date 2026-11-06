//! Purpose:
//! Home of the PHP `defined` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the argument is a string literal (AOT requirement: the
//!   constant name must be statically known at compile time).
//! - The semantic effect includes `MAY_THROW` because invalid relative class
//!   receivers raise catchable PHP `Error`s during static lowering.

use crate::builtins::semantics::{runtime_fn_semantics, BuiltinEffects, BuiltinSemantics};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    contract: "defined",
    check: check,
    semantics: defined_semantics(),
}

/// Adds static-lowering access errors to the typed runtime target's global-read effect.
const fn defined_semantics() -> BuiltinSemantics {
    let mut semantics = runtime_fn_semantics(crate::ir::RuntimeFnId::Defined);
    semantics.effects = BuiltinEffects::Static(crate::ir::Effects::from_bits_retain(
        crate::ir::RuntimeFnId::Defined.effects().bits() | crate::ir::Effects::MAY_THROW.bits(),
    ));
    semantics
}

/// Validates that the argument is a string literal.
///
/// AOT compilation requires a statically known constant name; dynamic names cannot
/// be resolved at compile time. Returns `PhpType::Bool` on success.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    let is_literal = match &cx.args[0].kind {
        ExprKind::StringLiteral(_) => true,
        ExprKind::NamedArg { name, value } if name == "constant_name" => {
            matches!(value.kind, ExprKind::StringLiteral(_))
        }
        _ => false,
    };
    if !is_literal {
        return Err(CompileError::aot_literal_required(
            cx.span,
            "defined() first argument must be a string literal in AOT mode",
        ));
    }
    Ok(PhpType::Bool)
}
