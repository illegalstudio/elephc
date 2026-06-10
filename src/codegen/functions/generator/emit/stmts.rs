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
use super::{preserved_scratch_reg, slot_offset, LoopLabels, ResumeCtx};
use super::super::model::*;
use crate::codegen::abi;
use crate::codegen::NULL_SENTINEL;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::runtime::generators::frame as gen_frame;

/// Emits a single generator body statement. Mutations use the standard
/// refcount-replace pattern for Mixed slots to maintain ownership safety.
/// The slot index is converted to a stack offset via `slot_offset`.
pub(super) fn emit_body_stmt(
    emitter: &mut Emitter,
    data: &mut DataSection,
    stmt: &BodyStmt,
    ctx: &mut ResumeCtx,
) {
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
        BodyStmt::EchoMixed(src) => emit_echo_mixed(emitter, src),
        BodyStmt::VarDumpMixed(src) => emit_var_dump_mixed(emitter, data, src, ctx),
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

/// Emits a generator-body `var_dump` for a Mixed-capable source.
///
/// The value is first boxed into an owned Mixed cell so runtime type dispatch
/// is uniform for locals, `yield from` returns, strings, ints, and nulls.
/// After formatting, the temporary box is released.
fn emit_var_dump_mixed(
    emitter: &mut Emitter,
    data: &mut DataSection,
    src: &MixedSource,
    ctx: &mut ResumeCtx,
) {
    emit_box_mixed_source(emitter, src);
    let result_reg = abi::int_result_reg(emitter);
    let saved_reg = preserved_scratch_reg(emitter);
    emitter.instruction(&format!("mov {}, {}", saved_reg, result_reg));         // preserve the temporary Mixed box across diagnostic formatting
    emit_var_dump_boxed_mixed(emitter, data, ctx);
    emitter.instruction(&format!("mov {}, {}", result_reg, saved_reg));         // restore the temporary Mixed box for release
    abi::emit_call_label(emitter, "__rt_decref_mixed");                        // release the temporary Mixed box after var_dump prints it
}

/// Emits type dispatch for a boxed Mixed value in the active integer result register.
///
/// Generator IR currently formats int, string, and null Mixed payloads here.
/// Unknown tags conservatively print `NULL`, matching the narrow generator IR
/// value surface rather than accepting unsupported runtime shapes.
fn emit_var_dump_boxed_mixed(
    emitter: &mut Emitter,
    data: &mut DataSection,
    ctx: &mut ResumeCtx,
) {
    let int_case = ctx.fresh_label("vd_mixed_int");
    let string_case = ctx.fresh_label("vd_mixed_string");
    let null_case = ctx.fresh_label("vd_mixed_null");
    let done = ctx.fresh_label("vd_mixed_done");

    abi::emit_call_label(emitter, "__rt_mixed_unbox");                         // unwrap the boxed mixed payload before formatting it
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // does the mixed payload hold an int?
            emit_jump_eq(emitter, &int_case);
            emitter.instruction("cmp x0, #1");                                  // does the mixed payload hold a string?
            emit_jump_eq(emitter, &string_case);
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 0");                                  // does the mixed payload hold an int?
            emit_jump_eq(emitter, &int_case);
            emitter.instruction("cmp rax, 1");                                  // does the mixed payload hold a string?
            emit_jump_eq(emitter, &string_case);
        }
    }
    emit_jump(emitter, &null_case);

    emitter.label(&int_case);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, x1");                                  // move the unboxed int payload into the integer result register
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, rdi");                                // move the unboxed int payload into the integer result register
        }
    }
    emit_var_dump_int(emitter, data, ctx);
    emit_jump(emitter, &done);

    emitter.label(&string_case);
    match emitter.target.arch {
        Arch::AArch64 => {}
        Arch::X86_64 => {
            emitter.instruction("mov rax, rdi");                                // move the unboxed string pointer into the string pointer register
            emitter.instruction("mov rdx, rsi");                                // move the unboxed string length into the string length register
        }
    }
    emit_var_dump_string(emitter, data);
    emit_jump(emitter, &done);

    emitter.label(&null_case);
    emit_write_literal(emitter, data, b"NULL\n");
    emitter.label(&done);
}

/// Emits `var_dump` output for an integer payload in the active result register.
fn emit_var_dump_int(emitter: &mut Emitter, data: &mut DataSection, ctx: &mut ResumeCtx) {
    if crate::codegen::sentinels::null_repr_is_tagged() {
        // Under the tagged representation a plain Int is never null; print the payload
        // directly so the full i64 range (including PHP_INT_MAX - 1) round-trips.
        let result_reg = abi::int_result_reg(emitter);
        abi::emit_push_reg(emitter, result_reg);                                // preserve the integer payload before prefix writes clobber the result register
        emit_write_literal(emitter, data, b"int(");
        abi::emit_pop_reg(emitter, result_reg);                                 // restore the integer payload after the prefix write
        abi::emit_call_label(emitter, "__rt_itoa");                             // convert the integer payload to decimal text
        emit_write_current_string(emitter);
        emit_write_literal(emitter, data, b")\n");
        return;
    }
    let not_null = ctx.fresh_label("vd_not_null");
    let done = ctx.fresh_label("vd_done");
    let result_reg = abi::int_result_reg(emitter);
    let scratch_reg = abi::symbol_scratch_reg(emitter);
    abi::emit_load_int_immediate(emitter, scratch_reg, NULL_SENTINEL); // materialize the shared null sentinel used by int-valued locals
    emitter.instruction(&format!("cmp {}, {}", result_reg, scratch_reg));       // compare the incoming integer payload against the null sentinel
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("b.ne {}", not_null));                 // branch to the ordinary int path when the payload is not null
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("jne {}", not_null));                  // branch to the ordinary int path when the payload is not null
        }
    }
    emit_write_literal(emitter, data, b"NULL\n");
    emit_jump(emitter, &done);
    emitter.label(&not_null);
    abi::emit_push_reg(emitter, result_reg);                                    // preserve the integer payload before prefix writes clobber the result register
    emit_write_literal(emitter, data, b"int(");
    abi::emit_pop_reg(emitter, result_reg);                                     // restore the integer payload after the prefix write
    abi::emit_call_label(emitter, "__rt_itoa");                                 // convert the integer payload to decimal text
    emit_write_current_string(emitter);
    emit_write_literal(emitter, data, b")\n");
    emitter.label(&done);
}

/// Emits `var_dump` output for a string payload in the active string result registers.
fn emit_var_dump_string(emitter: &mut Emitter, data: &mut DataSection) {
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);                         // preserve the original string payload while printing the type prefix
    emit_write_literal(emitter, data, b"string(");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x0, [sp, #8]");                            // load the preserved string length without consuming the saved payload pair
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, QWORD PTR [rsp + 8]");                // load the preserved string length without consuming the saved payload pair
        }
    }
    abi::emit_call_label(emitter, "__rt_itoa");                                 // convert the string length to decimal text
    emit_write_current_string(emitter);
    emit_write_literal(emitter, data, b") \"");
    abi::emit_pop_reg_pair(emitter, ptr_reg, len_reg);                          // restore the original string payload after the prefix writes finish
    emit_write_current_string(emitter);
    emit_write_literal(emitter, data, b"\"\n");
}

/// Emits a compile-time literal string to stdout from generator-body helpers.
fn emit_write_literal(emitter: &mut Emitter, data: &mut DataSection, bytes: &[u8]) {
    let (lbl, len) = data.add_string(bytes);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.adrp("x1", &lbl);                                           // load the page that contains the literal string bytes
            emitter.add_lo12("x1", "x1", &lbl);                                 // resolve the literal string address within that page
            emitter.instruction(&format!("mov x2, #{}", len));                  // pass the literal string length to write()
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.syscall(4);
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("lea rsi, [rip + {}]", lbl));          // point the write buffer register at the literal string bytes
            emitter.instruction(&format!("mov edx, {}", len));                  // pass the literal string length to write()
            emitter.instruction("mov edi, 1");                                  // fd = stdout
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // write the literal bytes directly to stdout
        }
    }
}

/// Writes the current string result register pair to stdout.
fn emit_write_current_string(emitter: &mut Emitter) {
    abi::emit_write_stdout(emitter, &crate::types::PhpType::Str);
}

/// Emits a generator-body `echo` by boxing the source as Mixed, writing it with
/// PHP echo semantics, and releasing the temporary box afterward.
fn emit_echo_mixed(emitter: &mut Emitter, src: &MixedSource) {
    emit_box_mixed_source(emitter, src);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x20, x0");                                 // preserve the temporary Mixed box until echo finishes
            abi::emit_call_label(emitter, "__rt_mixed_write_stdout");           // write the boxed value to stdout using PHP echo semantics
            emitter.instruction("mov x0, x20");                                 // restore the temporary Mixed box for release
            abi::emit_call_label(emitter, "__rt_decref_mixed");                 // release the temporary Mixed box created for echo
        }
        Arch::X86_64 => {
            emitter.instruction("mov r13, rax");                                // preserve the temporary Mixed box until echo finishes
            abi::emit_call_label(emitter, "__rt_mixed_write_stdout");           // write the boxed value to stdout using PHP echo semantics
            emitter.instruction("mov rax, r13");                                // restore the temporary Mixed box for release
            abi::emit_call_label(emitter, "__rt_decref_mixed");                 // release the temporary Mixed box created for echo
        }
    }
}

/// Emits a jump instruction to the given label name, using `b` on ARM64
/// and `jmp` on x86_64.
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

/// Emits a conditional jump (`b.eq` / `je`) that transfers control when the
/// preceding comparison result was equal. Used by `emit_switch` after `cmp`.
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

/// Walks a slice of `ResumeNode` items in source order, delegating each to
/// `emit_node`. Used for top-level body walks and nested control-flow bodies.
pub(super) fn emit_nodes(
    emitter: &mut Emitter,
    data: &mut DataSection,
    nodes: &[ResumeNode],
    ctx: &mut ResumeCtx,
) {
    for node in nodes {
        emit_node(emitter, data, node, ctx);
    }
}

/// Dispatches a single `ResumeNode` variant to the appropriate emitter.
/// Handles all statement types, control-flow constructs, and the special
/// `Yield`/`YieldFromGenerator` nodes that suspend the generator.
/// `break`/`continue` use `emit_loop_jump` to respect `loop_stack`.
fn emit_node(
    emitter: &mut Emitter,
    data: &mut DataSection,
    node: &ResumeNode,
    ctx: &mut ResumeCtx,
) {
    match node {
        ResumeNode::Stmt(s) => emit_body_stmt(emitter, data, s, ctx),
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
            emit_nodes(emitter, data, then_body, ctx);
            emit_jump(emitter, &end_lbl);
            emitter.label(&else_lbl);
            emit_nodes(emitter, data, else_body, ctx);
            emitter.label(&end_lbl);
        }
        ResumeNode::For { init, cond, update, body } => {
            emit_nodes(emitter, data, init, ctx);
            let test_lbl = ctx.fresh_label("for_test");
            let cont_lbl = ctx.fresh_label("for_cont");
            let end_lbl = ctx.fresh_label("for_end");
            emitter.label(&test_lbl);
            emit_branch_if_false(emitter, cond, &end_lbl);
            ctx.loop_stack.push(LoopLabels { end: end_lbl.clone(), cont: cont_lbl.clone() });
            emit_nodes(emitter, data, body, ctx);
            ctx.loop_stack.pop();
            emitter.label(&cont_lbl);
            emit_nodes(emitter, data, update, ctx);
            emit_jump(emitter, &test_lbl);
            emitter.label(&end_lbl);
        }
        ResumeNode::While { cond, body } => {
            let top_lbl = ctx.fresh_label("while_top");
            let end_lbl = ctx.fresh_label("while_end");
            emitter.label(&top_lbl);
            emit_branch_if_false(emitter, cond, &end_lbl);
            ctx.loop_stack.push(LoopLabels { end: end_lbl.clone(), cont: top_lbl.clone() });
            emit_nodes(emitter, data, body, ctx);
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
            emit_nodes(emitter, data, body, ctx);
            ctx.loop_stack.pop();
            emitter.label(&cond_lbl);
            emit_branch_if_false(emitter, cond, &end_lbl);
            emit_jump(emitter, &top_lbl);
            emitter.label(&end_lbl);
        }
        ResumeNode::Break => emit_loop_jump(emitter, ctx, /* break_jump */ true),
        ResumeNode::Continue => emit_loop_jump(emitter, ctx, /* break_jump */ false),
        ResumeNode::Switch { subject, cases, default } => {
            emit_switch(emitter, data, subject, cases, default, ctx);
        }
        ResumeNode::Try { try_body, finally_body } => {
            emit_nodes(emitter, data, try_body, ctx);
            emit_nodes(emitter, data, finally_body, ctx);
        }
        ResumeNode::YieldFromGenerator { source, state_idx, result } => {
            emit_yield_from_generator(emitter, source, *state_idx, *result, ctx);
            if matches!(result, YieldFromResult::Return) {
                let term = ctx.term_label.clone();
                emit_jump(emitter, &term);
            }
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
        ResumeNode::Block { stmts } => emit_nodes(emitter, data, stmts, ctx),
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

/// Emits a switch/case dispatch for integer subjects. Evaluates `subject`
/// once into a scratch register (x1 on ARM64, r10 on x86_64), then emits
/// a sequence of `cmp`/`b.eq` tests for each case value. Unmatched subjects
/// jump to `default`. Cases push a synthetic `LoopLabels { end, cont }` entry
/// onto `loop_stack` so that `break` inside a case exits the switch end,
/// matching PHP fall-through semantics.
fn emit_switch(
    emitter: &mut Emitter,
    data: &mut DataSection,
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
        emit_nodes(emitter, data, body, ctx);
    }
    emitter.label(&default_lbl);
    emit_nodes(emitter, data, default, ctx);
    ctx.loop_stack.pop();
    emitter.label(&end_lbl);
}
