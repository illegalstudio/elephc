//! Purpose:
//! Home of the `buffer_free` builtin (elephc extension): its declaration, checker contract, and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` enforces the legacy checker-resident rules verbatim: the argument must
//!   be a plain local variable (never `$this`, a by-ref parameter, a `global`, or a
//!   `static`) of type `buffer<T>`, because lowering nulls the local slot after the
//!   free so use-after-free traps deterministically.
//! - `extension: true`: buffers have no PHP equivalent, so `--strict-php` hides this
//!   builtin from user programs.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    name: "buffer_free",
    area: Pointers,
    params: [buffer: Mixed],
    returns: Void,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::BufferFree,
    ),
    summary: "Frees a buffer<T> and nulls the local variable that held it.",
    extension: true,
}

/// Validates that the argument is a freeable local `buffer<T>` variable.
///
/// Mirrors the legacy checker arm exactly: rejects `$this`, by-ref parameters,
/// `global` and `static` variables, and non-variable expressions, then requires
/// the argument type to be `buffer<T>`. The registry's `check_arity` handles
/// arity enforcement (exactly 1 argument).
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    match &cx.args[0].kind {
        ExprKind::Variable(name) => {
            if cx.checker.current_class.is_some() && name == "this" {
                return Err(CompileError::new(cx.span, "buffer_free() cannot free $this"));
            }
            if cx.checker.active_ref_params.contains(name)
                || cx.checker.active_globals.contains(name)
                || cx.checker.active_statics.contains(name)
            {
                return Err(CompileError::new(
                    cx.span,
                    "buffer_free() argument must be a local variable",
                ));
            }
        }
        _ => {
            let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
            if !matches!(ty, PhpType::Buffer(_)) {
                return Err(CompileError::new(
                    cx.span,
                    "buffer_free() argument must be buffer<T>",
                ));
            }
            return Err(CompileError::new(
                cx.span,
                "buffer_free() argument must be a local variable",
            ));
        }
    }
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(ty, PhpType::Buffer(_)) {
        return Err(CompileError::new(
            cx.span,
            "buffer_free() argument must be buffer<T>",
        ));
    }
    Ok(PhpType::Void)
}
