//! Purpose:
//! Home of the PHP `flock` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates the stream resource, accepts concrete or gradual integer-like
//!   `operation` values, and verifies that `would_block` (when present) is passed as
//!   a variable. Gradual values are cast by the typed runtime lowering.
//! - `would_block` is a by-reference parameter (`ref` marker in `params:`); the hook's
//!   variable check is in addition to, not instead of, the ref-ness.
//! - Arguments are pre-inferred by the registry before the hook runs; `operation` is
//!   re-inferred inside the hook to obtain its type for validation.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    contract: "flock",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Flock,
    ),
}

/// Validates the stream resource, accepts runtime-castable gradual operations, and
/// requires that `would_block` (if provided) is passed as a plain variable.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    let op_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if !matches!(
        op_ty,
        PhpType::Int | PhpType::Bool | PhpType::Mixed | PhpType::Union(_)
    ) {
        return Err(CompileError::new(
            cx.args[1].span,
            "flock() operation must be int",
        ));
    }
    if let Some(arg2) = cx.args.get(2) {
        if !matches!(arg2.kind, ExprKind::Variable(_)) {
            return Err(CompileError::new(
                arg2.span,
                "flock() parameter $would_block must be passed a variable",
            ));
        }
    }
    Ok(PhpType::Bool)
}
