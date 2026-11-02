//! Purpose:
//! Emits non-owning suspect queues with exact allocator identity validation.
//!
//! Called from:
//! - Candidate collection and nonfinal container releases in application workers.
//!
//! Key details:
//! - Header bit 31 deduplicates pending roots; bit 17 deduplicates snapshot members.
//! - A recycled payload may use an old queue slot only if it is independently queued again.
//! - Buffer growth uses libc storage and preserves runtime caller registers.

use super::{load, op, restore, save};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::sentinels::{REFERENCE_CELL_HEAP_KIND, CALLABLE_HEAP_KIND};

/// Emits validity, append, register-preserving release hooks, and raw buffer growth.
pub(super) fn emit(e: &mut Emitter) {
    emit_live(e);
    emit_append(e, "queue", "roots", 31, false);
    emit_append(e, "seed", "nodes", 17, true);
    emit_append(e, "anchor", "anchors", 20, true);
    emit_note(e);
    emit_realloc(e);
}

/// Returns the original payload or null without reading foreign, interior or retired headers.
fn emit_live(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: gc_candidate_live ---");
    e.label_global("__rt_gc_candidate_live");
    // -- validate exact allocation identity before reading headers --
    abi::emit_symbol_address(e, if e.target.arch == Arch::AArch64 { "x9" } else { "r10" }, "_heap_buf");
    op(e, "sub x10, x0, x9", "mov r11, rax", "derive the payload offset for exact identity validation");
    if e.target.arch == Arch::X86_64 { op(e, "", "sub r11, r10", "compute the heap-relative offset without altering the candidate"); }
    op(e, "cmp x10, #16", "cmp r11, 16", "exclude offsets below the first managed payload");
    op(e, "b.lo __rt_gc_candidate_invalid", "jb __rt_gc_candidate_invalid", "reject foreign pointers before accessing the bitmap");
    load(e, "x9", "r10", "_heap_off");
    op(e, "cmp x10, x9", "cmp r11, r10", "bound this offset by the active arena extent");
    op(e, "b.hs __rt_gc_candidate_invalid", "jae __rt_gc_candidate_invalid", "reject addresses beyond the allocator's current extent");
    op(e, "tst x10, #7", "test r11, 7", "require allocation-base alignment");
    op(e, "b.ne __rt_gc_candidate_invalid", "jnz __rt_gc_candidate_invalid", "reject unaligned interior addresses");
    op(e, "lsr x11, x10, #3", "mov rcx, r11", "preserve the offset for selecting its granule bit");
    op(e, "lsr x10, x10, #6", "shr r11, 6", "locate the bitmap byte covering this payload");
    op(e, "and x11, x11, #7", "shr rcx, 3", "select the granule bit within the bitmap byte");
    if e.target.arch == Arch::X86_64 { op(e, "", "and ecx, 7", "isolate the low three granule-index bits"); }
    abi::emit_symbol_address(e, if e.target.arch == Arch::AArch64 { "x9" } else { "r10" }, "_heap_live_bases");
    op(e, "ldrb w9, [x9, x10]", "movzx r10d, BYTE PTR [r10 + r11]", "inspect allocator-owned identity metadata");
    op(e, "lsr w9, w9, w11", "shr r10d, cl", "position the exact payload marker for testing");
    op(e, "tbz w9, #0, __rt_gc_candidate_invalid", "test r10b, 1", "require an allocator-published live base");
    if e.target.arch == Arch::X86_64 { op(e, "", "jz __rt_gc_candidate_invalid", "reject retired and forged interior pointers"); }
    op(e, "ldr w9, [x0, #-12]", "cmp DWORD PTR [rax - 12], 0", "inspect the validated allocation's reference count");
    op(e, "cbz w9, __rt_gc_candidate_invalid", "je __rt_gc_candidate_invalid", "exclude a node no longer owned by any reference");
    abi::emit_return(e);
    e.label("__rt_gc_candidate_invalid");
    abi::emit_load_int_immediate(e, abi::int_result_reg(e), 0);
    abi::emit_return(e);
}

/// Appends one graph identity, with independent deduplication for pending and active sets.
fn emit_append(e: &mut Emitter, name: &str, vector: &str, bit: u32, header: bool) {
    let prefix = format!("__rt_gc_candidate_{name}");
    let done = format!("{prefix}_done");
    let ready = format!("{prefix}_ready");
    e.blank();
    e.comment(&format!("--- runtime: gc_candidate_{name} ---"));
    e.label_global(&prefix);
    abi::emit_frame_prologue(e, 32);
    save(e, 0);
    abi::emit_call_label(e, "__rt_gc_candidate_live");
    abi::emit_branch_if_int_result_zero(e, &done);
    op(e, "ldr x9, [x0, #-8]", "mov r10, QWORD PTR [rax - 8]", "inspect kind and membership before appending a graph identity");
    if name == "seed" {
        op(e, &format!("tbnz x9, #20, {done}"), "test r10, 0x100000", "test whether a real worker root already proves liveness");
        if e.target.arch == Arch::X86_64 { op(e, "", &format!("jnz {done}"), "avoid expanding a graph whose owner is still rooted"); }
    }
    // -- accept only allocation shapes that can own graph edges --
    op(e, "and x10, x9, #0xff", "mov r11, r10", "isolate the physical allocation kind for graph filtering");
    if e.target.arch == Arch::X86_64 { op(e, "", "and r11d, 0xff", "remove transient collector metadata from the kind"); }
    op(e, &format!("cmp x10, #{CALLABLE_HEAP_KIND}"), &format!("cmp r11d, {CALLABLE_HEAP_KIND}"), "recognize independently managed callable captures");
    op(e, &format!("b.eq {prefix}_kind"), &format!("je {prefix}_kind"), "accept callable captures as graph identities");
    op(e, &format!("cmp x10, #{REFERENCE_CELL_HEAP_KIND}"), &format!("cmp r11d, {REFERENCE_CELL_HEAP_KIND}"), "recognize independently managed reference cells");
    op(e, &format!("b.eq {prefix}_kind"), &format!("je {prefix}_kind"), "accept reference cells as graph identities");
    op(e, "cmp x10, #2", "cmp r11d, 2", "exclude raw storage and strings from graph discovery");
    op(e, &format!("b.lo {done}"), &format!("jb {done}"), "return when the allocation cannot own graph edges");
    op(e, "cmp x10, #5", "cmp r11d, 5", "bound ordinary graph kinds by the Mixed wrapper kind");
    op(e, &format!("b.hi {done}"), &format!("ja {done}"), "exclude non-container allocation kinds");
    op(e, "cmp x10, #2", "cmp r11d, 2", "check whether array element typing needs additional filtering");
    op(e, &format!("b.ne {prefix}_kind"), &format!("jne {prefix}_kind"), "accept non-array graph containers directly");
    op(e, "ubfx x10, x9, #8, #7", "shr r10, 8", "recover the array's packed element descriptor");
    if e.target.arch == Arch::X86_64 { op(e, "", "and r10d, 0x7f", "remove array flags from the element descriptor"); }
    op(e, "cmp x10, #4", "cmp r10d, 4", "exclude arrays containing only scalar or string values");
    op(e, &format!("b.lo {done}"), &format!("jb {done}"), "return when no element can own a graph child");
    op(e, "cmp x10, #10", "cmp r10d, 10", "callable arrays own descriptor nodes rather than scalar data");
    op(e, &format!("b.eq {prefix}_kind"), &format!("je {prefix}_kind"), "include callable array cycles in the candidate snapshot");
    op(e, "cmp x10, #7", "cmp r10d, 7", "bound supported reference-bearing element descriptors");
    op(e, &format!("b.hi {done}"), &format!("ja {done}"), "exclude unknown element representations");
    // -- deduplicate membership and reserve a non-owning vector slot --
    e.label(&format!("{prefix}_kind"));
    if name == "seed" {
        op(e, "ldr x9, [x0, #-8]", "test DWORD PTR [rax - 8], 0x20000", "deduplicate the active snapshot before any cached-path work");
        op(e, &format!("tbnz x9, #17, {done}"), &format!("jnz {done}"), "an already selected member needs no repeated proof validation");
        save(e, 8);
        abi::emit_call_label(e, "__rt_gc_proof_valid");
        abi::emit_branch_if_int_result_nonzero(e, &done);
        restore(e, 8);
    }

    op(e, "ldr x9, [x0, #-8]", &format!("bts DWORD PTR [rax - 8], {bit}"), "claim this vector's independent membership marker");
    op(e, &format!("tbnz x9, #{bit}, {done}"), &format!("jc {done}"), "deduplicate an identity already present in the same vector");
    if e.target.arch == Arch::AArch64 {
        op(e, &format!("orr x9, x9, #0x{:x}", 1u32 << bit), "", "claim this identity before potentially growing the raw buffer");
        op(e, "str x9, [x0, #-8]", "", "publish the claimed membership before buffer growth");
    }
    // -- grow the raw vector only when its current capacity is exhausted --
    load(e, "x9", "r10", &format!("_gc_candidates_{vector}_count"));
    load(e, "x10", "r11", &format!("_gc_candidates_{vector}_capacity"));
    op(e, "cmp x9, x10", "cmp r10, r11", "check whether the current vector has an unused slot");
    op(e, &format!("b.lo {ready}"), &format!("jb {ready}"), "reuse the existing raw buffer when capacity suffices");
    op(e, "lsl x10, x10, #1", "shl r11, 1", "grow capacity geometrically to amortize append costs");
    op(e, "cmp x10, #256", "cmp r11, 256", "ensure the first allocation has a useful minimum capacity");
    op(e, &format!("b.hs {prefix}_capacity"), &format!("jae {prefix}_capacity"), "keep an already sufficient doubled capacity");
    op(e, "mov x10, #256", "mov r11, 256", "initialize capacity for an empty vector");
    e.label(&format!("{prefix}_capacity"));
    abi::emit_store_reg_to_symbol(e, if e.target.arch == Arch::AArch64 { "x10" } else { "r11" }, &format!("_gc_candidates_{vector}_capacity"), 0);
    op(e, "lsl x1, x10, #3", "lea rsi, [r11 * 8]", "convert the pointer-slot capacity into a libc allocation size");
    load(e, "x0", "rdi", &format!("_gc_candidates_{vector}"));
    abi::emit_call_label(e, "__rt_gc_candidate_realloc");
    abi::emit_branch_if_int_result_nonzero(e, &format!("{prefix}_allocated"));
    abi::emit_jump(e, "__rt_heap_allocation_failed");
    e.label(&format!("{prefix}_allocated"));
    abi::emit_store_reg_to_symbol(e, abi::int_result_reg(e), &format!("_gc_candidates_{vector}"), 0);
    e.label(&ready);
    restore(e, 0);
    if header { op(e, "sub x0, x0, #16", "sub rax, 16", "store headers for active vectors and payloads for pending roots"); }
    load(e, "x9", "r10", &format!("_gc_candidates_{vector}"));
    load(e, "x10", "r11", &format!("_gc_candidates_{vector}_count"));
    op(e, "str x0, [x9, x10, lsl #3]", "mov QWORD PTR [r10 + r11 * 8], rax", "append the identity without creating a PHP owner");
    op(e, "add x10, x10, #1", "add r11, 1", "advance the published vector length");
    abi::emit_store_reg_to_symbol(e, if e.target.arch == Arch::AArch64 { "x10" } else { "r11" }, &format!("_gc_candidates_{vector}_count"), 0);
    e.label(&done);
    restore(e, 0);
    abi::emit_frame_restore(e, 32);
    abi::emit_return(e);
}

/// Keeps the ordinary release ABI intact, including register arguments owned by its caller.
fn emit_note(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: gc_note_candidate ---");
    e.label_global("__rt_gc_note_candidate");
    load(e, "x9", "r10", "_gc_candidates_enabled");
    op(e, "cbz x9, __rt_gc_note_candidate_return", "test r10, r10", "test whether application candidate tracking is active");
    if e.target.arch == Arch::X86_64 { op(e, "", "jz __rt_gc_note_candidate_return", "keep legacy decrements outside queue maintenance"); }
    abi::emit_frame_prologue(e, 160);
    // -- preserve the decrement caller around bookkeeping --
    let registers: Vec<String> = if e.target.arch == Arch::AArch64 {
        (0..18).map(|index| format!("x{index}")).collect()
    } else {
        ["rax", "rcx", "rdx", "rsi", "rdi", "r8", "r9", "r10", "r11"].map(str::to_owned).to_vec()
    };
    for (index, reg) in registers.iter().enumerate() {
        op(e, &format!("str {reg}, [sp, #{}]", index * 8), &format!("mov QWORD PTR [rsp + {}], {reg}", index * 8), "preserve the caller's live integer arguments across bookkeeping");
    }
    abi::emit_call_label(e, "__rt_gc_candidate_queue");
    for (index, reg) in registers.iter().enumerate() {
        op(e, &format!("ldr {reg}, [sp, #{}]", index * 8), &format!("mov {reg}, QWORD PTR [rsp + {}]", index * 8), "restore every integer argument after queue maintenance");
    }
    abi::emit_frame_restore(e, 160);
    e.label("__rt_gc_note_candidate_return");
    abi::emit_return(e);
}

/// Preserves vector registers on the rare queue-growth path across the libc allocator.
fn emit_realloc(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: gc_candidate_realloc ---");
    e.label_global("__rt_gc_candidate_realloc");
    abi::emit_frame_prologue(e, 528);
    // -- preserve SIMD arguments around libc buffer growth --
    let count = if e.target.arch == Arch::AArch64 { 32 } else { 16 };
    for index in 0..count {
        op(e, &format!("str q{index}, [sp, #{}]", index * 16), &format!("movdqu XMMWORD PTR [rsp + {}], xmm{index}", index * 16), "preserve vector arguments across the rare libc allocation call");
    }
    e.bl_c("realloc"); // grow non-owning metadata while the PHP arena stays unchanged
    for index in 0..count {
        op(e, &format!("ldr q{index}, [sp, #{}]", index * 16), &format!("movdqu xmm{index}, XMMWORD PTR [rsp + {}]", index * 16), "restore vector arguments before returning to runtime ownership code");
    }
    abi::emit_frame_restore(e, 528);
    abi::emit_return(e);
}
