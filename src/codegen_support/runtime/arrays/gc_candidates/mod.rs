//! Purpose:
//! Restricts application-worker cycle collection to graphs that lost an owner.
//!
//! Called from:
//! - Decrement helpers, the cycle collector, and its shared graph visitor.
//!
//! Key details:
//! - Pending roots and scan vectors live outside the PHP heap and never own PHP values.
//! - Exact live-base checks reject stale queue entries before inspecting headers.
//! - Snapshot membership is cleared on every completed collection, including throws.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

mod buffers;
mod roots;
pub(super) mod proofs;
pub(crate) mod scans;

/// Selects the reviewed instruction for the target's runtime pointer ABI.
pub(crate) fn op(e: &mut Emitter, arm: &str, x86: &str, intent: &str) {
    let instruction = if e.target.arch == Arch::AArch64 { arm } else { x86 };
    let prefix = e.target.line_comment_prefix();
    e.instruction(&format!("{instruction:76} {prefix} {intent}"));              // preserve each call site's instruction intent in the emitted assembly
}

/// Loads a state word without borrowing another helper's live address register.
pub(crate) fn load(e: &mut Emitter, arm: &str, x86: &str, symbol: &str) {
    let reg = if e.target.arch == Arch::AArch64 { arm } else { x86 };
    abi::emit_symbol_address(e, reg, symbol);
    op(e, &format!("ldr {arm}, [{arm}]"), &format!("mov {x86}, QWORD PTR [{x86}]"), "read the addressed collector state without retaining its address");
}

/// Saves the runtime result in a scratch slot measured from the current stack pointer.
pub(crate) fn save(e: &mut Emitter, offset: usize) {
    op(e, &format!("str x0, [sp, #{offset}]"), &format!("mov QWORD PTR [rsp + {offset}], rax"), "preserve the result across a nested runtime call");
}

/// Restores the runtime result from a scratch slot measured from the stack pointer.
pub(crate) fn restore(e: &mut Emitter, offset: usize) {
    op(e, &format!("ldr x0, [sp, #{offset}]"), &format!("mov rax, QWORD PTR [rsp + {offset}]"), "recover the saved result after volatile registers were clobbered");
}

/// Emits buffers, validity checks, snapshot preparation and retirement for both ABIs.
pub fn emit(e: &mut Emitter) {
    buffers::emit(e);
    roots::emit(e);
    proofs::emit(e);
    emit_prepare(e);
    emit_finish(e);
}

/// Compacts surviving identities, drains suspect roots, and discovers their forward closure.
fn emit_prepare(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: gc_prepare_candidates ---");
    e.label_global("__rt_gc_prepare_candidates");
    abi::emit_frame_prologue(e, 64);
    load(e, "x9", "r10", "_gc_candidates_enabled");
    op(e, "cbz x9, __rt_gc_prepare_return", "test r10, r10", "keep legacy collection outside candidate preparation");
    if e.target.arch == Arch::X86_64 { op(e, "", "jz __rt_gc_prepare_return", "return immediately when candidate mode is disabled"); }
    abi::emit_call_label(e, "__rt_gc_proof_epoch_begin");
    // -- borrow actual owners before expanding suspected garbage --
    abi::emit_store_imm_to_symbol(e, "_gc_candidates_gathering", 0, 2);
    load(e, "x9", "r10", "_gc_candidate_root_scan_fn");
    op(e, "cbz x9, __rt_gc_prepare_root_scan_done", "test r10, r10", "test whether this program published a root visitor");
    if e.target.arch == Arch::X86_64 { op(e, "", "jz __rt_gc_prepare_root_scan_done", "skip root discovery when no program visitor exists"); }
    op(e, "blr x9", "call r10", "borrow the program's actual initialized owning roots");
    e.label("__rt_gc_prepare_root_scan_done");
    // -- initialize proofs once before any large retained lease can seed a closure --
    load(e, "x9", "r10", "_gc_proofs_initialized");
    op(e, "cbnz x9, __rt_gc_proofs_ready", "test r10, r10", "reuse an existing rooted spanning forest");
    if e.target.arch == Arch::X86_64 { op(e, "", "jnz __rt_gc_proofs_ready", "skip bootstrap cache construction after its first completion"); }
    abi::emit_call_label(e, "__rt_gc_proofs_refresh");
    e.label("__rt_gc_proofs_ready");
    abi::emit_store_imm_to_symbol(e, "_gc_candidates_gathering", 0, 1);
    load(e, "x0", "rax", "_gc_candidates_nodes_count");
    save(e, 0);
    abi::emit_load_int_immediate(e, abi::int_result_reg(e), 0);
    save(e, 8);
    save(e, 16);
    // -- keep pinned identities across destructor-driven recounts --
    e.label("__rt_gc_prepare_compact");
    restore(e, 8);
    op(e, "ldr x9, [sp]", "mov r10, QWORD PTR [rsp]", "recover the original vector length while compacting it");
    op(e, "cmp x0, x9", "cmp rax, r10", "test whether all old snapshot entries were inspected");
    op(e, "b.hs __rt_gc_prepare_roots", "jae __rt_gc_prepare_roots", "begin draining pending roots after compaction");
    load(e, "x9", "r10", "_gc_candidates_nodes");
    op(e, "ldr x0, [x9, x0, lsl #3]", "mov rax, QWORD PTR [r10 + rax * 8]", "resolve the next old snapshot header");
    op(e, "add x0, x0, #16", "add rax, 16", "validate the old entry using its payload base");
    abi::emit_call_label(e, "__rt_gc_candidate_live");
    abi::emit_branch_if_int_result_zero(e, "__rt_gc_prepare_compact_next");
    op(e, "ldr x9, [x0, #-8]", "mov r10, QWORD PTR [rax - 8]", "inspect this identity's current snapshot membership");
    op(e, "tbz x9, #17, __rt_gc_prepare_compact_next", "test r10, 0x20000", "exclude identities whose old snapshot membership ended");
    if e.target.arch == Arch::X86_64 { op(e, "", "jz __rt_gc_prepare_compact_next", "skip retired or independently reused identities"); }
    load(e, "x9", "r10", "_gc_candidates_nodes");
    op(e, "ldr x10, [sp, #16]", "mov r11, QWORD PTR [rsp + 16]", "recover the compacted vector's destination index");
    op(e, "sub x0, x0, #16", "sub rax, 16", "convert the validated payload back to its stored header");
    op(e, "str x0, [x9, x10, lsl #3]", "mov QWORD PTR [r10 + r11 * 8], rax", "retain this surviving snapshot identity in the compacted prefix");
    op(e, "add x10, x10, #1", "add r11, 1", "advance the surviving snapshot length");
    op(e, "str x10, [sp, #16]", "mov QWORD PTR [rsp + 16], r11", "preserve the compacted length across later validity calls");
    e.label("__rt_gc_prepare_compact_next");
    increment_slot(e, 8);
    abi::emit_jump(e, "__rt_gc_prepare_compact");
    // -- drain pending roots and retire their independent queue markers --
    e.label("__rt_gc_prepare_roots");
    restore(e, 16);
    abi::emit_store_reg_to_symbol(e, abi::int_result_reg(e), "_gc_candidates_nodes_count", 0);
    load(e, "x0", "rax", "_gc_candidates_roots_count");
    save(e, 0);
    abi::emit_store_zero_to_symbol(e, "_gc_candidates_roots_count", 0);
    abi::emit_load_int_immediate(e, abi::int_result_reg(e), 0);
    save(e, 8);
    e.label("__rt_gc_prepare_root_loop");
    restore(e, 8);
    op(e, "ldr x9, [sp]", "mov r10, QWORD PTR [rsp]", "recover the pending-root count captured before draining");
    op(e, "cmp x0, x9", "cmp rax, r10", "test whether the pending root queue is exhausted");
    op(e, "b.hs __rt_gc_prepare_trace_init", "jae __rt_gc_prepare_trace_init", "expand the gathered frontier after every root was processed");
    load(e, "x9", "r10", "_gc_candidates_roots");
    op(e, "ldr x0, [x9, x0, lsl #3]", "mov rax, QWORD PTR [r10 + rax * 8]", "borrow the next pending identity from the non-owning queue");
    abi::emit_call_label(e, "__rt_gc_candidate_live");
    abi::emit_branch_if_int_result_zero(e, "__rt_gc_prepare_root_next");
    op(e, "ldr x9, [x0, #-8]", "mov r10d, DWORD PTR [rax - 8]", "inspect this allocation's current queue-membership marker");
    op(e, "tbz x9, #31, __rt_gc_prepare_root_next", "bt r10d, 31", "ignore a stale queue slot not independently queued after reuse");
    if e.target.arch == Arch::X86_64 { op(e, "", "jnc __rt_gc_prepare_root_next", "skip identities that no longer belong to this pending set"); }
    op(e, "bic x9, x9, #0x80000000", "and DWORD PTR [rax - 8], 0x7fffffff", "retire pending membership before seeding the forward closure");
    if e.target.arch == Arch::AArch64 { op(e, "str x9, [x0, #-8]", "", "publish the cleared pending marker before nested work"); }
    abi::emit_call_label(e, "__rt_gc_candidate_seed");
    e.label("__rt_gc_prepare_root_next");
    increment_slot(e, 8);
    abi::emit_jump(e, "__rt_gc_prepare_root_loop");
    // -- expand the suspect closure iteratively --
    e.label("__rt_gc_prepare_trace_init");
    abi::emit_load_int_immediate(e, abi::int_result_reg(e), 0);
    save(e, 8);
    e.label("__rt_gc_prepare_trace");
    restore(e, 8);
    load(e, "x9", "r10", "_gc_candidates_nodes_count");
    op(e, "cmp x0, x9", "cmp rax, r10", "test the frontier index against its growing length");
    op(e, "b.hs __rt_gc_prepare_done", "jae __rt_gc_prepare_done", "finish discovery after the complete closure was expanded");
    load(e, "x9", "r10", "_gc_candidates_nodes");
    op(e, "ldr x0, [x9, x0, lsl #3]", "mov rax, QWORD PTR [r10 + rax * 8]", "resolve the next discovered graph header");
    op(e, "add x0, x0, #16", "add rax, 16", "recover its payload for the shared child-layout visitor");
    save(e, 24);
    abi::emit_call_label(e, "__rt_gc_node_scan_cost");
    load(e, "x9", "r10", "_gc_candidates_visited");
    op(e, "add x0, x0, x9", "add rax, r10", "include scalar, empty and reference-bearing slots in real candidate work");
    abi::emit_store_reg_to_symbol(e, abi::int_result_reg(e), "_gc_candidates_visited", 0);
    restore(e, 24);
    abi::emit_call_label(e, "__rt_gc_visit_children");
    increment_slot(e, 8);
    abi::emit_jump(e, "__rt_gc_prepare_trace");
    // -- record work and clear temporary root metadata --
    e.label("__rt_gc_prepare_done");
    abi::emit_store_zero_to_symbol(e, "_gc_candidates_gathering", 0);
    abi::emit_call_label(e, "__rt_gc_clear_candidate_anchors");
    e.label("__rt_gc_prepare_return");
    abi::emit_frame_restore(e, 64);
    abi::emit_return(e);
}

/// Increments a private loop counter without assuming a helper preserved volatile registers.
fn increment_slot(e: &mut Emitter, offset: usize) {
    restore(e, offset);
    op(e, "add x0, x0, #1", "add rax, 1", "advance the private cursor without relying on volatile state");
    save(e, offset);
}

/// Retires only surviving snapshot membership; reclaimed or reused payloads are never inspected.
fn emit_finish(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: gc_finish_candidates ---");
    e.label_global("__rt_gc_finish_candidates");
    abi::emit_frame_prologue(e, 32);
    abi::emit_load_int_immediate(e, abi::int_result_reg(e), 0);
    save(e, 0);
    // -- retire surviving snapshot identities before later collections --
    e.label("__rt_gc_finish_candidate_loop");
    restore(e, 0);
    load(e, "x9", "r10", "_gc_candidates_nodes_count");
    op(e, "cmp x0, x9", "cmp rax, r10", "test whether every final snapshot identity was retired");
    op(e, "b.hs __rt_gc_finish_candidates_done", "jae __rt_gc_finish_candidates_done", "finish once no snapshot membership remains to clear");
    load(e, "x9", "r10", "_gc_candidates_nodes");
    op(e, "ldr x0, [x9, x0, lsl #3]", "mov rax, QWORD PTR [r10 + rax * 8]", "borrow the next retiring snapshot identity");
    op(e, "add x0, x0, #16", "add rax, 16", "validate its current allocation before inspecting metadata");
    abi::emit_call_label(e, "__rt_gc_candidate_live");
    abi::emit_branch_if_int_result_zero(e, "__rt_gc_finish_candidate_next");
    // -- surviving unproved state earns cache maintenance without postponing its collection --
    op(e, "ldr x9, [x0, #-8]", "test DWORD PTR [rax - 8], 0x20000", "charge cache maintenance only for surviving members of this exact snapshot");
    op(e, "tbz x9, #17, __rt_gc_finish_candidate_next", "jz __rt_gc_finish_candidate_next", "ignore an independently reused allocation at an old vector address");
    save(e, 8);
    abi::emit_call_label(e, "__rt_gc_node_scan_cost");
    load(e, "x9", "r10", "_gc_proofs_debt");
    op(e, "adds x9, x9, x0", "add r10, rax", "charge the actual surviving node and its inspected slots toward cache maintenance");
    op(e, "b.cc __rt_gc_finish_candidate_debt_ready", "jnc __rt_gc_finish_candidate_debt_ready", "preserve the accumulated rebuild charge without unsigned overflow");
    op(e, "mov x9, #-1", "mov r10, -1", "saturate an exhausted counter instead of losing earned rebuild work");
    e.label("__rt_gc_finish_candidate_debt_ready");
    abi::emit_store_reg_to_symbol(e, if e.target.arch == Arch::AArch64 { "x9" } else { "r10" }, "_gc_proofs_debt", 0);
    restore(e, 8);
    op(e, "ldr x9, [x0, #-8]", "and DWORD PTR [rax - 8], 0xfffdffff", "retire snapshot membership while preserving unrelated flags");
    if e.target.arch == Arch::AArch64 {
        op(e, "bic x9, x9, #0x20000", "", "clear only the active-snapshot marker");
        op(e, "str x9, [x0, #-8]", "", "publish retirement before any later collection");
    }
    e.label("__rt_gc_finish_candidate_next");
    increment_slot(e, 0);
    abi::emit_jump(e, "__rt_gc_finish_candidate_loop");
    e.label("__rt_gc_finish_candidates_done");
    abi::emit_store_zero_to_symbol(e, "_gc_candidates_nodes_count", 0);
    abi::emit_frame_restore(e, 32);
    abi::emit_return(e);
}

#[cfg(test)]
mod tests;
