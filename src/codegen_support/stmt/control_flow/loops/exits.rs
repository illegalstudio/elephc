//! Purpose:
//! Lowers break and continue lowering with nested depth and cleanup handling.
//! Works with loop labels stored in codegen context during nested body emission.
//!
//! Called from:
//! - `crate::codegen_support::stmt::control_flow::loops`
//!
//! Key details:
//! - Loop exits must jump to the correct depth while preserving cleanup for skipped constructs.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::{
    coerce_result_to_type, emit_expr, string_result_is_owned_call_temp,
};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits a `break` statement targeting `levels` loops up in nested loop depth.
/// Resolves the target loop's break label, cleans up skipped switch subjects, and
/// branches through `finally` blocks if present.
pub(crate) fn emit_break_stmt(levels: usize, emitter: &mut Emitter, ctx: &Context) {
    let (labels, sp_adjust) = target_loop_labels(ctx, levels, "break");
    emit_skipped_switch_stack_cleanup(emitter, sp_adjust);
    if !ctx.finally_stack.is_empty() {
        super::super::emit_branch_through_finally(emitter, ctx, &labels.break_label);
    } else {
        crate::codegen_support::abi::emit_jump(emitter, &labels.break_label);            // unconditional branch to loop exit label
    }
}

/// Emits a `return` statement, evaluating the optional expression, coercing to the
/// function's return type, releasing temporary stack slots for switch subjects,
/// and branching to the function epilogue (or through `finally` blocks if present).
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
        let target_ty = ctx.return_type.clone();
        let release_string_original = matches!(ty, PhpType::Str)
            && string_result_is_owned_call_temp(e, ctx);
        if crate::codegen_support::expr::can_coerce_result_to_type(&ty, &target_ty) {
            let release_mixed_after_coerce = !matches!(target_ty, PhpType::Mixed | PhpType::Union(_))
                && super::super::super::helpers::should_release_owned_mixed_after_coerce(
                    e,
                    &ty,
                    &target_ty,
                );
            if release_mixed_after_coerce {
                crate::codegen_support::abi::emit_push_reg(
                    emitter,
                    crate::codegen_support::abi::int_result_reg(emitter),
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
        if matches!(target_ty, PhpType::Str) {
            persist_string_return_result(emitter, release_string_original);
        }
    }
    if let Some(label) = &ctx.return_label {
        let sp_total: usize = ctx.loop_stack.iter().map(|l| l.sp_adjust).sum();
        if sp_total > 0 {
            crate::codegen_support::abi::emit_release_temporary_stack(emitter, sp_total); // pop switch subjects before returning
        }
        if !ctx.finally_stack.is_empty() {
            super::super::exceptions::emit_return_through_finally(emitter, ctx);
        } else {
            crate::codegen_support::abi::emit_jump(emitter, label);                      // branch to function epilogue for stack cleanup and ret
        }
    }
}

/// Emits `__rt_str_persist` to persist a string return value before locals are freed.
/// If `release_original` is true, the original owned string (stored at a fixed stack
/// offset of 16 bytes) is copied and the original heap allocation is released afterward.
/// Returns the persisted string in `x1`/`x2` registers.
fn persist_string_return_result(emitter: &mut Emitter, release_original: bool) {
    if !release_original {
        crate::codegen_support::abi::emit_call_label(emitter, "__rt_str_persist");       // persist borrowed or concat-buffer string before locals are freed
        return;
    }

    let (ptr_reg, len_reg) = crate::codegen_support::abi::string_result_regs(emitter);
    crate::codegen_support::abi::emit_push_reg(emitter, ptr_reg);                        // preserve owned string-call temporary while copying the return value
    crate::codegen_support::abi::emit_call_label(emitter, "__rt_str_persist");           // copy the returned string into storage owned by the caller
    crate::codegen_support::abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);          // keep the persisted return string live while freeing the original
    crate::codegen_support::abi::emit_load_temporary_stack_slot(
        emitter,
        crate::codegen_support::abi::int_result_reg(emitter),
        16,
    );
    crate::codegen_support::abi::emit_call_label(emitter, "__rt_heap_free_safe");        // release the pre-persist owned call result when it came from heap storage
    crate::codegen_support::abi::emit_pop_reg_pair(emitter, ptr_reg, len_reg);           // restore the persisted return string result
    crate::codegen_support::abi::emit_release_temporary_stack(emitter, 16);              // discard the saved original string pointer
}

/// Emits a `continue` statement targeting `levels` loops up in nested loop depth.
/// Resolves the target loop's continue label, cleans up skipped switch subjects, and
/// branches through `finally` blocks if present.
pub(crate) fn emit_continue_stmt(levels: usize, emitter: &mut Emitter, ctx: &Context) {
    let (labels, sp_adjust) = target_loop_labels(ctx, levels, "continue");
    emit_skipped_switch_stack_cleanup(emitter, sp_adjust);
    if !ctx.finally_stack.is_empty() {
        super::super::emit_branch_through_finally(emitter, ctx, &labels.continue_label);
    } else {
        crate::codegen_support::abi::emit_jump(emitter, &labels.continue_label);         // unconditional branch to loop continue label
    }
}

/// Resolves the loop-labels entry and accumulated `sp_adjust` for a break/continue
/// targeting `levels` loops up. `keyword` is used only for the panic message.
/// Returns the target `LoopLabels` and the sum of `sp_adjust` values from all
/// intermediate loops that will be skipped by the exit.
fn target_loop_labels<'a>(
    ctx: &'a Context,
    levels: usize,
    keyword: &str,
) -> (&'a crate::codegen_support::context::LoopLabels, usize) {
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

/// Releases temporary stack slots for switch subjects skipped by a multi-level
/// loop exit. Called when `sp_adjust > 0` to pop switch subject slots from the
/// temporary stack before branching to the target loop's label.
fn emit_skipped_switch_stack_cleanup(emitter: &mut Emitter, sp_adjust: usize) {
    if sp_adjust > 0 {
        crate::codegen_support::abi::emit_release_temporary_stack(emitter, sp_adjust);     // release switch subject slots skipped by a multi-level loop exit
    }
}
