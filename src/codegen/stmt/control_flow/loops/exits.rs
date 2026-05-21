//! Purpose:
//! Lowers break and continue lowering with nested depth and cleanup handling.
//! Works with loop labels stored in codegen context during nested body emission.
//!
//! Called from:
//! - `crate::codegen::stmt::control_flow::loops`
//!
//! Key details:
//! - Loop exits must jump to the correct depth while preserving cleanup for skipped constructs.

use crate::codegen::context::{Context, HeapOwnership};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{
    coerce_result_to_type, emit_expr, expr_result_heap_ownership,
    string_result_is_owned_call_temp, string_result_uses_transient_concat_buffer,
};
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub(crate) fn emit_break_stmt(levels: usize, emitter: &mut Emitter, ctx: &Context) {
    let (labels, sp_adjust) = target_loop_labels(ctx, levels, "break");
    emit_skipped_switch_stack_cleanup(emitter, sp_adjust);
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
        if matches!(ty, PhpType::Str)
            && (expr_result_heap_ownership(e) != HeapOwnership::Owned
                || string_result_uses_transient_concat_buffer(e))
        {
            persist_string_return_result(
                emitter,
                string_result_is_owned_call_temp(e, ctx),
            );
        }
        let target_ty = ctx.return_type.clone();
        if crate::codegen::expr::can_coerce_result_to_type(&ty, &target_ty) {
            let release_mixed_after_coerce = !matches!(target_ty, PhpType::Mixed | PhpType::Union(_))
                && super::super::super::helpers::should_release_owned_mixed_after_coerce(
                    e,
                    &ty,
                    &target_ty,
                );
            if release_mixed_after_coerce {
                crate::codegen::abi::emit_push_reg(
                    emitter,
                    crate::codegen::abi::int_result_reg(emitter),
                );
            }
            coerce_result_to_type(emitter, ctx, data, &ty, &target_ty);
            if release_mixed_after_coerce {
                super::super::super::helpers::release_preserved_mixed_after_coercion(
                    emitter,
                    &target_ty,
                );
            }
        }
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

fn persist_string_return_result(emitter: &mut Emitter, release_original: bool) {
    if !release_original {
        crate::codegen::abi::emit_call_label(emitter, "__rt_str_persist");       // persist borrowed or concat-buffer string before locals are freed
        return;
    }

    let (ptr_reg, len_reg) = crate::codegen::abi::string_result_regs(emitter);
    crate::codegen::abi::emit_push_reg(emitter, ptr_reg);                        // preserve owned string-call temporary while copying the return value
    crate::codegen::abi::emit_call_label(emitter, "__rt_str_persist");           // copy the returned string into storage owned by the caller
    crate::codegen::abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);          // keep the persisted return string live while freeing the original
    crate::codegen::abi::emit_load_temporary_stack_slot(
        emitter,
        crate::codegen::abi::int_result_reg(emitter),
        16,
    );
    crate::codegen::abi::emit_call_label(emitter, "__rt_heap_free_safe");        // release the pre-persist owned call result when it came from heap storage
    crate::codegen::abi::emit_pop_reg_pair(emitter, ptr_reg, len_reg);           // restore the persisted return string result
    crate::codegen::abi::emit_release_temporary_stack(emitter, 16);              // discard the saved original string pointer
}

pub(crate) fn emit_continue_stmt(levels: usize, emitter: &mut Emitter, ctx: &Context) {
    let (labels, sp_adjust) = target_loop_labels(ctx, levels, "continue");
    emit_skipped_switch_stack_cleanup(emitter, sp_adjust);
    if !ctx.finally_stack.is_empty() {
        super::super::emit_branch_through_finally(emitter, ctx, &labels.continue_label);
    } else {
        crate::codegen::abi::emit_jump(emitter, &labels.continue_label);         // unconditional branch to loop continue label
    }
}

fn target_loop_labels<'a>(
    ctx: &'a Context,
    levels: usize,
    keyword: &str,
) -> (&'a crate::codegen::context::LoopLabels, usize) {
    let index = ctx.loop_stack.len().checked_sub(levels).unwrap_or_else(|| {
        panic!(
            "codegen bug: {} statement targets {} levels with only {} active targets",
            keyword,
            levels,
            ctx.loop_stack.len()
        )
    });
    let sp_adjust = ctx.loop_stack[index + 1..]
        .iter()
        .map(|labels| labels.sp_adjust)
        .sum();
    (&ctx.loop_stack[index], sp_adjust)
}

fn emit_skipped_switch_stack_cleanup(emitter: &mut Emitter, sp_adjust: usize) {
    if sp_adjust > 0 {
        crate::codegen::abi::emit_release_temporary_stack(emitter, sp_adjust);     // release switch subject slots skipped by a multi-level loop exit
    }
}
