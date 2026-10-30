//! Purpose:
//! Enumerates real owners in the saved activation chain of a suspended coroutine.
//!
//! Called from:
//! - The shared object-child boundary in both collectors and scan-cost accounting.
//!
//! Key details:
//! - Each activation publishes a pure visitor beside its existing cleanup/backtrace fields.
//! - Traversals have independent slot-identity sets, including recursively visited coroutines.
//! - Only suspended native coroutine stacks are inspected; frame edges remain opaque proofs.

use super::gc_candidates::{load, op, restore, save};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch, runtime::fibers};

/// Emits saved-frame traversal and adapters for transferred guard and call-operand owners.
pub fn emit_gc_frame_children(e: &mut Emitter) {
    e.blank();
    e.label_global("__rt_gc_frame_children");
    abi::emit_frame_prologue(e, 144);
    for (i, (arm, x86)) in [("x0", "rdi"), ("x1", "rsi"), ("x2", "rdx")].iter().enumerate() {
        op(e, &format!("str {arm}, [sp, #{}]", i * 8), &format!("mov QWORD PTR [rsp + {}], {x86}", i * 8), "retain the native owner traversal inputs");
    }
    for offset in [64, 72, 80, 88, 96, 104] {
        op(e, &format!("str xzr, [sp, #{offset}]"), &format!("mov QWORD PTR [rsp + {offset}], 0"), "initialize this walk's independent count and slot-identity set");
    }
    restore(e, 0);
    abi::emit_branch_if_int_result_zero(e, "__rt_gc_frames_done");
    op(e, "ldr x9, [x0]", "mov r10, QWORD PTR [rax]", "identify a native coroutine before reading saved stack state");
    for symbol in ["_fiber_class_id", "_generator_class_id"] {
        load(e, "x10", "r11", symbol);
        op(e, "cmp x9, x10", "cmp r10, r11", "only native coroutine class identities have saved activations");
        op(e, "b.eq __rt_gc_frames_native", "je __rt_gc_frames_native", "select the guarded coroutine layout");
    }
    abi::emit_jump(e, "__rt_gc_frames_done");
    e.label("__rt_gc_frames_native");
    op(e, "ldr w9, [x0, #-16]", "mov r10d, DWORD PTR [rax - 16]", "bound native layout reads by allocator-owned capacity");
    op(e, &format!("cmp x9, #{}", fibers::FIBER_OBJECT_SIZE), &format!("cmp r10d, {}", fibers::FIBER_OBJECT_SIZE), "require a complete native coroutine payload");
    op(e, "b.lo __rt_gc_frames_done", "jb __rt_gc_frames_done", "reject incomplete native storage");
    op(e, "ldr x9, [x0, #8]", "mov r10, QWORD PTR [rax + 8]", "running and terminated stacks are not suspended ownership graphs");
    op(e, "cmp x9, #2", "cmp r10, 2", "inspect only the stable saved activation chain of a suspended coroutine");
    op(e, "b.ne __rt_gc_frames_done", "jne __rt_gc_frames_done", "leave active frames as ordinary external owners");
    for (field, local) in [(fibers::FIBER_OWN_CALL_FRAME_OFFSET, 24), (fibers::FIBER_STACK_BASE_OFFSET, 32), (fibers::FIBER_STACK_TOP_OFFSET, 40)] {
        op(e, &format!("ldr x9, [x0, #{field}]"), &format!("mov r10, QWORD PTR [rax + {field}]"), "borrow a saved stack bound or activation cursor");
        op(e, &format!("str x9, [sp, #{local}]"), &format!("mov QWORD PTR [rsp + {local}], r10"), "preserve native stack metadata across pure visitor calls");
    }
    for (source, destination) in [(8, 64), (16, 72)] { restore(e, source); save(e, destination); }
    e.label("__rt_gc_frames_loop");
    restore(e, 24);
    abi::emit_branch_if_int_result_zero(e, "__rt_gc_frames_done");
    op(e, "ldr x9, [sp, #32]", "mov r10, QWORD PTR [rsp + 32]", "keep saved activations inside their owning mapped stack");
    op(e, "cmp x0, x9", "cmp rax, r10", "reject a foreign or retired activation link");
    op(e, "b.lo __rt_gc_frames_done", "jb __rt_gc_frames_done", "stop before reading outside the coroutine stack");
    op(e, "add x9, x0, #32", "lea r10, [rax + 32]", "require the complete shared activation and guard prefix");
    op(e, "ldr x10, [sp, #40]", "mov r11, QWORD PTR [rsp + 40]", "bound the record by the mapped stack's usable top");
    op(e, "cmp x9, x10", "cmp r10, r11", "exclude incomplete records at the stack boundary");
    op(e, "b.hi __rt_gc_frames_done", "ja __rt_gc_frames_done", "do not inspect a partial activation");
    op(e, "ldr x9, [sp, #72]", "mov r10, QWORD PTR [rsp + 72]", "structural accounting includes activation records even when they own no graph slot");
    op(e, "cmp x9, #3", "cmp r10, 3", "charge the same saved-chain traversal performed by actual collection");
    op(e, "b.ne __rt_gc_frames_record_charged", "jne __rt_gc_frames_record_charged", "ordinary traversal counts only real matching child owners");
    op(e, "ldr x9, [sp, #80]", "mov r10, QWORD PTR [rsp + 80]", "retain work for each validated activation record");
    op(e, "add x9, x9, #1", "add r10, 1", "charge the saved-chain step independently of the local slot inventory");
    op(e, "str x9, [sp, #80]", "mov QWORD PTR [rsp + 80], r10", "publish complete native frame scan work");
    e.label("__rt_gc_frames_record_charged");
    op(e, "ldr x9, [x0]", "mov r10, QWORD PTR [rax]", "advance independently of child traversal and nested coroutine walks");
    op(e, "str x9, [sp, #24]", "mov QWORD PTR [rsp + 24], r10", "retain the next saved activation before invoking its visitor");
    op(e, "ldr x9, [x0, #16]", "mov r10, QWORD PTR [rax + 16]", "the compact guard ABI identifies itself through its callback data pointer");
    op(e, "cmp x9, x0", "cmp r10, rax", "compact transferred-owner guards remain valid without expanding native records");
    op(e, "b.eq __rt_gc_frames_guard", "je __rt_gc_frames_guard", "select the compact guard adapter before reading extended frame metadata");
    op(e, "ldr x10, [sp, #40]", "mov r11, QWORD PTR [rsp + 40]", "recover the stack bound after classifying compact guard records");
    op(e, "add x9, x0, #48", "lea r10, [rax + 48]", "regular frames and call-operand records publish an extended owner visitor");
    op(e, "cmp x9, x10", "cmp r10, r11", "require the complete extended record inside its mapped stack");
    op(e, "b.hi __rt_gc_frames_done", "ja __rt_gc_frames_done", "reject a truncated extended activation record");
    op(e, "ldr x10, [x0, #40]", "mov r11, QWORD PTR [rax + 40]", "read the pure typed owner visitor published with this activation");
    op(e, "cmp x10, #0", "test r11, r11", "synthetic non-owning activations publish no owner visitor");
    op(e, "b.eq __rt_gc_frames_loop", "jz __rt_gc_frames_loop", "skip activations that own no PHP values");
    abi::emit_jump(e, "__rt_gc_frames_visit");
    e.label("__rt_gc_frames_guard");
    abi::emit_symbol_address(e, if e.target.arch == Arch::AArch64 { "x10" } else { "r11" }, "__rt_gc_guard_frame_owners");
    e.label("__rt_gc_frames_visit");
    op(e, "ldr x0, [x0, #16]", "mov rdi, QWORD PTR [rax + 16]", "pass the original cleanup frame or owner-slot address");
    op(e, "add x1, sp, #64", "lea rsi, [rsp + 64]", "pass this walk's private mode, result and slot-set context");
    op(e, "blr x10", "call r11", "enumerate initialized owners without retaining values or invoking PHP");
    abi::emit_jump(e, "__rt_gc_frames_loop");
    e.label("__rt_gc_frames_done");
    restore(e, 88);
    abi::emit_branch_if_int_result_zero(e, "__rt_gc_frames_return");
    if e.target.arch == Arch::X86_64 { op(e, "", "mov rdi, rax", "free only this traversal's native slot-identity buffer"); }
    e.bl_c("free");
    e.label("__rt_gc_frames_return");
    restore(e, 80);
    abi::emit_frame_restore(e, 144);
    abi::emit_return(e);
    for (name, offset) in [("guard", 24), ("operand", 0)] {
        e.label_global(&format!("__rt_gc_{name}_frame_owners"));
        if name == "guard" {
            op(e, "ldr x9, [x0, #8]", "mov r8, QWORD PTR [rdi + 8]", "classify compact native activations by their cleanup callback");
            abi::emit_symbol_address(e, if e.target.arch == Arch::AArch64 { "x10" } else { "r9" }, "__rt_error_handler_restore");
            op(e, "cmp x9, x10", "cmp r8, r9", "suspended error handlers retain registration owners beyond the compact prefix");
            op(e, "b.ne __rt_gc_guard_regular_owner", "jne __rt_gc_guard_regular_owner", "ordinary compact guards use their single payload slot");
            abi::emit_jump(e, "__rt_gc_error_handler_frame_owners");
            e.label("__rt_gc_guard_regular_owner");
            abi::emit_symbol_address(e, if e.target.arch == Arch::AArch64 { "x10" } else { "r9" }, "__rt_user_filter_close_unwind");
            op(e, "cmp x9, x10", "cmp r8, r9", "exclude the typed non-heap descriptor guard from PHP ownership");
            op(e, "b.eq __rt_gc_guard_frame_done", "je __rt_gc_guard_frame_done", "never interpret a native descriptor as a pointer-shaped PHP value");
        }
        op(e, "mov x2, x1", "mov rdx, rsi", "forward the traversal context before materializing the slot descriptor");
        if offset != 0 { op(e, "add x0, x0, #24", "add rdi, 24", "the transferred guard owns its separate payload word"); }
        op(e, "mov x1, #0", "xor esi, esi", "guard slots own a direct managed value without raw-cell normalization");
        abi::emit_jump(e, "__rt_gc_frame_owner_slot");
        if name == "guard" { e.label("__rt_gc_guard_frame_done"); abi::emit_return(e); }
    }
}
