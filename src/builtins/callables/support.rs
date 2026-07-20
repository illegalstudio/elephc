//! Purpose:
//! Shared type-check hooks for the callables-area class-reflection builtin homes.
//! Provides the common validation logic used by multiple homes to avoid duplication.
//!
//! Called from:
//! - `crate::builtins::callables::*` homes that set `check:` to one of these functions.
//!
//! Key details:
//! - Each hook receives a pre-populated `BuiltinCheckCtx`; for non-lazy homes args are
//!   already inferred by the registry common path before the hook runs.
//! - `check_class_like_exists` inspects `.kind` only (no infer) — the common path already
//!   inferred every arg before this hook is called.
//! - `check_class_relation` homes use `lazy_check: true`, so the hook performs its own
//!   inference in source order (matching the legacy arm).
//! - `check_declared_names` takes no args and returns `Array<Str>` unconditionally.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

/// Validates `class_exists` / `interface_exists` / `trait_exists` / `enum_exists` arguments.
///
/// Requires that the first argument is a string literal and, if present, the second argument
/// is a literal bool or int (the autoload flag). Returns `Bool` on success.
/// Arguments are pre-inferred by the registry common path before this hook runs.
pub(crate) fn check_class_like_exists(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if !matches!(cx.args[0].kind, ExprKind::StringLiteral(_)) {
        return Err(CompileError::new(
            cx.span,
            &format!("{}() first argument must be a string literal in AOT mode", cx.name),
        ));
    }
    // The optional autoload flag may be dynamic: it never contributes an AOT
    // autoload demand (the demand walker treats non-literals as false), and
    // existence still folds from the literal class name. The registry common
    // path has already inferred it.
    Ok(PhpType::Bool)
}

/// Validates `class_implements` / `class_parents` / `class_uses` arguments.
///
/// Infers the first argument and requires it to be an object or string literal.
/// If present, infers and validates the second argument (autoload flag) as a literal bool or int.
/// Returns the union `array<string,string>|bool` used by the PHP class-relation builtins.
/// This hook is called with `lazy_check: true` so inference happens here, not in the common path.
pub(crate) fn check_class_relation(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let first_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let dynamic_eval_target = cx.checker.eval_barrier_active
        && matches!(first_ty.codegen_repr(), PhpType::Mixed | PhpType::Str);
    if !matches!(first_ty, PhpType::Object(_))
        && !matches!(cx.args[0].kind, ExprKind::StringLiteral(_))
        && !dynamic_eval_target
    {
        return Err(CompileError::new(
            cx.span,
            &format!("{}() first argument must be an object or string literal in AOT mode", cx.name),
        ));
    }
    if let Some(autoload_arg) = cx.args.get(1) {
        cx.checker.infer_type(autoload_arg, cx.env)?;
        if !matches!(
            autoload_arg.kind,
            ExprKind::BoolLiteral(_) | ExprKind::IntLiteral(_)
        ) {
            return Err(CompileError::new(
                cx.span,
                &format!("{}() autoload argument must be a literal bool or int in AOT mode", cx.name),
            ));
        }
    }
    Ok(PhpType::Union(vec![
        PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Str),
        },
        PhpType::Bool,
    ]))
}

/// Returns `Array<Str>` for the zero-argument declared-names builtins.
///
/// The hook ignores its context because these builtins take no arguments; the registry
/// common path enforces arity = 0 before this hook runs.
pub(crate) fn check_declared_names(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Array(Box::new(PhpType::Str)))
}
