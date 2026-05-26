//! Purpose:
//! Defines type-system metadata for compiler-supported PHP Fiber behavior.
//! Tracks Fiber class relationships and result typing used by builtin type injection and checks.
//!
//! Called from:
//! - `crate::types::checker::builtin_types`
//! - `crate::types::checker`
//!
//! Key details:
//! - Fiber typing is modeled at compile time; runtime scheduling behavior remains outside this module.

use crate::errors::CompileError;
use crate::parser::ast::{Stmt, StmtKind};
use crate::span::Span;

use super::{FunctionSig, PhpType};

/// Maximum number of start arguments a Fiber callback can accept.
pub(crate) const FIBER_START_ARG_LIMIT: usize = 7;
/// Maximum number of integer slots available for Fiber capture of non-float types.
pub(crate) const FIBER_INT_SLOT_LIMIT: usize = 7;
/// Maximum number of float slots available for Fiber capture of float types.
pub(crate) const FIBER_FLOAT_SLOT_LIMIT: usize = 7;

/// Returns the number of visible parameters for a Fiber callback signature.
/// For non-variadic functions this equals `param_count`; for variadic functions
/// it is `param_count + 1` to account for the variadic slot.
pub(crate) fn visible_param_count(param_count: usize, variadic: bool) -> usize {
    param_count + usize::from(variadic)
}

/// Adapts a Fiber callback's `FunctionSig` in-place for the Fiber calling convention.
/// Undeclared, non-reference parameters (up to `visible_param_count`) are set to `PhpType::Mixed`
/// so the caller can pass any type. If `no_terminal_return` is true and the signature has no
/// declared return, the return type is set to `PhpType::Void`.
pub(crate) fn adapt_entry_sig(
    sig: &mut FunctionSig,
    visible_param_count: usize,
    no_terminal_return: bool,
) {
    for i in 0..visible_param_count.min(sig.params.len()) {
        let declared = sig.declared_params.get(i).copied().unwrap_or(false);
        let by_ref = sig.ref_params.get(i).copied().unwrap_or(false);
        if !declared && !by_ref {
            sig.params[i].1 = PhpType::Mixed;
        }
    }
    if no_terminal_return && !sig.declared_return {
        sig.return_type = PhpType::Void;
    }
}

/// Returns `true` if the given statement list contains a `return` statement
/// that is reachable from any path (including inside synthetic bodies, conditionals,
/// loops, try-catch-finally, and switch). Used to determine whether a Fiber callback
/// body has a terminal return.
pub(crate) fn closure_body_has_return(body: &[Stmt]) -> bool {
    body.iter().any(stmt_has_return)
}

/// Validates that a `FunctionSig` is a valid Fiber callback signature.
/// Returns an error if the callback is variadic, has more than `FIBER_START_ARG_LIMIT`
/// visible parameters, or has any by-reference parameters.
pub(crate) fn validate_callback_signature(
    sig: &FunctionSig,
    visible_param_count: usize,
    span: Span,
) -> Result<(), CompileError> {
    if sig.variadic.is_some() {
        return Err(CompileError::new(
            span,
            "Fiber callbacks cannot be variadic",
        ));
    }

    if visible_param_count > FIBER_START_ARG_LIMIT {
        return Err(CompileError::new(
            span,
            &format!(
                "Fiber callbacks support at most {} start arguments, got {}",
                FIBER_START_ARG_LIMIT, visible_param_count
            ),
        ));
    }

    if sig
        .ref_params
        .iter()
        .take(visible_param_count)
        .any(|by_ref| *by_ref)
    {
        return Err(CompileError::new(
            span,
            "Fiber callbacks cannot receive start arguments by reference",
        ));
    }

    Ok(())
}

/// Validates that a Fiber capture list does not exceed available integer or float slots
/// for the Fiber ABI. Returns an error if the combined capture types would exceed
/// `FIBER_INT_SLOT_LIMIT` integer slots or `FIBER_FLOAT_SLOT_LIMIT` float slots.
/// By-reference captures count as integer slots regardless of their actual type.
pub(crate) fn validate_capture_slots(
    sig: &FunctionSig,
    visible_param_count: usize,
    capture_types: &[(String, PhpType, bool)],
    span: Span,
) -> Result<(), CompileError> {
    let mut int_slot = sig
        .params
        .iter()
        .take(visible_param_count)
        .map(|(_, ty)| int_slot_count(ty))
        .sum::<usize>();
    let mut float_slot = sig
        .params
        .iter()
        .take(visible_param_count)
        .filter(|(_, ty)| matches!(ty.codegen_repr(), PhpType::Float))
        .count();

    for (name, ty, by_ref) in capture_types {
        let slot_ty = if *by_ref {
            PhpType::Int
        } else {
            ty.codegen_repr()
        };
        match slot_ty {
            PhpType::Float => {
                if float_slot >= FIBER_FLOAT_SLOT_LIMIT {
                    return Err(CompileError::new(
                        span,
                        &format!(
                            "Fiber capture ${} exceeds the {} float-slot Fiber capture limit",
                            name, FIBER_FLOAT_SLOT_LIMIT
                        ),
                    ));
                }
                float_slot += 1;
            }
            PhpType::Void | PhpType::Never => {}
            other => {
                let needed = int_slot_count(&other);
                if int_slot + needed > FIBER_INT_SLOT_LIMIT {
                    return Err(CompileError::new(
                        span,
                        &format!(
                            "Fiber capture ${} exceeds the {} integer-slot Fiber capture limit",
                            name, FIBER_INT_SLOT_LIMIT
                        ),
                    ));
                }
                int_slot += needed;
            }
        }
    }

    Ok(())
}

/// Returns how many integer slots the given `PhpType` consumes in the Fiber ABI.
/// `Float`/`Void`/`Never` consume 0 slots; `Str` consumes 2; all other types consume 1.
fn int_slot_count(ty: &PhpType) -> usize {
    match ty.codegen_repr() {
        PhpType::Float | PhpType::Void | PhpType::Never => 0,
        PhpType::Str => 2,
        _ => 1,
    }
}

/// Returns `true` if the given statement contains a reachable `return` based on its variant.
/// Recursively checks synthetic bodies, branches of `If`/`ElseIf`/`Else`, loop bodies,
/// `Try`/`Catch`/`Finally`, and `Switch` cases. All other statement kinds return `false`.
fn stmt_has_return(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Return(_) => true,
        StmtKind::Synthetic(stmts) => closure_body_has_return(stmts),
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            closure_body_has_return(then_body)
                || elseif_clauses
                    .iter()
                    .any(|(_, body)| closure_body_has_return(body))
                || else_body
                    .as_ref()
                    .is_some_and(|body| closure_body_has_return(body))
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::Foreach { body, .. } => closure_body_has_return(body),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            closure_body_has_return(try_body)
                || catches
                    .iter()
                    .any(|catch_clause| closure_body_has_return(&catch_clause.body))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| closure_body_has_return(body))
        }
        StmtKind::Switch { cases, default, .. } => {
            cases.iter().any(|(_, body)| closure_body_has_return(body))
                || default
                    .as_ref()
                    .is_some_and(|body| closure_body_has_return(body))
        }
        _ => false,
    }
}
