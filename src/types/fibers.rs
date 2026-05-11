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

pub(crate) const FIBER_START_ARG_LIMIT: usize = 7;
pub(crate) const FIBER_INT_SLOT_LIMIT: usize = 7;
pub(crate) const FIBER_FLOAT_SLOT_LIMIT: usize = 7;

pub(crate) fn visible_param_count(param_count: usize, variadic: bool) -> usize {
    param_count + usize::from(variadic)
}

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

pub(crate) fn closure_body_has_return(body: &[Stmt]) -> bool {
    body.iter().any(stmt_has_return)
}

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

pub(crate) fn validate_capture_slots(
    sig: &FunctionSig,
    visible_param_count: usize,
    capture_types: &[(String, PhpType)],
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

    for (name, ty) in capture_types {
        match ty.codegen_repr() {
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

fn int_slot_count(ty: &PhpType) -> usize {
    match ty.codegen_repr() {
        PhpType::Float | PhpType::Void | PhpType::Never => 0,
        PhpType::Str => 2,
        _ => 1,
    }
}

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
