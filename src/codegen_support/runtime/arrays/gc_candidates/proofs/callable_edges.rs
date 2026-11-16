//! Purpose:
//! Validates retained ownership proofs through native callable captures.
//!
//! Called from:
//! - The physical-kind dispatcher in the retained graph edge predicate.
//!
//! Key details:
//! - Logical capture indexes survive descriptor aliases without storing slot pointers.
//! - Current capacity, environment bindings and exact child identity are checked each pass.
//! - Reference captures prove ownership of their managed cell, not its changing payload.

use super::{op, restore, save};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits a predicate with parent in x0/rax, capture index in x1/rdi and child in x2/rsi.
pub(super) fn emit(e: &mut Emitter) {
    e.blank();
    e.label_global("__rt_gc_proof_callable_edge");
    abi::emit_frame_prologue(e, 32);
    op(e, "str x2, [sp, #8]", "mov QWORD PTR [rsp + 8], rsi", "preserve the expected child across reference-cell normalization");
    op(e, "ldr w9, [x0, #-16]", "mov r8d, DWORD PTR [rax - 16]", "validate the current descriptor allocation capacity");
    op(e, "cmp x9, #64", "cmp r8, 64", "require the complete callable header before inspecting environment metadata");
    branch(e, "lo", "b", "invalid");
    op(e, "sub x9, x9, #64", "sub r8, 64", "exclude the fixed header from runtime capture capacity");
    op(e, "lsr x9, x9, #4", "shr r8, 4", "each initialized capture occupies two pointer-sized words");
    op(e, "cmp x1, x9", "cmp rdi, r8", "require this logical capture to fit inside current storage");
    branch(e, "hs", "ae", "invalid");
    op(e, "ldr x10, [x0, #40]", "mov r9, QWORD PTR [rax + 40]", "read the current immutable callable environment");
    op(e, "cmp x10, #0", "test r9, r9", "a descriptor without an environment cannot prove capture ownership");
    branch(e, "eq", "z", "invalid");
    op(e, "ldr x9, [x10]", "mov r8, QWORD PTR [r9]", "bound logical captures by the environment's actual binding count");
    op(e, "cmp x1, x9", "cmp rdi, r8", "reject removed or nonexistent capture indexes");
    branch(e, "hs", "ae", "invalid");
    op(e, "ldr x10, [x10, #16]", "mov r9, QWORD PTR [r9 + 16]", "require typed bindings before treating a capture as an owner");
    op(e, "cmp x10, #0", "test r9, r9", "untyped foreign environments do not establish a native ownership proof");
    branch(e, "eq", "z", "invalid");
    op(e, "add x10, x10, x1, lsl #5", "mov r8, rdi", "address the selected four-word capture binding");
    if e.target.arch == Arch::X86_64 {
        op(e, "", "shl r8, 5", "scale the logical index by the immutable binding record width");
        op(e, "", "add r9, r8", "locate the binding without retaining its address across calls");
    }
    op(e, "ldr x11, [x10, #24]", "mov r10, QWORD PTR [r9 + 24]", "reference bindings own a managed cell independently of its contents");
    op(e, "ldr x10, [x10, #16]", "mov r9, QWORD PTR [r9 + 16]", "recover the current by-value ownership representation");
    op(e, "add x0, x0, x1, lsl #4", "mov r8, rdi", "locate this runtime capture inside the validated allocation");
    if e.target.arch == Arch::X86_64 {
        op(e, "", "shl r8, 4", "scale the capture index by its two-word slot width");
        op(e, "", "add rax, r8", "address the capture relative to its current descriptor");
    }
    op(e, "ldr x0, [x0, #64]", "mov rax, QWORD PTR [rax + 64]", "borrow the current capture payload for exact identity comparison");
    op(e, "cmp x11, #0", "test r10, r10", "choose cell ownership before interpreting a by-value payload tag");
    branch(e, "ne", "nz", "reference");
    for tag in [4, 5, 6, 7, 10, 12] {
        op(e, &format!("cmp x10, #{tag}"), &format!("cmp r9, {tag}"), "recognize the same managed capture representations as the shared visitor");
        branch(e, "eq", "e", "compare");
    }
    abi::emit_jump(e, "__rt_gc_proof_callable_invalid");
    e.label("__rt_gc_proof_callable_reference");
    abi::emit_call_label(e, "__rt_reference_cell_owner");
    abi::emit_branch_if_int_result_zero(e, "__rt_gc_proof_callable_invalid");
    e.label("__rt_gc_proof_callable_compare");
    save(e, 0);
    restore(e, 8);
    op(e, "ldr x9, [sp]", "mov r10, QWORD PTR [rsp]", "recover the current owner without changing PHP refcounts");
    op(e, "cmp x0, x9", "cmp rax, r10", "replaced captures cannot prove liveness of their former child");
    branch(e, "ne", "ne", "invalid");
    op(e, "mov x0, #1", "mov eax, 1", "report a current typed ownership edge");
    abi::emit_jump(e, "__rt_gc_proof_callable_return");
    e.label("__rt_gc_proof_callable_invalid");
    op(e, "mov x0, #0", "xor eax, eax", "leave unproved nodes on the ordinary immediate collection path");
    e.label("__rt_gc_proof_callable_return");
    abi::emit_frame_restore(e, 32);
    abi::emit_return(e);
}

/// Selects local predicate paths using the target's condition-code syntax.
fn branch(e: &mut Emitter, arm: &str, x86: &str, label: &str) {
    op(e, &format!("b.{arm} __rt_gc_proof_callable_{label}"), &format!("j{x86} __rt_gc_proof_callable_{label}"), "reject a stale proof or select the next validated capture check");
}
