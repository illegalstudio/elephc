//! Purpose:
//! Normalizes suspended-frame owner slots and counts each physical owning slot once.
//!
//! Called from:
//! - Compiler-generated frame visitors and native exception guard adapters.
//!
//! Key details:
//! - A call-operand guard can relay a frame local's ownership without adding a reference.
//! - Per-walk native identity sets deduplicate slot addresses, never value identities.
//! - Pure cost mode reads no PHP values and allocates no slot table.

use super::gc_candidates::{op, proofs, restore, save};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Publishes native frame slots using the same pure slot visitor as generated PHP frames.
pub(crate) fn emit_native_frame_owners(e: &mut Emitter, label: &str, offsets: &[usize]) {
    e.label_global(label);
    abi::emit_frame_prologue(e, 32);
    op(e, "str x0, [sp]", "mov QWORD PTR [rsp], rdi", "borrow the native cleanup data base across pure slot callbacks");
    op(e, "str x1, [sp, #8]", "mov QWORD PTR [rsp + 8], rsi", "preserve this walk's independent context");
    for offset in offsets {
        op(e, "ldr x0, [sp]", "mov rdi, QWORD PTR [rsp]", "recover the same base used by native cleanup");
        op(e, &format!("add x0, x0, #{offset}"), &format!("add rdi, {offset}"), "address one independently owned native frame slot");
        op(e, "mov x1, #0", "xor esi, esi", "native slots hold direct owners rather than raw reference cells");
        op(e, "ldr x2, [sp, #8]", "mov rdx, QWORD PTR [rsp + 8]", "reuse the caller's mode and physical-slot set");
        abi::emit_call_label(e, "__rt_gc_frame_owner_slot");
    }
    abi::emit_frame_restore(e, 32);
    abi::emit_return(e);
}

/// Emits the C-ABI slot reader and its independent native slot-identity set.
pub fn emit_gc_frame_slots(e: &mut Emitter) {
    unique(e);
    e.blank();
    e.label_global("__rt_gc_frame_owner_slot");
    abi::emit_frame_prologue(e, 64);
    for (offset, arm, x86) in [(0, "x0", "rdi"), (8, "x1", "rsi"), (16, "x2", "rdx")] {
        op(e, &format!("str {arm}, [sp, #{offset}]"), &format!("mov QWORD PTR [rsp + {offset}], {x86}"), "retain this physical owner-slot address and traversal descriptor");
    }
    op(e, "ldr x9, [x2, #8]", "mov r10, QWORD PTR [rdx + 8]", "pure work accounting visits slots without tracing or allocating");
    op(e, "cmp x9, #3", "cmp r10, 3", "distinguish structural cost from real ownership traversal");
    op(e, "b.eq __rt_gc_frame_slot_hit", "je __rt_gc_frame_slot_hit", "charge each inspected slot including null and scalar wrappers");
    restore(e, 0);
    op(e, "ldr x0, [x0]", "mov rax, QWORD PTR [rax]", "borrow the slot's current owner without retaining it");
    abi::emit_call_label(e, "__rt_gc_candidate_live");
    abi::emit_branch_if_int_result_zero(e, "__rt_gc_frame_slot_done");
    op(e, "ldr x9, [sp, #8]", "mov r10, QWORD PTR [rsp + 8]", "raw fallback reference cells describe their separately owned payload");
    op(e, "cmp x9, #128", "cmp r10, 128", "ordinary locals and managed reference cells remain direct graph owners");
    op(e, "b.lo __rt_gc_frame_slot_ready", "jb __rt_gc_frame_slot_ready", "direct owners need no reference-cell payload normalization");
    op(e, "ldrb w10, [x0, #-8]", "movzx r11d, BYTE PTR [rax - 8]", "recognize raw storage only after exact allocation validation");
    op(e, "cmp x10, #0", "test r11, r11", "managed cells already own their payload through ordinary graph traversal");
    op(e, "b.ne __rt_gc_frame_slot_ready", "jnz __rt_gc_frame_slot_ready", "preserve independent managed cell aliases as distinct frame owners");
    op(e, "and x9, x9, #127", "and r10, 127", "recover the raw fallback cell's payload representation");
    op(e, "cmp x9, #4", "cmp r10, 4", "scalar and string reference payloads cannot establish graph cycles");
    op(e, "b.lo __rt_gc_frame_slot_done", "jb __rt_gc_frame_slot_done", "leave non-graph raw payloads to normal cancellation cleanup");
    save(e, 0);
    op(e, "ldr x0, [x0]", "mov rax, QWORD PTR [rax]", "the raw cell's first word is its single owning payload slot");
    abi::emit_call_label(e, "__rt_gc_candidate_live");
    abi::emit_branch_if_int_result_zero(e, "__rt_gc_frame_slot_done");
    e.label("__rt_gc_frame_slot_ready");
    save(e, 24);
    restore(e, 0);
    op(e, "ldr x1, [sp, #16]", "mov rdi, QWORD PTR [rsp + 16]", "deduplicate ownership slots in this traversal's private native set");
    abi::emit_call_label(e, "__rt_gc_frame_slot_unique");
    abi::emit_branch_if_int_result_zero(e, "__rt_gc_frame_slot_done");
    restore(e, 24);
    op(e, "ldr x9, [sp, #16]", "mov r10, QWORD PTR [rsp + 16]", "recover this independent walk's mode and exact selected candidate");
    op(e, "ldr x10, [x9, #8]", "mov r11, QWORD PTR [r10 + 8]", "choose reachability or incoming ownership counting");
    op(e, "cmp x10, #1", "cmp r11, 1", "only marking and candidate discovery follow child graphs");
    op(e, "b.eq __rt_gc_frame_slot_mark", "je __rt_gc_frame_slot_mark", "route real owners through the reviewed shared marker");
    if e.target.arch == Arch::X86_64 {
        op(e, "", "cmp r11, 2", "cache priming records every owned child");
        op(e, "", "je __rt_gc_frame_slot_note", "use the shared saturating incoming cache");
        op(e, "", "cmp rax, QWORD PTR [r10]", "an exact saturated recount counts only the requested candidate");
        op(e, "", "jne __rt_gc_frame_slot_done", "other frame owners do not contribute to this candidate");
        abi::emit_jump(e, "__rt_gc_frame_slot_hit");
    }
    e.label("__rt_gc_frame_slot_note");
    abi::emit_call_label(e, "__rt_gc_note_child_ref");
    abi::emit_jump(e, "__rt_gc_frame_slot_done");
    e.label("__rt_gc_frame_slot_mark");
    proofs::slot(e, "suspended_frame", None, -1);
    abi::emit_call_label(e, "__rt_gc_mark_reachable");
    abi::emit_jump(e, "__rt_gc_frame_slot_done");
    e.label("__rt_gc_frame_slot_hit");
    op(e, "ldr x9, [sp, #16]", "mov r10, QWORD PTR [rsp + 16]", "accumulate exact candidate hits or pure inspected-slot work");
    op(e, "ldr x10, [x9, #16]", "mov r11, QWORD PTR [r10 + 16]", "read this walk's private result count");
    op(e, "add x10, x10, #1", "add r11, 1", "charge one independent ownership slot");
    op(e, "str x10, [x9, #16]", "mov QWORD PTR [r10 + 16], r11", "publish the result without changing PHP ownership");
    e.label("__rt_gc_frame_slot_done");
    abi::emit_frame_restore(e, 64);
    abi::emit_return(e);
}

/// Inserts an address with amortized constant work; native storage belongs to one frame walk.
fn unique(e: &mut Emitter) {
    e.blank();
    e.label_global("__rt_gc_frame_slot_unique");
    abi::emit_frame_prologue(e, 80);
    save(e, 0);
    op(e, "str x1, [sp, #8]", "mov QWORD PTR [rsp + 8], rdi", "retain this walk's private hash-set context");
    op(e, "ldr x9, [x1, #32]", "mov r10, QWORD PTR [rdi + 32]", "read the current power-of-two slot capacity");
    op(e, "ldr x10, [x1, #40]", "mov r11, QWORD PTR [rdi + 40]", "grow before the slot set reaches half occupancy");
    op(e, "add x10, x10, #1", "add r11, 1", "reserve capacity for the prospective new owner slot");
    op(e, "lsl x10, x10, #1", "shl r11, 1", "keep probing bounded by the geometric load-factor rule");
    op(e, "cmp x10, x9", "cmp r11, r10", "reuse the current buffer when its load remains below one half");
    op(e, "b.lo __rt_gc_slot_set_ready", "jb __rt_gc_slot_set_ready", "avoid native allocation on ordinary insertions");
    op(e, "str x9, [sp, #24]", "mov QWORD PTR [rsp + 24], r10", "retain the old capacity for rehashing");
    op(e, "ldr x10, [x1, #24]", "mov r11, QWORD PTR [rdi + 24]", "retain the old native bucket array until every slot is moved");
    op(e, "str x10, [sp, #16]", "mov QWORD PTR [rsp + 16], r11", "the old buffer is never a PHP owner");
    op(e, "lsl x0, x9, #1", "lea rdi, [r10 + r10]", "double the independent slot table geometrically");
    op(e, "cmp x0, #64", "cmp rdi, 64", "give small suspended frames a useful initial native table");
    op(e, "b.hs __rt_gc_slot_set_capacity", "jae __rt_gc_slot_set_capacity", "retain an already sufficient doubled capacity");
    op(e, "mov x0, #64", "mov edi, 64", "initialize the first table with sixty-four empty buckets");
    e.label("__rt_gc_slot_set_capacity");
    op(e, "str x0, [sp, #40]", "mov QWORD PTR [rsp + 40], rdi", "preserve the new capacity across libc allocation");
    op(e, "mov x1, #8", "mov esi, 8", "each native bucket stores one non-owning slot address");
    e.bl_c("calloc");
    abi::emit_branch_if_int_result_nonzero(e, "__rt_gc_slot_set_allocated");
    abi::emit_jump(e, "__rt_heap_allocation_failed");
    e.label("__rt_gc_slot_set_allocated");
    save(e, 32);
    op(e, "ldr x9, [sp, #8]", "mov r10, QWORD PTR [rsp + 8]", "recover this set's private owner context");
    op(e, "str x0, [x9, #24]", "mov QWORD PTR [r10 + 24], rax", "publish the newly allocated native bucket array");
    restore(e, 40);
    op(e, "str x0, [x9, #32]", "mov QWORD PTR [r10 + 32], rax", "publish its complete power-of-two capacity");
    op(e, "mov x0, #0", "xor eax, eax", "begin rehashing old buckets from index zero");
    save(e, 48);
    e.label("__rt_gc_slot_set_rehash");
    restore(e, 48);
    op(e, "ldr x9, [sp, #24]", "mov r10, QWORD PTR [rsp + 24]", "stop after the complete previous bucket array");
    op(e, "cmp x0, x9", "cmp rax, r10", "only initialized old buckets may be inspected");
    op(e, "b.hs __rt_gc_slot_set_rehashed", "jae __rt_gc_slot_set_rehashed", "dispose the old buffer after all slots were moved");
    op(e, "ldr x9, [sp, #16]", "mov r10, QWORD PTR [rsp + 16]", "address the old buffer without retaining a PHP value");
    op(e, "ldr x9, [x9, x0, lsl #3]", "mov r10, QWORD PTR [r10 + rax * 8]", "borrow one old slot identity");
    op(e, "add x0, x0, #1", "add rax, 1", "advance the old cursor before probing the new table");
    save(e, 48);
    op(e, "mov x0, x9", "mov rax, r10", "rehash only occupied old buckets");
    abi::emit_branch_if_int_result_zero(e, "__rt_gc_slot_set_rehash");
    buckets(e, "rehash");
    abi::emit_jump(e, "__rt_gc_slot_set_rehash");
    e.label("__rt_gc_slot_set_rehashed");
    restore(e, 16);
    if e.target.arch == Arch::X86_64 { op(e, "", "mov rdi, rax", "release only the previous native bucket array"); }
    e.bl_c("free");
    e.label("__rt_gc_slot_set_ready");
    restore(e, 0);
    buckets(e, "insert");
    op(e, "ldr x9, [sp, #8]", "mov r10, QWORD PTR [rsp + 8]", "count the newly inserted physical owner slot");
    op(e, "ldr x10, [x9, #40]", "mov r11, QWORD PTR [r10 + 40]", "read the current number of unique ownership slots");
    op(e, "add x10, x10, #1", "add r11, 1", "record one newly observed independent owner");
    op(e, "str x10, [x9, #40]", "mov QWORD PTR [r10 + 40], r11", "publish set occupancy independently of PHP refcounts");
    op(e, "mov x0, #1", "mov eax, 1", "allow this new ownership slot to contribute exactly once");
    abi::emit_jump(e, "__rt_gc_slot_set_return");
    e.label("__rt_gc_slot_set_duplicate");
    op(e, "mov x0, #0", "xor eax, eax", "a relay guard adds no owner beyond the original frame slot");
    e.label("__rt_gc_slot_set_return");
    abi::emit_frame_restore(e, 80);
    abi::emit_return(e);
}

/// Probes one power-of-two bucket array without calling PHP or retaining heap values.
fn buckets(e: &mut Emitter, name: &str) {
    op(e, "ldr x9, [sp, #8]", "mov r8, QWORD PTR [rsp + 8]", "recover the independent native slot-set context");
    op(e, "ldr x10, [x9, #32]", "mov r9, QWORD PTR [r8 + 32]", "load the new or reused bucket capacity");
    op(e, "ldr x9, [x9, #24]", "mov r8, QWORD PTR [r8 + 24]", "borrow the bucket array owned by this walk");
    op(e, "sub x10, x10, #1", "sub r9, 1", "derive the wrap mask from a power-of-two capacity");
    op(e, "lsr x11, x0, #3", "mov r10, rax", "hash aligned slot addresses rather than pointed-to PHP identities");
    if e.target.arch == Arch::X86_64 { op(e, "", "shr r10, 3", "remove the alignment bits from this slot address"); }
    op(e, "eor x11, x11, x0, lsr #13", "mov r11, rax", "mix different stack pages into the slot hash");
    if e.target.arch == Arch::X86_64 {
        op(e, "", "shr r11, 13", "select stack-page bits for the hash");
        op(e, "", "xor r10, r11", "combine slot and stack-page coordinates");
    }
    e.label(&format!("__rt_gc_slot_set_{name}_probe"));
    op(e, "and x11, x11, x10", "and r10, r9", "wrap linear probing within the allocated bucket array");
    op(e, "ldr x12, [x9, x11, lsl #3]", "mov r11, QWORD PTR [r8 + r10 * 8]", "inspect one native bucket without touching PHP storage");
    op(e, "cmp x12, #0", "test r11, r11", "an empty bucket terminates this bounded insertion search");
    op(e, &format!("b.eq __rt_gc_slot_set_{name}_store"), &format!("jz __rt_gc_slot_set_{name}_store"), "store the slot at the first free native bucket");
    if name == "insert" {
        op(e, "cmp x12, x0", "cmp r11, rax", "duplicate guard/local addresses represent one physical owner");
        op(e, "b.eq __rt_gc_slot_set_duplicate", "je __rt_gc_slot_set_duplicate", "reject an already counted ownership relay");
    }
    op(e, "add x11, x11, #1", "add r10, 1", "try the next bucket while maintaining the half-full invariant");
    abi::emit_jump(e, &format!("__rt_gc_slot_set_{name}_probe"));
    e.label(&format!("__rt_gc_slot_set_{name}_store"));
    op(e, "str x0, [x9, x11, lsl #3]", "mov QWORD PTR [r8 + r10 * 8], rax", "record the physical slot address without adding a PHP owner");
}
