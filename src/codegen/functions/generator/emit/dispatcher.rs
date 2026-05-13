//! Purpose:
//! Emits the `_fn_<f>__resume` state-machine entry point: prologue, state-idx
//! dispatch table, body invocation through `stmts::emit_nodes`, and the
//! shared terminator/epilogue that releases retained Mixed slots.
//!
//! Called from:
//!  - `crate::codegen::functions::generator::emit_generator_function()` via
//!    the parent module's `emit_resume` re-export.
//!
//! Key details:
//!  - State 0 is the body entry; states 1..N each have a corresponding
//!    `<label>_resume_N` label emitted by `yields::emit_yield`.
//!  - The terminator path sets `FLAG_TERMINATED`, then iterates the recorded
//!    Mixed-typed local slot indices and decrefs them — without this the
//!    generator would leak a cell per slot that ever held a yielded array
//!    or string.

use super::stmts::emit_nodes;
use super::{slot_offset, LoopLabels, ResumeCtx};
use super::super::model::ResumeNode;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::runtime::generators::frame as gen_frame;

pub(in crate::codegen::functions::generator) fn emit_resume(
    emitter: &mut Emitter,
    label: &str,
    nodes: &[ResumeNode],
    highest_state: u32,
    mixed_slot_indices: &[usize],
) {
    if emitter.target.arch == Arch::X86_64 {
        emit_resume_x86_64(emitter, label, nodes, highest_state, mixed_slot_indices);
        return;
    }

    emitter.blank();
    emitter.comment(&format!("--- generator resume {} ---", label));
    emitter.label_global(label);

    emitter.instruction("sub sp, sp, #32");                                     // reserve frame: 16 bytes for fp/lr + 16 for x19/x20
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("stp x19, x20, [sp, #0]");                              // save callee-saved x19/x20
    emitter.instruction("add x29, sp, #16");                                    // establish the resume function's frame pointer
    emitter.instruction("mov x19, x0");                                         // x19 = generator frame pointer

    emitter.instruction(&format!("ldr w10, [x19, #{}]", gen_frame::OFF_STATE_IDX)); // load resume state index

    let term_label = format!("{}_terminated", label);
    let end_label = format!("{}_end", label);
    let entry_label = format!("{}_entry", label);

    emitter.instruction("cmp w10, #0");                                         // state 0 → entry
    emitter.instruction(&format!("b.eq {}", entry_label));                      // dispatch to body entry on the initial resume
    for k in 1..=highest_state {
        emitter.instruction(&format!("cmp w10, #{}", k));                       // compare against a generated yield resume state
        emitter.instruction(&format!("b.eq {}_resume_{}", label, k));           // dispatch to yield K's resume label
    }
    emitter.instruction(&format!("b {}", term_label));                          // unknown state → terminate

    emitter.label(&entry_label);

    let mut ctx = ResumeCtx {
        label,
        term_label: term_label.clone(),
        end_label: end_label.clone(),
        next_label_id: 0,
        loop_stack: Vec::<LoopLabels>::new(),
    };
    emit_nodes(emitter, nodes, &mut ctx);

    emitter.instruction(&format!("b {}", term_label));                          // body fell off the end → terminate

    emitter.label(&term_label);
    // Decref any Mixed-typed locals that still hold a cell; without this
    // each generator that yielded an array/string into a slot would leak
    // that cell when the generator terminates. The flag is set first so
    // a re-entry via `next()` returns immediately without re-running.
    emitter.instruction(&format!("ldr w10, [x19, #{}]", gen_frame::OFF_FLAGS)); // load generator flags
    emitter.instruction(&format!("orr w10, w10, #{}", gen_frame::FLAG_TERMINATED)); // set TERMINATED bit
    emitter.instruction(&format!("str w10, [x19, #{}]", gen_frame::OFF_FLAGS)); // store updated flags
    for &idx in mixed_slot_indices {
        let off = slot_offset(idx);
        emitter.instruction(&format!("ldr x0, [x19, #{}]", off));               // load the Mixed pointer parked in the local slot
        emitter.instruction(&format!("str xzr, [x19, #{}]", off));              // clear the slot so a double-terminate cannot decref twice
        emitter.instruction("bl __rt_decref_mixed");                            // release our refcount on the cell (NULL is safe)
    }

    emitter.label(&end_label);
    emitter.instruction("ldp x19, x20, [sp, #0]");                              // restore callee-saved x19/x20
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the resume function's frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_resume_x86_64(
    emitter: &mut Emitter,
    label: &str,
    nodes: &[ResumeNode],
    highest_state: u32,
    mixed_slot_indices: &[usize],
) {
    emitter.blank();
    emitter.comment(&format!("--- generator resume {} ---", label));
    emitter.label_global(label);

    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the resume function frame pointer
    emitter.instruction("push r12");                                            // save generator-frame register
    emitter.instruction("push r13");                                            // save preserved mixed-slot scratch register
    emitter.instruction("push r14");                                            // save secondary preserved scratch register
    emitter.instruction("sub rsp, 8");                                          // keep the stack 16-byte aligned across nested calls
    emitter.instruction("mov r12, rdi");                                        // r12 = generator frame pointer

    emitter.instruction(&format!("mov r10d, DWORD PTR [r12 + {}]", gen_frame::OFF_STATE_IDX)); // load resume state index

    let term_label = format!("{}_terminated", label);
    let end_label = format!("{}_end", label);
    let entry_label = format!("{}_entry", label);

    emitter.instruction("cmp r10d, 0");                                         // state 0 -> entry
    emitter.instruction(&format!("je {}", entry_label));                        // dispatch to body entry on the initial resume
    for k in 1..=highest_state {
        emitter.instruction(&format!("cmp r10d, {}", k));                       // compare against a generated yield resume state
        emitter.instruction(&format!("je {}_resume_{}", label, k));             // dispatch to yield K's resume label
    }
    emitter.instruction(&format!("jmp {}", term_label));                        // unknown state -> terminate

    emitter.label(&entry_label);

    let mut ctx = ResumeCtx {
        label,
        term_label: term_label.clone(),
        end_label: end_label.clone(),
        next_label_id: 0,
        loop_stack: Vec::<LoopLabels>::new(),
    };
    emit_nodes(emitter, nodes, &mut ctx);

    emitter.instruction(&format!("jmp {}", term_label));                        // body fell off the end -> terminate

    emitter.label(&term_label);
    emitter.instruction(&format!("mov r10d, DWORD PTR [r12 + {}]", gen_frame::OFF_FLAGS)); // load generator flags
    emitter.instruction(&format!("or r10d, {}", gen_frame::FLAG_TERMINATED));   // set TERMINATED bit
    emitter.instruction(&format!("mov DWORD PTR [r12 + {}], r10d", gen_frame::OFF_FLAGS)); // store updated flags
    for &idx in mixed_slot_indices {
        let off = slot_offset(idx);
        emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", off));    // load the Mixed pointer parked in the local slot
        emitter.instruction(&format!("mov QWORD PTR [r12 + {}], 0", off));      // clear the slot so a double-terminate cannot decref twice
        emitter.instruction("call __rt_decref_mixed");                          // release our refcount on the cell (NULL is safe)
    }

    emitter.label(&end_label);
    emitter.instruction("add rsp, 8");                                          // release the alignment pad
    emitter.instruction("pop r14");                                             // restore secondary preserved scratch register
    emitter.instruction("pop r13");                                             // restore preserved mixed-slot scratch register
    emitter.instruction("pop r12");                                             // restore caller's r12
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}
