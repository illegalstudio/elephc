//! Purpose:
//! Body-statement and control-flow lowering for the generator state machine.
//! Walks the `ResumeNode` tree, emitting straight-line statements, branches,
//! loops, switch dispatch, and the `Bail` short-circuit to the terminator.
//!
//! Called from:
//!  - `super::dispatcher::emit_resume()` for the body entry-point walk.
//!  - Recursive `emit_nodes` calls within each control-flow node.
//!
//! Key details:
//!  - Yield-bearing nodes (`Yield`, `YieldAssign`, `YieldFromGenerator`) are
//!    delegated to `yields` so the suspension/resume mechanics stay in one
//!    place.
//!  - Loop labels live on `ResumeCtx::loop_stack` so `break`/`continue` can
//!    walk back to the nearest enclosing target; outside a loop both fall
//!    through to the terminator.
//!  - `Switch` re-uses the loop stack so `break` inside a case escapes the
//!    switch end, matching PHP fall-through semantics.

use super::values::{
    emit_branch_if_false, emit_box_mixed_source, emit_load_int_source, emit_replace_mixed_slot,
};
use super::yields::{
    emit_yield, emit_yield_assign_store_mixed, emit_yield_assign_unbox_int,
    emit_yield_from_generator,
};
use super::{slot_offset, LoopLabels, ResumeCtx};
use super::super::model::*;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::runtime::generators::frame as gen_frame;

pub(super) fn emit_body_stmt(emitter: &mut Emitter, stmt: &BodyStmt) {
    match stmt {
        BodyStmt::AssignInt(idx, src) => {
            match emitter.target.arch {
                Arch::AArch64 => {
                    emit_load_int_source(emitter, "x1", src);
                    emitter.instruction(&format!("str x1, [x19, #{}]", slot_offset(*idx))); // store the computed int into the local's slot
                }
                Arch::X86_64 => {
                    emit_load_int_source(emitter, "r10", src);
                    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], r10", slot_offset(*idx))); // store the computed int into the local's slot
                }
            }
        }
        BodyStmt::AssignMixed(idx, src) => {
            // Mixed slots use the standard refcount-replace pattern: park
            // the previous Mixed pointer in x20, materialise the new
            // boxed pointer in x0, store, then decref the previous.
            let off = slot_offset(*idx);
            emit_replace_mixed_slot(emitter, off, |em| emit_box_mixed_source(em, src));
        }
        BodyStmt::PostIncrement(idx) => {
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction(&format!("ldr x1, [x19, #{}]", slot_offset(*idx))); // load the local's current value
                    emitter.instruction("add x1, x1, #1");                      // increment the value by 1
                    emitter.instruction(&format!("str x1, [x19, #{}]", slot_offset(*idx))); // write the incremented value back to the slot
                }
                Arch::X86_64 => {
                    emitter.instruction(&format!("mov r10, QWORD PTR [r12 + {}]", slot_offset(*idx))); // load the local's current value
                    emitter.instruction("add r10, 1");                          // increment the value by 1
                    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], r10", slot_offset(*idx))); // write the incremented value back to the slot
                }
            }
        }
        BodyStmt::PostDecrement(idx) => {
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction(&format!("ldr x1, [x19, #{}]", slot_offset(*idx))); // load the local's current value
                    emitter.instruction("sub x1, x1, #1");                      // decrement the value by 1
                    emitter.instruction(&format!("str x1, [x19, #{}]", slot_offset(*idx))); // write the decremented value back to the slot
                }
                Arch::X86_64 => {
                    emitter.instruction(&format!("mov r10, QWORD PTR [r12 + {}]", slot_offset(*idx))); // load the local's current value
                    emitter.instruction("sub r10, 1");                          // decrement the value by 1
                    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], r10", slot_offset(*idx))); // write the decremented value back to the slot
                }
            }
        }
    }
}

fn emit_jump(emitter: &mut Emitter, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("b {}", label));                       // jump to the requested generator label
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("jmp {}", label));                     // jump to the requested generator label
        }
    }
}

fn emit_jump_eq(emitter: &mut Emitter, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("b.eq {}", label));                    // jump when the preceding comparison matched
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("je {}", label));                      // jump when the preceding comparison matched
        }
    }
}

pub(super) fn emit_nodes(emitter: &mut Emitter, nodes: &[ResumeNode], ctx: &mut ResumeCtx) {
    for node in nodes {
        emit_node(emitter, node, ctx);
    }
}

fn emit_node(emitter: &mut Emitter, node: &ResumeNode, ctx: &mut ResumeCtx) {
    match node {
        ResumeNode::Stmt(s) => emit_body_stmt(emitter, s),
        ResumeNode::Yield(entry, state_idx) => emit_yield(emitter, entry, *state_idx, ctx, true),
        ResumeNode::YieldAssign { local_idx, local_ty, yield_entry, state_idx } => {
            emit_yield(emitter, yield_entry, *state_idx, ctx, false);
            match local_ty {
                SlotType::Int => emit_yield_assign_unbox_int(emitter, *local_idx, ctx),
                SlotType::Mixed => emit_yield_assign_store_mixed(emitter, *local_idx, ctx),
            }
        }
        ResumeNode::If { cond, then_body, else_body } => {
            let else_lbl = ctx.fresh_label("if_else");
            let end_lbl = ctx.fresh_label("if_end");
            emit_branch_if_false(emitter, cond, &else_lbl);
            emit_nodes(emitter, then_body, ctx);
            emit_jump(emitter, &end_lbl);
            emitter.label(&else_lbl);
            emit_nodes(emitter, else_body, ctx);
            emitter.label(&end_lbl);
        }
        ResumeNode::For { init, cond, update, body } => {
            emit_nodes(emitter, init, ctx);
            let test_lbl = ctx.fresh_label("for_test");
            let cont_lbl = ctx.fresh_label("for_cont");
            let end_lbl = ctx.fresh_label("for_end");
            emitter.label(&test_lbl);
            emit_branch_if_false(emitter, cond, &end_lbl);
            ctx.loop_stack.push(LoopLabels { end: end_lbl.clone(), cont: cont_lbl.clone() });
            emit_nodes(emitter, body, ctx);
            ctx.loop_stack.pop();
            emitter.label(&cont_lbl);
            emit_nodes(emitter, update, ctx);
            emit_jump(emitter, &test_lbl);
            emitter.label(&end_lbl);
        }
        ResumeNode::While { cond, body } => {
            let top_lbl = ctx.fresh_label("while_top");
            let end_lbl = ctx.fresh_label("while_end");
            emitter.label(&top_lbl);
            emit_branch_if_false(emitter, cond, &end_lbl);
            ctx.loop_stack.push(LoopLabels { end: end_lbl.clone(), cont: top_lbl.clone() });
            emit_nodes(emitter, body, ctx);
            ctx.loop_stack.pop();
            emit_jump(emitter, &top_lbl);
            emitter.label(&end_lbl);
        }
        ResumeNode::DoWhile { cond, body } => {
            let top_lbl = ctx.fresh_label("do_top");
            let cond_lbl = ctx.fresh_label("do_cond");
            let end_lbl = ctx.fresh_label("do_end");
            emitter.label(&top_lbl);
            ctx.loop_stack.push(LoopLabels { end: end_lbl.clone(), cont: cond_lbl.clone() });
            emit_nodes(emitter, body, ctx);
            ctx.loop_stack.pop();
            emitter.label(&cond_lbl);
            emit_branch_if_false(emitter, cond, &end_lbl);
            emit_jump(emitter, &top_lbl);
            emitter.label(&end_lbl);
        }
        ResumeNode::Break => emit_loop_jump(emitter, ctx, /* break_jump */ true),
        ResumeNode::Continue => emit_loop_jump(emitter, ctx, /* break_jump */ false),
        ResumeNode::Switch { subject, cases, default } => {
            emit_switch(emitter, subject, cases, default, ctx);
        }
        ResumeNode::YieldFromGenerator { source, state_idx, result_local } => {
            emit_yield_from_generator(emitter, source, *state_idx, *result_local, ctx);
        }
        ResumeNode::Return(value) => {
            // Box the return value (if any) into the frame's return_value
            // slot using the standard refcount-replace pattern, then jump
            // to the terminator which sets the TERMINATED flag.
            if let Some(src) = value {
                emit_replace_mixed_slot(emitter, gen_frame::OFF_RETURN_VALUE, |em| {
                    emit_box_mixed_source(em, src);
                });
            }
            let term = ctx.term_label.clone();
            emit_jump(emitter, &term);
        }
        ResumeNode::Block { stmts } => emit_nodes(emitter, stmts, ctx),
        ResumeNode::Bail => {
            let term = ctx.term_label.clone();
            emit_jump(emitter, &term);
        }
    }
}

/// Branch out of the innermost loop (break) or back to its `cont` label
/// (continue). Outside a loop, fall through to the terminator.
fn emit_loop_jump(emitter: &mut Emitter, ctx: &mut ResumeCtx, break_jump: bool) {
    let target = ctx.loop_stack.last().map(|labels| {
        if break_jump { labels.end.clone() } else { labels.cont.clone() }
    });
    let lbl = target.unwrap_or_else(|| ctx.term_label.clone());
    emit_jump(emitter, &lbl);
}

fn emit_switch(
    emitter: &mut Emitter,
    subject: &IntSource,
    cases: &[(Vec<i64>, Vec<ResumeNode>)],
    default: &[ResumeNode],
    ctx: &mut ResumeCtx,
) {
    let end_lbl = ctx.fresh_label("switch_end");
    let default_lbl = ctx.fresh_label("switch_default");
    let case_labels: Vec<String> = (0..cases.len())
        .map(|_| ctx.fresh_label("switch_case"))
        .collect();
    // Evaluate subject once into x1, then dispatch to the matching case.
    let subject_reg = match emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "r10",
    };
    emit_load_int_source(emitter, subject_reg, subject);
    for (i, (values, _)) in cases.iter().enumerate() {
        for v in values {
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction(&format!("mov x2, #{}", v));            // load this case literal into the comparison register
                    emitter.instruction("cmp x1, x2");                          // compare subject against the case literal
                }
                Arch::X86_64 => {
                    emitter.instruction(&format!("mov r11, {}", v));            // load this case literal into the comparison register
                    emitter.instruction("cmp r10, r11");                        // compare subject against the case literal
                }
            }
            emit_jump_eq(emitter, &case_labels[i]);
        }
    }
    emit_jump(emitter, &default_lbl);
    // Cases fall through unless their body breaks.
    ctx.loop_stack.push(LoopLabels {
        end: end_lbl.clone(),
        cont: end_lbl.clone(),
    });
    for (i, (_, body)) in cases.iter().enumerate() {
        emitter.label(&case_labels[i]);
        emit_nodes(emitter, body, ctx);
    }
    emitter.label(&default_lbl);
    emit_nodes(emitter, default, ctx);
    ctx.loop_stack.pop();
    emitter.label(&end_lbl);
}
