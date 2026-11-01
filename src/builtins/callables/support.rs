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
//! - `check_class_like_exists` relies on the shared signature for string/bool validation;
//!   literal and runtime string names share the same closed-world metadata lookup.
//! - `check_class_relation` homes use `lazy_check: true`, so the hook performs its own
//!   inference in source order (matching the legacy arm).
//! - `check_declared_names` takes no args and returns `Array<Str>` unconditionally.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

/// Validates `class_exists` / `interface_exists` / `trait_exists` / `enum_exists` arguments.
///
/// Accepts literal or runtime string names and returns `Bool`.
///
/// The registry common path has already validated both arguments against the shared signature.
/// Literal names may additionally seed AOT autoload discovery, while runtime names query the
/// emitted closed-world metadata without creating an unknowable compile-time load demand.
pub(crate) fn check_class_like_exists(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Bool)
}

/// Validates `class_implements` / `class_parents` / `class_uses` arguments.
///
/// Infers the first argument and requires it to be an object or string.
/// If present, infers and validates the second argument (autoload flag) as a literal bool or int.
/// Returns the union `array<string,string>|bool` used by the PHP class-relation builtins.
/// This hook is called with `lazy_check: true` so inference happens here, not in the common path.
pub(crate) fn check_class_relation(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let first_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let gradual_target = matches!(first_ty.codegen_repr(), PhpType::Mixed);
    if !matches!(first_ty, PhpType::Object(_) | PhpType::Str) && !gradual_target {
        return Err(CompileError::new(
            cx.span,
            &format!("{}() first argument must be an object or string", cx.name),
        ));
    }
    if let Some(autoload_arg) = cx.args.get(1) {
        cx.checker.infer_type(autoload_arg, cx.env)?;
        if !matches!(
            autoload_arg.kind,
            ExprKind::BoolLiteral(_) | ExprKind::IntLiteral(_)
        ) {
            return Err(CompileError::aot_literal_required(
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
