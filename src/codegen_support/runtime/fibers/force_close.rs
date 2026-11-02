//! Purpose:
//! Closes suspended native coroutines through ordinary PHP finally and frame unwinding.
//!
//! Called from:
//! - Native Fiber/Generator destructor table entries and suspension guards.
//!
//! Key details:
//! - Class sentinel -3 is an internal unwind marker that matches no PHP catch clause.
//! - The entry boundary consumes only that marker; real cleanup exceptions propagate.
//! - A reserved coroutine word records force-close state without changing public layouts.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch, sentinels::x86_64_heap_kind_word};
use crate::codegen_support::runtime::arrays::gc_candidates::{load, op, restore, save};

/// Offset of a borrowed forced-unwind marker, zero outside a force-close operation.
pub(crate) const FORCE_CLOSE_OFFSET: i32 = 224;

/// Emits destruction, marker absorption and forbidden suspension errors on both ABIs.
pub(crate) fn emit(e: &mut Emitter) {
    close(e);
    absorb(e);
    error(e);
}

/// Executes the existing suspension point's exceptional continuation under protected cleanup.
fn close(e: &mut Emitter) {
    e.blank();
    e.label_global("__rt_coroutine_force_close");
    abi::emit_frame_prologue(e, 64);
    op(e, "str x0, [sp]", "mov QWORD PTR [rsp], rdi", "keep the borrowed coroutine alive under its destructor guard or GC pin");
    restore(e, 0);
    op(e, "ldr x9, [x0, #8]", "mov r10, QWORD PTR [rax + 8]", "not-started and completed coroutines have no suspended body to unwind");
    op(e, "cmp x9, #2", "cmp r10, 2", "force-close only a stable suspended coroutine");
    op(e, "b.ne __rt_coroutine_close_return", "jne __rt_coroutine_close_return", "leave empty or already completed coroutine bodies untouched");
    op(e, "mov x0, #8", "mov eax, 8", "allocate one internal object-shaped marker with no PHP properties");
    abi::emit_call_label(e, "__rt_heap_alloc");
    kind(e);
    op(e, "mov x9, #-3", "mov QWORD PTR [rax], -3", "the internal class sentinel is outside every PHP catch hierarchy");
    if e.target.arch == Arch::AArch64 { op(e, "str x9, [x0]", "", "stamp the uncatchable forced-unwind class identity"); }
    save(e, 8);
    op(e, "ldr x9, [sp]", "mov r10, QWORD PTR [rsp]", "publish a borrowed marker as this coroutine's force-close guard");
    op(e, "str x0, [x9, #224]", "mov QWORD PTR [r10 + 224], rax", "prevent yield or suspend from abandoning the cancellation unwind");
    op(e, "str xzr, [sp, #16]", "mov QWORD PTR [rsp + 16], 0", "initialize the independently owned real-cleanup exception output");
    abi::emit_symbol_address(e, abi::int_arg_reg_name(e.target, 0), "__rt_coroutine_close_resume");
    op(e, "ldr x1, [sp]", "mov rsi, QWORD PTR [rsp]", "resume the same suspended coroutine with its private marker");
    op(e, "add x2, sp, #16", "lea rdx, [rsp + 16]", "retain a real finally exception while remaining native teardown continues");
    abi::emit_call_label(e, "__rt_cleanup_invoke");
    op(e, "ldr x9, [sp]", "mov r10, QWORD PTR [rsp]", "restore ordinary coroutine state after the exceptional continuation completes");
    op(e, "str xzr, [x9, #224]", "mov QWORD PTR [r10 + 224], 0", "retire only this coroutine's force-close guard");
    restore(e, 8);
    abi::emit_call_label(e, "__rt_decref_any");
    restore(e, 16);
    abi::emit_branch_if_int_result_zero(e, "__rt_coroutine_close_return");
    abi::emit_store_reg_to_symbol(e, abi::int_result_reg(e), "_exc_value", 0);
    abi::emit_frame_restore(e, 64);
    abi::emit_jump(e, "__rt_throw_current");
    e.label("__rt_coroutine_close_return");
    abi::emit_frame_restore(e, 64);
    abi::emit_return(e);
    e.label_global("__rt_coroutine_close_resume");
    abi::emit_frame_prologue(e, 16);
    op(e, "ldr x1, [x0, #224]", "mov rsi, QWORD PTR [rdi + 224]", "the pending throw slot retains its own marker reference");
    abi::emit_call_label(e, "__rt_fiber_throw");
    abi::emit_call_label(e, "__rt_decref_any");
    abi::emit_frame_restore(e, 16);
    abi::emit_return(e);
}

/// Moves an escaping Throwable to the resumer, consuming only internal forced-unwind markers.
fn absorb(e: &mut Emitter) {
    e.label_global("__rt_coroutine_unwind_result");
    abi::emit_frame_prologue(e, 16);
    op(e, "mov x9, x0", "mov rax, rdi", "take the escaping exception owner through the native result convention");
    op(e, "ldr x10, [x0]", "cmp QWORD PTR [rax], -3", "an internal unwind marker must never become a PHP-visible exception");
    if e.target.arch == Arch::AArch64 { op(e, "cmn x10, #3", "", "recognize the private class sentinel without consulting PHP metadata"); }
    op(e, "b.ne __rt_coroutine_unwind_real", "jne __rt_coroutine_unwind_real", "transfer real finally or destructor exceptions unchanged");
    abi::emit_call_label(e, "__rt_decref_any");
    op(e, "mov x0, #0", "xor eax, eax", "normal forced termination has no pending PHP Throwable");
    e.label("__rt_coroutine_unwind_real");
    abi::emit_frame_restore(e, 16);
    abi::emit_return(e);
}

/// Emits the inline fast guard while suspension keeps current/value in x19/x20 or r12/r13.
pub(super) fn suspend_guard(e: &mut Emitter) {
    op(e, &format!("ldr x9, [x19, #{}]", FORCE_CLOSE_OFFSET), &format!("cmp QWORD PTR [r12 + {}], 0", FORCE_CLOSE_OFFSET), "force-close may run finally code but cannot suspend again");
    op(e, "cbz x9, __rt_fiber_suspend_close_ok", "je __rt_fiber_suspend_close_ok", "ordinary suspension keeps its existing ownership path");
    super::api::emit_force_close_unpark(e);
    op(e, "mov x0, x20", "mov rdi, r13", "the forbidden suspension still consumes its yielded Mixed owner");
    abi::emit_call_label(e, "__rt_coroutine_close_error");
    e.label("__rt_fiber_suspend_close_ok");
}

/// Constructs PHP's distinct Generator Error or FiberError after releasing a rejected yield value.
fn error(e: &mut Emitter) {
    e.label_global("__rt_coroutine_close_error");
    abi::emit_frame_prologue(e, 48);
    if e.target.arch == Arch::X86_64 { op(e, "", "mov rax, rdi", "release the consumed suspension argument through the native GC ABI"); }
    abi::emit_call_label(e, "__rt_decref_any");
    load(e, "x9", "r10", "_fiber_current");
    op(e, "ldr x9, [x9]", "mov r10, QWORD PTR [r10]", "choose PHP's forced-close diagnostic from the current native class");
    load(e, "x10", "r11", "_generator_class_id");
    op(e, "cmp x9, x10", "cmp r10, r11", "Generator yield and Fiber suspend have distinct Error subclasses");
    op(e, "b.ne __rt_coroutine_close_fiber_error", "jne __rt_coroutine_close_fiber_error", "use FiberError for a suspended Fiber's forbidden switch");
    let cases = [("_spl_error_class_id", "_generator_msg_force_close", 53), ("_fiber_error_class_id", "_fiber_msg_force_close", 38)];
    for (index, (class, message, length)) in cases.into_iter().enumerate() {
        if index == 1 { e.label("__rt_coroutine_close_fiber_error"); }
        load(e, "x0", "rax", class); save(e, 0);
        abi::emit_symbol_address(e, abi::int_result_reg(e), message); save(e, 8);
        op(e, &format!("mov x0, #{length}"), &format!("mov eax, {length}"), "retain the exact static PHP diagnostic length"); save(e, 16);
        if index == 0 { abi::emit_jump(e, "__rt_coroutine_close_error_alloc"); }
    }
    e.label("__rt_coroutine_close_error_alloc");
    op(e, "mov x0, #24", "mov eax, 24", "allocate the compact native Error class and message layout");
    abi::emit_call_label(e, "__rt_heap_alloc");
    kind(e);
    abi::emit_call_label(e, "__rt_object_handle_acquire");
    for (source, destination) in [(0, 0), (8, 8), (16, 16)] {
        op(e, &format!("ldr x9, [sp, #{source}]"), &format!("mov r10, QWORD PTR [rsp + {source}]"), "recover one owned compact-error field");
        op(e, &format!("str x9, [x0, #{destination}]"), &format!("mov QWORD PTR [rax + {destination}], r10"), "publish the native class or immutable message metadata");
    }
    abi::emit_store_reg_to_symbol(e, abi::int_result_reg(e), "_exc_value", 0);
    abi::emit_frame_restore(e, 48);
    abi::emit_jump(e, "__rt_throw_current");
}

/// Stamps canonical native object storage without fabricating PHP ownership metadata.
fn kind(e: &mut Emitter) {
    op(e, "mov x9, #4", &format!("mov r10, 0x{:x}", x86_64_heap_kind_word(4)), "use the canonical target-aware physical object representation");
    op(e, "str x9, [x0, #-8]", "mov QWORD PTR [rax - 8], r10", "publish managed object storage before exception handling observes it");
}
