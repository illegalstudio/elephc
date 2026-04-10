use crate::codegen::context::{Context, HeapOwnership};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{coerce_result_to_type, emit_expr, expr_result_heap_ownership};
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub(crate) fn emit_break_stmt(emitter: &mut Emitter, ctx: &Context) {
    let labels = ctx
        .loop_stack
        .last()
        .expect("codegen bug: break statement outside loop (should have been caught by type checker)");
    if !ctx.finally_stack.is_empty() {
        super::super::emit_branch_through_finally(emitter, ctx, &labels.break_label);
    } else {
        crate::codegen::abi::emit_jump(emitter, &labels.break_label);            // unconditional branch to loop exit label
    }
}

pub(crate) fn emit_return_stmt(
    expr: &Option<Expr>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment("return");
    if let Some(e) = expr {
        let ty = emit_expr(e, emitter, ctx, data);
        super::super::super::helpers::retain_borrowed_heap_result(emitter, e, &ty);
        if matches!(ty, PhpType::Str) && expr_result_heap_ownership(e) != HeapOwnership::Owned {
            crate::codegen::abi::emit_call_label(emitter, "__rt_str_persist");   // persist borrowed string before locals are freed
        }
        let target_ty = ctx.return_type.clone();
        coerce_result_to_type(emitter, ctx, data, &ty, &target_ty);
    }
    if let Some(label) = &ctx.return_label {
        let sp_total: usize = ctx.loop_stack.iter().map(|l| l.sp_adjust).sum();
        if sp_total > 0 {
            crate::codegen::abi::emit_release_temporary_stack(emitter, sp_total); // pop switch subjects before returning
        }
        if !ctx.finally_stack.is_empty() {
            super::super::exceptions::emit_return_through_finally(emitter, ctx);
        } else {
            crate::codegen::abi::emit_jump(emitter, label);                      // branch to function epilogue for stack cleanup and ret
        }
    }
}

pub(crate) fn emit_continue_stmt(emitter: &mut Emitter, ctx: &Context) {
    let labels = ctx
        .loop_stack
        .last()
        .expect("codegen bug: continue statement outside loop (should have been caught by type checker)");
    if !ctx.finally_stack.is_empty() {
        super::super::emit_branch_through_finally(emitter, ctx, &labels.continue_label);
    } else {
        crate::codegen::abi::emit_jump(emitter, &labels.continue_label);         // unconditional branch to loop continue label
    }
}
