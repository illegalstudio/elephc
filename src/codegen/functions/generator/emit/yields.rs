//! Purpose:
//! Yield suspension and `Generator::send` resume mechanics. Stores the
//! yielded key/value cells into the frame, bumps `state_idx`, and emits the
//! per-yield resume label that `dispatcher::emit_resume` will branch to on
//! re-entry.
//!
//! Called from:
//!  - `super::stmts::emit_node()` for `ResumeNode::Yield`, `YieldAssign`,
//!    and `YieldFromGenerator`.
//!
//! Key details:
//!  - Every yield runs the refcount-replace pattern on both `last_key` and
//!    `last_value` so a long-running generator doesn't leak a boxed cell per
//!    yield.
//!  - `Generator::send($v)` parks the boxed Mixed pointer in the frame's
//!    `sent_value` slot before resuming. `emit_yield_assign_*` reads that
//!    slot at the resume label, transfers it into the assignment LHS, and
//!    clears `sent_value` so a subsequent `next()` does not see a stale
//!    pointer.
//!  - `emit_yield_from_generator` uses a single state index for the whole
//!    delegation loop: the first call rewinds the inner generator, each
//!    `next()` resumes at the loop continuation, and exit clears
//!    `delegated_iter` so future yields do not re-enter the inner pipeline.

use super::values::{
    emit_box_mixed_source, emit_compute_key, emit_int_function_call, emit_replace_mixed_slot,
};
use super::{slot_offset, ResumeCtx};
use super::super::model::*;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::runtime::generators::frame as gen_frame;

pub(super) fn emit_yield(
    emitter: &mut Emitter,
    entry: &YieldEntry,
    state_idx: u32,
    ctx: &mut ResumeCtx,
    clear_sent_on_resume: bool,
) {
    // Each yield overwrites both Mixed-pointer slots in the frame; we
    // refcount-drop the previous occupant so we don't leak a cell per
    // yield. `__rt_decref_mixed` tolerates NULL (the wrapper's initial
    // state).
    emit_replace_mixed_slot(emitter, gen_frame::OFF_LAST_KEY, |em| {
        emit_compute_key(em, entry.key.as_ref());
    });
    emit_replace_mixed_slot(emitter, gen_frame::OFF_LAST_VALUE, |em| {
        emit_box_mixed_source(em, &entry.value);
    });

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov w10, #{}", state_idx));           // bump state to this yield's resume index
            emitter.instruction(&format!("str w10, [x19, #{}]", gen_frame::OFF_STATE_IDX)); // store updated state_idx
            emitter.instruction(&format!("b {}", ctx.end_label));               // jump to common epilogue
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov r10d, {}", state_idx));           // bump state to this yield's resume index
            emitter.instruction(&format!("mov DWORD PTR [r12 + {}], r10d", gen_frame::OFF_STATE_IDX)); // store updated state_idx
            emitter.instruction(&format!("jmp {}", ctx.end_label));             // jump to common epilogue
        }
    }
    emitter.label(&format!("{}_resume_{}", ctx.label, state_idx));              // resume label for this yield
    if clear_sent_on_resume {
        emit_clear_sent_value(emitter);
    }
}

fn emit_clear_sent_value(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x0, [x19, #{}]", gen_frame::OFF_SENT_VALUE)); // load ignored sent_value pointer
            emitter.instruction(&format!("str xzr, [x19, #{}]", gen_frame::OFF_SENT_VALUE)); // clear sent_value before the next resume
            emitter.instruction("bl __rt_decref_mixed");                        // release the ignored sent_value box (NULL is safe)
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", gen_frame::OFF_SENT_VALUE)); // load ignored sent_value pointer
            emitter.instruction(&format!("mov QWORD PTR [r12 + {}], 0", gen_frame::OFF_SENT_VALUE)); // clear sent_value before the next resume
            emitter.instruction("call __rt_decref_mixed");                      // release the ignored sent_value box (NULL is safe)
        }
    }
}

/// Emit the runtime-delegation loop for `yield from <gen_func>(args)`.
///
/// The body is structured so that **one** state index covers every
/// iteration of the inner generator. The first call (rewind on the outer
/// generator) lands at the entry, allocates the inner generator, calls
/// its rewind, then enters the loop. Each call to outer.next() resumes
/// at the `resume_<state_idx>` label, advances the inner generator, and
/// loops back. When the inner generator is exhausted, the outer body
/// continues immediately after the `yield from`.
pub(super) fn emit_yield_from_generator(
    emitter: &mut Emitter,
    source: &YieldFromSource,
    state_idx: u32,
    ctx: &mut ResumeCtx,
) {
    if emitter.target.arch == Arch::X86_64 {
        emit_yield_from_generator_x86_64(emitter, source, state_idx, ctx);
        return;
    }

    let loop_lbl = ctx.fresh_label("yield_from_loop");
    let end_lbl = ctx.fresh_label("yield_from_end");
    let owns_delegated_iter = matches!(source, YieldFromSource::Call { .. });

    // -- materialise the inner generator pointer in x0 --
    match source {
        YieldFromSource::Call { fn_name, args } => {
            emit_int_function_call(emitter, fn_name, args);                 // x0 = inner generator pointer
        }
        YieldFromSource::IntSlot(idx) => {
            emitter.instruction(&format!("ldr x0, [x19, #{}]", slot_offset(*idx))); // x0 = raw Generator pointer (loaded from int-typed slot)
        }
        YieldFromSource::MixedSlot(idx) => {
            // The Mixed slot holds a boxed Mixed cell wrapping an Object
            // payload. Unbox to recover the raw Generator/Iterator
            // pointer; `__rt_mixed_unbox` returns the unboxed payload
            // in x1 (low word) and the type tag in x0.
            emitter.instruction(&format!("ldr x0, [x19, #{}]", slot_offset(*idx))); // x0 = boxed Mixed pointer
            emitter.instruction("bl __rt_mixed_unbox");                         // x1 = unboxed object pointer (low word)
            emitter.instruction("mov x0, x1");                                  // x0 = inner generator pointer
        }
    }
    emitter.instruction(&format!("str x0, [x19, #{}]", gen_frame::OFF_DELEGATED_ITER)); // store the inner generator handle in the frame
    emitter.instruction("bl __rt_gen_rewind");                                  // run inner up to its first yield (x0 already = inner)

    // -- delegation loop: entered both initially and on every resume --
    emitter.label(&loop_lbl);
    emitter.instruction(&format!("ldr x0, [x19, #{}]", gen_frame::OFF_DELEGATED_ITER)); // reload inner pointer for valid()
    emitter.instruction("bl __rt_gen_valid");                                   // x0 = 1 if inner has more values, 0 otherwise
    emitter.instruction(&format!("cbz x0, {}", end_lbl));                       // inner exhausted — leave the delegation loop

    // -- forward inner.current() into outer.last_value with refcount --
    // `__rt_gen_current`/`__rt_gen_key` already incref the returned cell
    // before handing it back, so the pointer we store here owns its own
    // refcount alongside the inner generator's frame slot. The inner's
    // next yield-replace decrefs the inner's reference but leaves ours
    // alive.
    emit_replace_mixed_slot(emitter, gen_frame::OFF_LAST_VALUE, |em| {
        em.instruction(&format!("ldr x0, [x19, #{}]", gen_frame::OFF_DELEGATED_ITER)); // x0 = inner gen ptr
        em.instruction("bl __rt_gen_current");                              // x0 = owned boxed Mixed pointer for the inner's current value
    });
    emit_replace_mixed_slot(emitter, gen_frame::OFF_LAST_KEY, |em| {
        em.instruction(&format!("ldr x0, [x19, #{}]", gen_frame::OFF_DELEGATED_ITER)); // x0 = inner gen ptr
        em.instruction("bl __rt_gen_key");                                  // x0 = owned boxed Mixed pointer for the inner's current key
    });

    // -- bump state_idx and yield back to the outer caller --
    emitter.instruction(&format!("mov w10, #{}", state_idx));                   // mark this yield-from's resume index
    emitter.instruction(&format!("str w10, [x19, #{}]", gen_frame::OFF_STATE_IDX)); // store updated state_idx
    emitter.instruction(&format!("b {}", ctx.end_label));                       // return to the outer caller via the resume epilogue
    emitter.label(&format!("{}_resume_{}", ctx.label, state_idx));          // resume label hit on each subsequent next()
    emit_clear_sent_value(emitter);
    emitter.instruction(&format!("ldr x0, [x19, #{}]", gen_frame::OFF_DELEGATED_ITER)); // x0 = inner gen ptr
    emitter.instruction("bl __rt_gen_next");                                    // advance inner one step
    emitter.instruction(&format!("b {}", loop_lbl));                            // loop back to re-check valid()

    emitter.label(&end_lbl);
    if owns_delegated_iter {
        emitter.instruction(&format!("ldr x0, [x19, #{}]", gen_frame::OFF_DELEGATED_ITER)); // load the owned inner generator before clearing delegation state
        emitter.instruction(&format!("str xzr, [x19, #{}]", gen_frame::OFF_DELEGATED_ITER)); // clear delegated_iter so future yields don't re-enter the loop
        emitter.instruction("bl __rt_decref_any");                              // release the inner generator produced by yield-from's direct call
    } else {
        emitter.instruction(&format!("str xzr, [x19, #{}]", gen_frame::OFF_DELEGATED_ITER)); // clear borrowed delegated_iter without releasing the local owner
    }
    // Fall through to the caller's continuation of the outer body.
}

fn emit_yield_from_generator_x86_64(
    emitter: &mut Emitter,
    source: &YieldFromSource,
    state_idx: u32,
    ctx: &mut ResumeCtx,
) {
    let loop_lbl = ctx.fresh_label("yield_from_loop");
    let end_lbl = ctx.fresh_label("yield_from_end");
    let owns_delegated_iter = matches!(source, YieldFromSource::Call { .. });

    match source {
        YieldFromSource::Call { fn_name, args } => {
            emit_int_function_call(emitter, fn_name, args);                     // rax = inner generator pointer
        }
        YieldFromSource::IntSlot(idx) => {
            emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", slot_offset(*idx))); // rax = raw Generator pointer
        }
        YieldFromSource::MixedSlot(idx) => {
            emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", slot_offset(*idx))); // rax = boxed Mixed pointer
            emitter.instruction("call __rt_mixed_unbox");                       // rdi = unboxed object pointer
            emitter.instruction("mov rax, rdi");                                // rax = inner generator pointer
        }
    }
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], rax", gen_frame::OFF_DELEGATED_ITER)); // store the inner generator handle in the frame
    emitter.instruction("mov rdi, rax");                                        // pass inner generator to rewind()
    emitter.instruction("call __rt_gen_rewind");                                // run inner up to its first yield

    emitter.label(&loop_lbl);
    emitter.instruction(&format!("mov rdi, QWORD PTR [r12 + {}]", gen_frame::OFF_DELEGATED_ITER)); // reload inner pointer for valid()
    emitter.instruction("call __rt_gen_valid");                                 // rax = 1 if inner has more values, 0 otherwise
    emitter.instruction("test rax, rax");                                       // check whether the inner generator is exhausted
    emitter.instruction(&format!("jz {}", end_lbl));                            // inner exhausted -> leave the delegation loop

    emit_replace_mixed_slot(emitter, gen_frame::OFF_LAST_VALUE, |em| {
        em.instruction(&format!("mov rdi, QWORD PTR [r12 + {}]", gen_frame::OFF_DELEGATED_ITER)); // rdi = inner gen ptr
        em.instruction("call __rt_gen_current");                                // rax = owned boxed Mixed pointer for the inner's current value
    });
    emit_replace_mixed_slot(emitter, gen_frame::OFF_LAST_KEY, |em| {
        em.instruction(&format!("mov rdi, QWORD PTR [r12 + {}]", gen_frame::OFF_DELEGATED_ITER)); // rdi = inner gen ptr
        em.instruction("call __rt_gen_key");                                    // rax = owned boxed Mixed pointer for the inner's current key
    });

    emitter.instruction(&format!("mov r10d, {}", state_idx));                   // mark this yield-from's resume index
    emitter.instruction(&format!("mov DWORD PTR [r12 + {}], r10d", gen_frame::OFF_STATE_IDX)); // store updated state_idx
    emitter.instruction(&format!("jmp {}", ctx.end_label));                     // return to the outer caller via the resume epilogue
    emitter.label(&format!("{}_resume_{}", ctx.label, state_idx));              // resume label hit on each subsequent next()
    emit_clear_sent_value(emitter);
    emitter.instruction(&format!("mov rdi, QWORD PTR [r12 + {}]", gen_frame::OFF_DELEGATED_ITER)); // rdi = inner gen ptr
    emitter.instruction("call __rt_gen_next");                                  // advance inner one step
    emitter.instruction(&format!("jmp {}", loop_lbl));                          // loop back to re-check valid()

    emitter.label(&end_lbl);
    if owns_delegated_iter {
        emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", gen_frame::OFF_DELEGATED_ITER)); // load the owned inner generator before clearing delegation state
        emitter.instruction(&format!("mov QWORD PTR [r12 + {}], 0", gen_frame::OFF_DELEGATED_ITER)); // clear delegated_iter so future yields do not re-enter
        emitter.instruction("call __rt_decref_any");                            // release the inner generator produced by yield-from's direct call
    } else {
        emitter.instruction(&format!("mov QWORD PTR [r12 + {}], 0", gen_frame::OFF_DELEGATED_ITER)); // clear borrowed delegated_iter without releasing the local owner
    }
}

/// After the resume label of a `YieldAssign` whose LHS is an Int slot,
/// unbox the int payload that `Generator::send($v)` parked in the
/// frame's `sent_value` slot and store it into the local.
pub(super) fn emit_yield_assign_unbox_int(
    emitter: &mut Emitter,
    local_idx: usize,
    ctx: &mut ResumeCtx,
) {
    if emitter.target.arch == Arch::X86_64 {
        emit_yield_assign_unbox_int_x86_64(emitter, local_idx, ctx);
        return;
    }

    let null_lbl = ctx.fresh_label("send_null");
    let done_lbl = ctx.fresh_label("send_done");
    emitter.instruction(&format!("ldr x20, [x19, #{}]", gen_frame::OFF_SENT_VALUE)); // load the boxed sent_value pointer
    emitter.instruction("mov x0, x20");                                         // pass sent_value to the mixed unbox helper
    emitter.instruction(&format!("cbz x0, {}", null_lbl));                      // jump to null path when no send was performed
    emitter.instruction("bl __rt_mixed_unbox");                                 // x1 = unboxed low payload
    emitter.instruction("mov x9, x1");                                          // save the unboxed int across the next branch
    emitter.instruction(&format!("b {}", done_lbl));                            // skip the null default path
    emitter.label(&null_lbl);
    emitter.instruction("mov x9, xzr");                                         // no sent_value → assignment receives 0
    emitter.label(&done_lbl);
    emitter.instruction(&format!("str x9, [x19, #{}]", slot_offset(local_idx))); // store the int into the assignment LHS local
    emitter.instruction(&format!("str xzr, [x19, #{}]", gen_frame::OFF_SENT_VALUE)); // clear sent_value for the next round
    emitter.instruction("mov x0, x20");                                         // reload the consumed sent_value box for release
    emitter.instruction("bl __rt_decref_mixed");                                // release the consumed sent_value box (NULL is safe)
}

fn emit_yield_assign_unbox_int_x86_64(
    emitter: &mut Emitter,
    local_idx: usize,
    ctx: &mut ResumeCtx,
) {
    let null_lbl = ctx.fresh_label("send_null");
    let done_lbl = ctx.fresh_label("send_done");
    emitter.instruction(&format!("mov r13, QWORD PTR [r12 + {}]", gen_frame::OFF_SENT_VALUE)); // r13 = boxed sent_value pointer
    emitter.instruction("mov rax, r13");                                        // pass sent_value to the mixed unbox helper
    emitter.instruction("test rax, rax");                                       // check whether send() provided a value
    emitter.instruction(&format!("jz {}", null_lbl));                           // jump to null path when no send was performed
    emitter.instruction("call __rt_mixed_unbox");                               // rdi = unboxed low payload
    emitter.instruction("mov r10, rdi");                                        // save the unboxed int across the next branch
    emitter.instruction(&format!("jmp {}", done_lbl));                          // skip the null default path
    emitter.label(&null_lbl);
    emitter.instruction("xor r10, r10");                                        // no sent_value -> assignment receives 0
    emitter.label(&done_lbl);
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], r10", slot_offset(local_idx))); // store the int into the assignment LHS local
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], 0", gen_frame::OFF_SENT_VALUE)); // clear sent_value for the next round
    emitter.instruction("mov rax, r13");                                        // reload the consumed sent_value box for release
    emitter.instruction("call __rt_decref_mixed");                              // release the consumed sent_value box (NULL is safe)
}

/// After the resume label of a `YieldAssign` whose LHS is a Mixed slot,
/// transfer the sent Mixed pointer into the slot. `Generator::send($v)`
/// stored the boxed pointer in `sent_value`; we transfer ownership of
/// that single refcount into the slot via a refcount-replace pattern
/// (decref the slot's previous occupant). When `next()` was used
/// instead of `send()`, the slot stays at whatever it previously held.
pub(super) fn emit_yield_assign_store_mixed(
    emitter: &mut Emitter,
    local_idx: usize,
    ctx: &mut ResumeCtx,
) {
    if emitter.target.arch == Arch::X86_64 {
        emit_yield_assign_store_mixed_x86_64(emitter, local_idx, ctx);
        return;
    }

    let off = slot_offset(local_idx);
    let skip = ctx.fresh_label("send_mixed_skip");
    let done = ctx.fresh_label("send_mixed_done");
    emitter.instruction(&format!("ldr x9, [x19, #{}]", gen_frame::OFF_SENT_VALUE)); // x9 = boxed sent_value pointer
    emitter.instruction(&format!("cbz x9, {}", skip));                          // no send_value → keep slot unchanged
    emitter.instruction("mov x20, x9");                                         // park the sent pointer across the slot decref call
    emitter.instruction(&format!("str xzr, [x19, #{}]", gen_frame::OFF_SENT_VALUE)); // clear sent_value (slot now owns the refcount)
    emitter.instruction(&format!("ldr x0, [x19, #{}]", off));                   // x0 = previous slot occupant (or NULL)
    emitter.instruction(&format!("str x20, [x19, #{}]", off));                  // overwrite slot with the sent pointer
    emitter.instruction("bl __rt_decref_mixed");                                // decref the previous occupant (NULL is safe)
    emitter.instruction(&format!("b {}", done));                                // skip the no-send cleanup path
    emitter.label(&skip);
    emitter.instruction(&format!("str xzr, [x19, #{}]", gen_frame::OFF_SENT_VALUE)); // clear sent_value defensively
    emitter.label(&done);
}

fn emit_yield_assign_store_mixed_x86_64(
    emitter: &mut Emitter,
    local_idx: usize,
    ctx: &mut ResumeCtx,
) {
    let off = slot_offset(local_idx);
    let skip = ctx.fresh_label("send_mixed_skip");
    let done = ctx.fresh_label("send_mixed_done");
    emitter.instruction(&format!("mov r13, QWORD PTR [r12 + {}]", gen_frame::OFF_SENT_VALUE)); // r13 = boxed sent_value pointer
    emitter.instruction("test r13, r13");                                       // check whether send() provided a value
    emitter.instruction(&format!("jz {}", skip));                               // no send_value -> keep slot unchanged
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], 0", gen_frame::OFF_SENT_VALUE)); // clear sent_value (slot now owns the refcount)
    emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", off));        // rax = previous slot occupant (or NULL)
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], r13", off));        // overwrite slot with the sent pointer
    emitter.instruction("call __rt_decref_mixed");                              // decref the previous occupant (NULL is safe)
    emitter.instruction(&format!("jmp {}", done));                              // skip the no-send cleanup path
    emitter.label(&skip);
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], 0", gen_frame::OFF_SENT_VALUE)); // clear sent_value defensively
    emitter.label(&done);
}
