//! Purpose:
//! Dispatches conditional and switch statement lowering.
//! Keeps branch-label orchestration separate from the top-level statement dispatcher.
//!
//! Called from:
//! - `crate::codegen_support::stmt::control_flow`
//!
//! Key details:
//! - Condition expressions must be evaluated once and branch bodies must share surrounding cleanup context.

mod if_stmt;
mod switch_stmt;

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::parser::ast::{Expr, Stmt};

/// Lowers a PHP if/elseif/else statement into conditional branch assembly.
/// Evaluates the condition once, emits branch labels for each clause,
/// and jumps past remaining branches after each body completes.
/// All branch bodies share the surrounding cleanup context (loops, switch, finally).
pub(super) fn emit_if_stmt(
    condition: &Expr,
    then_body: &[Stmt],
    elseif_clauses: &[(Expr, Vec<Stmt>)],
    else_body: &Option<Vec<Stmt>>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    if_stmt::emit_if_stmt(
        condition,
        then_body,
        elseif_clauses,
        else_body,
        emitter,
        ctx,
        data,
    )
}

/// Lowers a PHP switch statement into jump-table or chained-equality branch assembly.
/// Evaluates the subject expression once, saves it to a temporary stack slot,
/// compares it against each case pattern, and dispatches to the matching body label.
/// The default label is pushed onto the loop stack so that break exits correctly.
pub(super) fn emit_switch_stmt(
    subject: &Expr,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: &Option<Vec<Stmt>>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    switch_stmt::emit_switch_stmt(subject, cases, default, emitter, ctx, data)
}
