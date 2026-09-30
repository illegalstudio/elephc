//! Purpose:
//! Home of the PHP `class_alias` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The check hook always errors: `class_alias()` is only supported as a top-level
//!   statement whose class names are compile-time constants (string literals, `Name::class`,
//!   and their concatenations), which `crate::autoload::alias` turns into declarations before
//!   the type checker runs. Any call that reaches this hook is rejected.
//! - Arguments are pre-inferred by the registry common path before the hook runs.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    contract: "class_alias",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ClassAlias,
    ),
}

/// Rejects any direct `class_alias()` call that reaches the type checker.
///
/// AOT compilation resolves `class_alias()` at the top-level statement stage only, and only when
/// both class names are compile-time constants. A call in another context, or one whose names
/// are only known at run time (a variable, `$object::class`), is not supported and is rejected
/// here with a message naming the forms that are accepted.
/// An explicitly disabled autoload flag has its own diagnostic after shared argument planning.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if cx.args.get(2).is_some_and(|arg| {
        matches!(arg.kind, ExprKind::BoolLiteral(false) | ExprKind::IntLiteral(0))
    }) {
        return Err(CompileError::new(cx.span, UNSUPPORTED_AUTOLOAD_FALSE));
    }
    Err(CompileError::new(cx.span, UNSUPPORTED_CALL_SHAPE))
}

/// Explains the alias collector's unsupported explicitly disabled autoload mode.
const UNSUPPORTED_AUTOLOAD_FALSE: &str = "class_alias() does not support autoload=false in AOT mode; \
    omit autoload or pass true";

/// The diagnostic for a `class_alias()` call the top-level alias collector could not turn into a
/// declaration. It lists the accepted class-name forms so a rejected call points at a fix.
const UNSUPPORTED_CALL_SHAPE: &str = "class_alias() is only supported as a top-level statement \
    with compile-time-constant class names (string literals, Name::class, or a concatenation of them)";
