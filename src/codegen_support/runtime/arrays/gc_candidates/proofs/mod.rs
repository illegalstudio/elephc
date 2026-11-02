//! Purpose:
//! Proves retained graph liveness through current owning edges and freshly enumerated roots.
//!
//! Called from:
//! - Candidate preparation and explicit application bootstrap/request boundaries.
//!
//! Key details:
//! - Records are non-owning (node, parent, logical slot) spanning-tree edges in libc storage.
//! - Each prepare revalidates shared paths once; mutation and allocator reuse need no write barrier.
//! - Unproved nodes retain ordinary immediate candidate collection and destructor timing.

use super::{load, op, restore, save};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

mod build;
mod edges;
mod callable_edges;
mod memo;
mod cost;

/// Emits cache construction, binary lookup and current-path validation on every target.
pub(super) fn emit(e: &mut Emitter) {
    edges::emit(e);
    callable_edges::emit(e);
    build::emit(e);
    memo::emit(e);
    cost::emit(e);
    emit_lookup(e);
    emit_valid(e);
}

/// Finds a borrowed proof record by exact payload base in the sorted cache.
fn emit_lookup(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: gc_proof_lookup ---");
    e.label_global("__rt_gc_proof_lookup");
    // -- binary search immutable records without allocating or adding PHP references --
    load(e, "x9", "r8", "_gc_proofs");
    load(e, "x10", "r9", "_gc_proofs_count");
    op(e, "mov x11, #0", "xor r10d, r10d", "start at the first proof record");
    e.label("__rt_gc_proof_lookup_loop");
    op(e, "cmp x11, x10", "cmp r10, r9", "test whether the remaining search interval is empty");
    op(e, "b.hs __rt_gc_proof_lookup_missing", "jae __rt_gc_proof_lookup_missing", "return unknown when this payload has no cached path");
    op(e, "add x12, x11, x10", "lea rcx, [r10 + r9]", "select a midpoint inside the remaining sorted interval");
    op(e, "lsr x12, x12, #1", "shr rcx, 1", "halve the interval without changing its bounds");
    op(e, "add x13, x12, x12, lsl #1", "lea r11, [rcx + rcx * 2]", "scale the record index by its three pointer-sized fields");
    op(e, "add x13, x9, x13, lsl #3", "lea r11, [r8 + r11 * 8]", "address the candidate cache record");
    op(e, "ldr x14, [x13]", "mov rdx, QWORD PTR [r11]", "inspect the record's exact payload identity");
    op(e, "cmp x0, x14", "cmp rax, rdx", "compare unsigned payload addresses in the cache's ordering");
    abi::emit_branch_if_equal_wide(e, "__rt_gc_proof_lookup_found");
    op(e, "b.lo __rt_gc_proof_lookup_lower", "jb __rt_gc_proof_lookup_lower", "discard records above the requested payload");
    op(e, "add x11, x12, #1", "lea r10, [rcx + 1]", "discard records below the requested payload");
    abi::emit_jump(e, "__rt_gc_proof_lookup_loop");
    e.label("__rt_gc_proof_lookup_lower");
    op(e, "mov x10, x12", "mov r9, rcx", "keep the lower half of the remaining interval");
    abi::emit_jump(e, "__rt_gc_proof_lookup_loop");
    e.label("__rt_gc_proof_lookup_found");
    op(e, "mov x1, x12", "mov rdi, rcx", "return the matching record index for its parallel epoch word");
    op(e, "mov x0, x13", "mov rax, r11", "return a borrowed record without changing application ownership");
    abi::emit_return(e);
    e.label("__rt_gc_proof_lookup_missing");
    abi::emit_load_int_immediate(e, abi::int_result_reg(e), 0);
    abi::emit_return(e);
}

/// Returns the original payload only if every current owning edge reaches a current root.
fn emit_valid(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: gc_proof_valid ---");
    e.label_global("__rt_gc_proof_valid");
    abi::emit_frame_prologue(e, 64);
    save(e, 0);
    save(e, 8);
    load(e, "x0", "rax", "_gc_proofs_count");
    op(e, "add x0, x0, #1", "add rax, 1", "bound path checks even if a cache invariant is violated");
    save(e, 16);
    // -- every proof terminates at an anchor re-established from real owning roots --
    e.label("__rt_gc_proof_valid_loop");
    restore(e, 16);
    op(e, "subs x0, x0, #1", "sub rax, 1", "consume one finite proof-path step");
    save(e, 16);
    abi::emit_branch_if_int_result_zero(e, "__rt_gc_proof_valid_missing");
    restore(e, 8);
    abi::emit_call_label(e, "__rt_gc_candidate_live");
    abi::emit_branch_if_int_result_zero(e, "__rt_gc_proof_valid_missing");
    load(e, "x9", "r10", "_gc_proofs_visited");
    op(e, "add x9, x9, #1", "add r10, 1", "count path validation as real collector work");
    abi::emit_store_reg_to_symbol(e, reg(e, "x9", "r10"), "_gc_proofs_visited", 0);
    op(e, "ldr x9, [x0, #-8]", "test DWORD PTR [rax - 8], 0x100000", "recognize a freshly proved global/static anchor");
    op(e, "tbnz x9, #20, __rt_gc_proof_valid_found", "jnz __rt_gc_proof_valid_found", "accept this path only after it reaches an actual current root");
    abi::emit_call_label(e, "__rt_gc_proof_lookup");
    abi::emit_branch_if_int_result_zero(e, "__rt_gc_proof_valid_missing");
    load(e, "x9", "r10", "_gc_proof_memos");
    op(e, "ldr x10, [x9, x1, lsl #3]", "mov r11, QWORD PTR [r10 + rdi * 8]", "inspect the outcome already established for this record in the current pass");
    load(e, "x9", "r10", "_gc_proof_epoch");
    op(e, "cmp x10, x9", "cmp r11, r10", "reuse an unproved prefix without treating it as dead");
    abi::emit_branch_if_equal_wide(e, "__rt_gc_proof_valid_missing");
    op(e, "orr x9, x9, #1", "or r10, 1", "select the proved variant of this pass generation");
    op(e, "cmp x10, x9", "cmp r11, r10", "reuse a prefix whose actual current path already reached a root");
    abi::emit_branch_if_equal_wide(e, "__rt_gc_proof_valid_found");
    op(e, "ldr x9, [x0, #16]", "mov r10, QWORD PTR [rax + 16]", "recover the logical slot rather than a stale entry-buffer address");
    op(e, "str x9, [sp, #24]", "mov QWORD PTR [rsp + 24], r10", "preserve the logical slot across parent-base validation");
    op(e, "ldr x0, [x0, #8]", "mov rax, QWORD PTR [rax + 8]", "follow the cache's non-owning spanning-tree parent");
    save(e, 32);
    abi::emit_call_label(e, "__rt_gc_candidate_live");
    abi::emit_branch_if_int_result_zero(e, "__rt_gc_proof_valid_missing");
    op(e, "ldr x1, [sp, #24]", "mov rdi, QWORD PTR [rsp + 24]", "provide the saved logical slot to the current-layout predicate");
    op(e, "ldr x2, [sp, #8]", "mov rsi, QWORD PTR [rsp + 8]", "require the slot to own this exact child identity");
    abi::emit_call_label(e, "__rt_gc_proof_edge");
    abi::emit_branch_if_int_result_zero(e, "__rt_gc_proof_valid_missing");
    restore(e, 32);
    save(e, 8);
    abi::emit_jump(e, "__rt_gc_proof_valid_loop");
    memo::finish(e);
    abi::emit_frame_restore(e, 64);
    abi::emit_return(e);
}

/// Selects a register whose value is already expressed in the target's runtime ABI.
fn reg<'a>(e: &Emitter, arm: &'a str, x86: &'a str) -> &'a str {
    if e.target.arch == Arch::AArch64 { arm } else { x86 }
}

/// Publishes a logical child slot only during cache construction, preserving visitor operands.
pub(crate) fn slot(e: &mut Emitter, name: &str, registers: Option<(&str, &str, i64)>, constant: i64) {
    let done = format!("__rt_gc_proof_slot_{name}_done");
    // -- ordinary marking and candidate traversal do not maintain proof metadata --
    load(e, "x17", "r11", "_gc_candidates_gathering");
    op(e, "cmp x17, #3", "cmp r11, 3", "publish edge metadata only while building the rooted cache");
    op(e, &format!("b.ne {done}"), &format!("jne {done}"), "preserve ordinary traversal without recording a cached edge");
    if e.target.arch == Arch::AArch64 {
        op(e, "stp x9, x10, [sp, #-16]!", "", "preserve live loop operands around symbol-address materialization");
    }
    if let Some((arm, x86, adjustment)) = registers {
        let source = if adjustment == 0 { reg(e, arm, x86) } else {
            op(e, &format!("sub x17, {arm}, #{}", -adjustment), &format!("lea r11, [{x86} {adjustment:+}]"), "locate the logical slot relative to the current declared layout");
            reg(e, "x17", "r11")
        };
        abi::emit_store_reg_to_symbol(e, source, "_gc_proof_slot", 0);
    } else {
        abi::emit_store_imm_to_symbol(e, "_gc_proof_slot", 0, constant);
    }
    if e.target.arch == Arch::AArch64 {
        op(e, "ldp x9, x10, [sp], #16", "", "restore all visitor loop operands before following the child");
    }
    e.label(&done);
}
