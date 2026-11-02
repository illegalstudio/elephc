//! Purpose:
//! Traverses typed capture owners of managed native callable descriptors.
//!
//! Called from:
//! - Both collectors, the shared child visitor and structural scan accounting.
//!
//! Key details:
//! - C ABI modes match eval object traversal: count, mark, prime, pure slot count.
//! - Reference captures normalize their storage address to the owning managed cell.
//! - Capture proofs save logical indexes and revalidate typed ownership on each pass.

use super::gc_candidates::{op, proofs, restore, save};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Enumerates captures without retaining values or invoking PHP code.
pub fn emit_gc_callable_children(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: gc_callable_children ---");
    e.label_global("__rt_gc_callable_children");
    abi::emit_frame_prologue(e, 96);
    // -- preserve the C arguments and immutable capture enumeration metadata --
    op(e, "str x0, [sp]", "mov QWORD PTR [rsp], rdi", "preserve the managed descriptor across child traversal");
    op(e, "str x1, [sp, #8]", "mov QWORD PTR [rsp + 8], rsi", "preserve the candidate used by exact x86 incoming recounts");
    op(e, "str x2, [sp, #16]", "mov QWORD PTR [rsp + 16], rdx", "preserve count, mark, prime or pure work-accounting mode");
    op(e, "mov x0, #0", "xor eax, eax", "initialize the exact matching-edge result");
    save(e, 48);
    save(e, 40);
    restore(e, 0);
    op(e, "ldr x9, [x0, #40]", "mov r10, QWORD PTR [rax + 40]", "read the immutable environment metadata, not captured PHP values");
    op(e, "cbz x9, __rt_gc_callable_done", "test r10, r10", "descriptors without an environment have no capture slots");
    if e.target.arch == Arch::X86_64 { op(e, "", "jz __rt_gc_callable_done", "finish without dereferencing missing environment metadata"); }
    op(e, "ldr x0, [x9]", "mov rax, QWORD PTR [r10]", "count every capture slot, including scalar and string slots");
    save(e, 24);
    op(e, "ldr x9, [x9, #16]", "mov r10, QWORD PTR [r10 + 16]", "read the immutable typed binding table");
    op(e, "str x9, [sp, #32]", "mov QWORD PTR [rsp + 32], r10", "preserve the binding table across recursive child marking");
    op(e, "ldr x9, [sp, #16]", "mov r10, QWORD PTR [rsp + 16]", "pure scan accounting needs the slot count without tracing values");
    op(e, "cmp x9, #3", "cmp r10, 3", "distinguish work estimation from actual graph edge processing");
    op(e, "b.eq __rt_gc_callable_return", "je __rt_gc_callable_return", "return all inspected capture slots without ownership side effects");
    // -- each binding entry is four words and each runtime capture is two words --
    e.label("__rt_gc_callable_loop");
    restore(e, 40);
    op(e, "ldr x9, [sp, #24]", "mov r10, QWORD PTR [rsp + 24]", "recover the finite capture count");
    op(e, "cmp x0, x9", "cmp rax, r10", "stop after every initialized capture slot");
    op(e, "b.hs __rt_gc_callable_done", "jae __rt_gc_callable_done", "complete traversal without reading beyond capture metadata");
    op(e, "ldr x10, [sp, #32]", "mov r11, QWORD PTR [rsp + 32]", "recover the immutable typed binding table");
    op(e, "cbz x10, __rt_gc_callable_done", "test r11, r11", "missing typed bindings have no known owned capture edges");
    if e.target.arch == Arch::X86_64 { op(e, "", "jz __rt_gc_callable_done", "avoid interpreting untyped foreign capture storage"); }
    op(e, "add x10, x10, x0, lsl #5", "mov r10, rax", "locate this capture's four-word binding metadata");
    if e.target.arch == Arch::X86_64 {
        op(e, "", "shl r10, 5", "scale the binding index by its exact record width");
        op(e, "", "add r11, r10", "address the selected capture binding");
    }
    op(e, "ldr x11, [x10, #24]", "mov rcx, QWORD PTR [r11 + 24]", "identify captures that own a reference cell rather than its current payload");
    op(e, "ldr x10, [x10, #16]", "mov r11, QWORD PTR [r11 + 16]", "read the by-value capture representation");
    op(e, "mov x9, x0", "mov r10, rax", "preserve the capture index while recovering its descriptor");
    restore(e, 0);
    if e.target.arch == Arch::X86_64 { op(e, "", "shl r10, 4", "scale to the two-word runtime capture slot"); }
    op(e, "add x0, x0, x9, lsl #4", "lea rax, [rax + r10]", "locate the runtime capture relative to the fixed header");
    op(e, "ldr x0, [x0, #64]", "mov rax, QWORD PTR [rax + 64]", "borrow the capture payload without acquiring an owner");
    op(e, "cbnz x11, __rt_gc_callable_reference", "test rcx, rcx", "reference bindings own a normalized cell, independent of its changing contents");
    if e.target.arch == Arch::X86_64 { op(e, "", "jnz __rt_gc_callable_reference", "normalize reference storage before counting its ownership"); }
    for tag in [4, 5, 6, 7, 10, 12] {
        op(e, &format!("cmp x10, #{tag}"), &format!("cmp r11, {tag}"), "recognize managed containers, nested callables or erased iterable captures");
        op(e, "b.eq __rt_gc_callable_child", "je __rt_gc_callable_child", "process the independently owned capture edge");
    }
    abi::emit_jump(e, "__rt_gc_callable_next");
    e.label("__rt_gc_callable_reference");
    abi::emit_call_label(e, "__rt_reference_cell_owner");
    // -- collector modes borrow the same child, without running user callbacks --
    e.label("__rt_gc_callable_child");
    op(e, "ldr x9, [sp, #16]", "mov r10, QWORD PTR [rsp + 16]", "select counting versus reachability for this child");
    op(e, "cmp x9, #1", "cmp r10, 1", "only marking and candidate discovery recurse through graph ownership");
    op(e, "b.eq __rt_gc_callable_mark", "je __rt_gc_callable_mark", "use the shared visitor for normal marking and every gathering mode");
    if e.target.arch == Arch::X86_64 {
        op(e, "", "cmp r10, 2", "all-edge priming updates the child's saturating incoming cache");
        op(e, "", "je __rt_gc_callable_note", "record every incoming edge during cache priming");
        op(e, "", "cmp rax, QWORD PTR [rsp + 8]", "an exact recount counts only the selected saturated candidate");
        op(e, "", "jne __rt_gc_callable_next", "other capture owners do not contribute to this candidate");
        op(e, "", "add QWORD PTR [rsp + 48], 1", "retain a full-width exact incoming count");
        abi::emit_jump(e, "__rt_gc_callable_next");
    }
    e.label("__rt_gc_callable_note");
    abi::emit_call_label(e, "__rt_gc_note_child_ref");
    abi::emit_jump(e, "__rt_gc_callable_next");
    e.label("__rt_gc_callable_mark");
    op(e, "ldr x14, [sp, #40]", "mov r8, QWORD PTR [rsp + 40]", "publish the logical capture index without borrowing a mutable slot address");
    proofs::slot(e, "callable_capture", Some(("x14", "r8", 0)), 0);
    abi::emit_call_label(e, "__rt_gc_mark_reachable");
    e.label("__rt_gc_callable_next");
    restore(e, 40);
    op(e, "add x0, x0, #1", "add rax, 1", "advance one capture without relying on volatile child-call registers");
    save(e, 40);
    abi::emit_jump(e, "__rt_gc_callable_loop");
    e.label("__rt_gc_callable_done");
    restore(e, 48);
    e.label("__rt_gc_callable_return");
    abi::emit_frame_restore(e, 96);
    abi::emit_return(e);
}
