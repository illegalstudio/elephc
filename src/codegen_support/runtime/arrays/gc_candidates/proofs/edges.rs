//! Purpose:
//! Validates one cached parent-to-child ownership edge against the current runtime layout.
//!
//! Called from:
//! - Retained application proof lookup, after exact parent-base validation.
//!
//! Key details:
//! - Hash proofs store logical slot indexes, so rehashing never leaves borrowed entry pointers.
//! - Every typed edge is checked again; a pointer-shaped scalar cannot establish ownership.

use super::super::{load, op};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch, sentinels::{REFERENCE_CELL_HEAP_KIND, CALLABLE_HEAP_KIND}};

/// Emits a leaf predicate with parent in x0/rax, logical slot in x1/rdi, child in x2/rsi.
pub(super) fn emit(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: gc_proof_edge ---");
    e.label_global("__rt_gc_proof_edge");
    // -- select the parent's actual ownership representation --
    op(e, "ldr w9, [x0, #-16]", "mov r8d, DWORD PTR [rax - 16]", "bound in-payload edge accesses by the current allocation capacity");
    op(e, "ldrb w10, [x0, #-8]", "movzx r9d, BYTE PTR [rax - 8]", "dispatch using the current physical layout, including reused addresses");
    for (kind, label) in [(2, "array"), (3, "hash"), (4, "object"), (5, "box"), (REFERENCE_CELL_HEAP_KIND as u64, "reference")] {
        op(e, &format!("cmp x10, #{kind}"), &format!("cmp r9, {kind}"), "recognize this graph representation before validating its logical slot");
        abi::emit_branch_if_equal_wide(e, &format!("__rt_gc_proof_edge_{label}"));
    }
    op(e, &format!("cmp x10, #{CALLABLE_HEAP_KIND}"), &format!("cmp r9, {CALLABLE_HEAP_KIND}"), "native callable captures have their own typed logical slot predicate");
    op(e, "b.ne __rt_gc_proof_edge_invalid", "jne __rt_gc_proof_edge_invalid", "reject unsupported physical representations without reading their payloads");
    abi::emit_jump(e, "__rt_gc_proof_callable_edge");

    // -- indexed arrays own only initialized, graph-bearing element slots --
    e.label("__rt_gc_proof_edge_array");
    op(e, "cmp x1, #24", "cmp rdi, 24", "exclude the indexed array's non-element header words");
    branch(e, "lo", "b", "invalid");
    op(e, "sub x11, x1, #24", "lea r10, [rdi - 24]", "convert the saved byte offset to an element position");
    op(e, "tst x11, #7", "test r10, 7", "require a complete aligned pointer element");
    branch(e, "ne", "nz", "invalid");
    op(e, "lsr x11, x11, #3", "shr r10, 3", "recover the logical element index");
    op(e, "ldr x12, [x0]", "mov r11, QWORD PTR [rax]", "inspect the array's current initialized length");
    op(e, "cmp x11, x12", "cmp r10, r11", "exclude an element removed since the proof was built");
    branch(e, "hs", "ae", "invalid");
    descriptor(e);
    abi::emit_jump(e, "__rt_gc_proof_edge_typed");

    // -- Mixed and reference cells have one explicitly tagged graph slot --
    e.label("__rt_gc_proof_edge_box");
    op(e, "cmp x1, #8", "cmp rdi, 8", "only the Mixed payload word can own this child");
    branch(e, "ne", "ne", "invalid");
    op(e, "ldr x10, [x0]", "mov r9, QWORD PTR [rax]", "read the box's current runtime payload tag");
    abi::emit_jump(e, "__rt_gc_proof_edge_typed");
    e.label("__rt_gc_proof_edge_reference");
    op(e, "cmp x1, #0", "cmp rdi, 0", "only the reference cell's payload word is an owning edge");
    branch(e, "ne", "ne", "invalid");
    descriptor(e);
    e.label("__rt_gc_proof_edge_typed");
    graph_tag(e, false, "typed");
    abi::emit_jump(e, "__rt_gc_proof_edge_payload");

    // -- hashes reacquire the current separately allocated entry storage --
    e.label("__rt_gc_proof_edge_hash");
    op(e, "cmp x9, #64", "cmp r8, 64", "require a complete current hash header before reading its storage");
    branch(e, "lo", "b", "invalid");
    op(e, "ldr x11, [x0, #8]", "mov r10, QWORD PTR [rax + 8]", "read the current entry capacity after any rehash");
    op(e, "cmp x1, x11", "cmp rdi, r10", "exclude indexes outside the current entry allocation");
    branch(e, "hs", "ae", "invalid");
    op(e, "ldr x11, [x0, #40]", "mov r10, QWORD PTR [rax + 40]", "borrow the current entry buffer rather than a cached native address");
    op(e, "add x11, x11, x1, lsl #6", "mov r11, rdi", "locate the saved logical entry in the current storage");
    if e.target.arch == Arch::X86_64 {
        op(e, "", "shl r11, 6", "scale the logical hash slot by the entry width");
        op(e, "", "add r10, r11", "address this entry in the current hash buffer");
    }
    op(e, "ldr x12, [x11]", "mov r11, QWORD PTR [r10]", "check that the saved entry is still occupied");
    op(e, "cmp x12, #1", "cmp r11, 1", "tombstones and empty entries cannot retain a child");
    branch(e, "ne", "ne", "invalid");
    op(e, "ldr x10, [x11, #40]", "mov r9, QWORD PTR [r10 + 40]", "inspect the entry's current value ownership tag");
    graph_tag(e, true, "hash");
    op(e, "ldr x11, [x11, #24]", "mov r10, QWORD PTR [r10 + 24]", "read the currently owned child from the occupied entry");
    abi::emit_jump(e, "__rt_gc_proof_edge_compare");

    // -- objects use current class descriptors, excluding scalar and removed properties --
    e.label("__rt_gc_proof_edge_object");
    op(e, "ldr x13, [x0]", "mov rdx, QWORD PTR [rax]", "read the current class identity before consulting its property layout");
    load(e, "x11", "r10", "_class_gc_desc_count");
    op(e, "cmp x13, x11", "cmp rdx, r10", "require a valid emitted class descriptor");
    branch(e, "hs", "ae", "invalid");
    address(e, "x11", "r10", "_class_object_payload_sizes");
    op(e, "ldr x11, [x11, x13, lsl #3]", "mov r10, QWORD PTR [r10 + rdx * 8]", "use the declared layout rather than recycled allocation capacity");
    op(e, "cmp x11, x9", "cmp r10, r8", "exclude a class layout that cannot fit this allocation");
    branch(e, "hi", "a", "invalid");
    op(e, "cmp x1, #8", "cmp rdi, 8", "exclude the leading class identity word");
    branch(e, "lo", "b", "invalid");
    op(e, "cmp x1, x11", "cmp rdi, r10", "keep the saved property inside the declared object layout");
    branch(e, "hs", "ae", "invalid");
    address(e, "x12", "r11", "_class_object_dynamic_prop_flags");
    op(e, "ldr x12, [x12, x13, lsl #3]", "mov r11, QWORD PTR [r11 + rdx * 8]", "recognize an owned dynamic-property hash at the final payload word");
    op(e, "sub x11, x11, #8", "sub r10, 8", "locate the optional final dynamic-property slot");
    op(e, "cmp x1, x11", "cmp rdi, r10", "distinguish the dynamic tail from ordinary fixed properties");
    branch(e, "ne", "ne", "fixed");
    op(e, "cmp x12, #0", "test r11, r11", "require a declared owning dynamic-property tail");
    branch(e, "eq", "z", "invalid");
    abi::emit_jump(e, "__rt_gc_proof_edge_payload");
    e.label("__rt_gc_proof_edge_fixed");
    op(e, "sub x11, x1, #8", "lea r10, [rdi - 8]", "derive the fixed-property slot relative to the class word");
    op(e, "tst x11, #15", "test r10, 15", "only a property's payload word establishes an owning edge");
    branch(e, "ne", "nz", "invalid");
    op(e, "lsr x11, x11, #4", "shr r10, 4", "recover the property descriptor index");
    address(e, "x12", "r11", "_class_gc_desc_ptrs");
    op(e, "ldr x12, [x12, x13, lsl #3]", "mov r11, QWORD PTR [r11 + rdx * 8]", "read the descriptor belonging to the current class");
    op(e, "ldrb w10, [x12, x11]", "movzx r9d, BYTE PTR [r11 + r10]", "check the property's actual declared ownership representation");
    graph_tag(e, true, "object");

    // -- an owning slot proves this exact child only while its current value still matches --
    e.label("__rt_gc_proof_edge_payload");
    op(e, "add x11, x1, #8", "lea r10, [rdi + 8]", "include the complete pointer word in the allocation bounds check");
    op(e, "cmp x11, x9", "cmp r10, r8", "bound the final payload read by current storage capacity");
    branch(e, "hi", "a", "invalid");
    op(e, "ldr x11, [x0, x1]", "mov r10, QWORD PTR [rax + rdi]", "read the current child from the validated owning slot");
    e.label("__rt_gc_proof_edge_compare");
    op(e, "cmp x11, x2", "cmp r10, rsi", "reject replaced children even when their old addresses remain live");
    branch(e, "ne", "ne", "invalid");
    abi::emit_load_int_immediate(e, abi::int_result_reg(e), 1);
    abi::emit_return(e);
    e.label("__rt_gc_proof_edge_invalid");
    abi::emit_load_int_immediate(e, abi::int_result_reg(e), 0);
    abi::emit_return(e);
}

/// Reads the graph-bearing array or reference payload descriptor from the current header.
fn descriptor(e: &mut Emitter) {
    op(e, "ldr x10, [x0, #-8]", "mov r9, QWORD PTR [rax - 8]", "inspect the current packed payload representation");
    op(e, "ubfx x10, x10, #8, #7", "shr r9, 8", "isolate the descriptor from physical kind and collector metadata");
    if e.target.arch == Arch::X86_64 { op(e, "", "and r9d, 0x7f", "exclude COW and collector flags from payload typing"); }
}

/// Requires a graph-bearing descriptor, optionally including managed reference entries.
fn graph_tag(e: &mut Emitter, reference: bool, name: &str) {
    let next = format!("__rt_gc_proof_edge_tag_{name}");
    op(e, "cmp x10, #10", "cmp r9, 10", "a current callable slot owns its managed descriptor");
    abi::emit_branch_if_equal_wide(e, &next);
    if reference {
        op(e, "cmp x10, #11", "cmp r9, 11", "accept a managed reference cell as an independent owning edge");
        abi::emit_branch_if_equal_wide(e, &next);
    }
    tag_range(e);
    e.label(&next);
}

/// Restricts tags to array, hash, object or boxed Mixed ownership representations.
fn tag_range(e: &mut Emitter) {
    op(e, "cmp x10, #4", "cmp r9, 4", "exclude scalar, string and null values from ownership proofs");
    branch(e, "lo", "b", "invalid");
    op(e, "cmp x10, #7", "cmp r9, 7", "exclude unsupported or non-owning payload representations");
    branch(e, "hi", "a", "invalid");
}

/// Emits a local representation check in the appropriate condition-code dialect.
fn branch(e: &mut Emitter, arm: &str, x86: &str, target: &str) {
    op(e, &format!("b.{arm} __rt_gc_proof_edge_{target}"), &format!("j{x86} __rt_gc_proof_edge_{target}"), "reject a stale ownership claim or select the next valid layout check");
}

/// Addresses immutable class-layout metadata without borrowing live ownership operands.
fn address(e: &mut Emitter, arm: &str, x86: &str, symbol: &str) {
    abi::emit_symbol_address(e, if e.target.arch == Arch::AArch64 { arm } else { x86 }, symbol);
}
