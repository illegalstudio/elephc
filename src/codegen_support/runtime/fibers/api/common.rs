//! Purpose:
//! Emits the `__rt_mixed_from_value` runtime helper assembly for common.
//! Keeps emitted runtime labels and generated code call sites aligned across supported targets.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//!
//! Key details:
//! - Runtime labels, registers, and data symbols here are ABI shared with generated assembly call sites.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_mixed_from_value` call sequence to allocate a boxed `Mixed` null cell.
///
/// For both targets the sequence sets x0/rax to tag 8 (PHP null) and clears the low/high
/// payload words before branching to the runtime allocator. The result is returned in the
/// ABI's normal scalar return location (x0 for ARM64, rax for x86_64).
pub(super) fn emit_box_null_mixed(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #8");                                  // runtime tag 8 = PHP null
            emitter.instruction("mov x1, #0");                                  // null has no low payload word
            emitter.instruction("mov x2, #0");                                  // null has no high payload word
            emitter.instruction("bl __rt_mixed_from_value");                    // allocate a boxed Mixed null cell for the PHP-visible result
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, 8");                                  // runtime tag 8 = PHP null
            emitter.instruction("xor edi, edi");                                // null has no low payload word
            emitter.instruction("xor esi, esi");                                // null has no high payload word
            emitter.instruction("call __rt_mixed_from_value");                  // allocate a boxed Mixed null cell for the PHP-visible result
        }
    }
}

/// Emits the `--instrument` unpark hook: puts this coroutine's activations back
/// on the profiler's stack before control leaves without returning.
///
/// `__rt_fiber_suspend` has three such exits, and every one of them reaches PHP
/// handlers: `Fiber::suspend()` outside a fiber and a live `unserialize()` both
/// raise `FiberError` before the stack switch, and a pending
/// `Fiber::throw()`/`Generator::throw()` raises after it. The hook at the
/// suspension site brackets the CALL, so its second half is skipped on all
/// three, and the activation stayed parked while its own `catch` ran — the
/// handler's work charged to whatever frame was below it, and the function's own
/// exit later finding no frame and closing nothing. Generators reach the same
/// helper through `__rt_gen_suspend`, so this covers `yield` too.
///
/// `coro_reg` names the running coroutine (`_fiber_current`), which is the key
/// the park recorded. A frame pointer would not do: read from the frame chain it
/// gives the PHP function for a direct `Fiber::suspend()` and gives
/// `__rt_gen_suspend`'s own frame for a `yield`, because that path is one level
/// deeper.
///
/// The slot is null unless `--with-monitoring` filled it, so a binary without
/// the capability pays one load and a not-taken branch — and only on a path that
/// is already raising. `skip` must be unique per call site, since these are
/// file-local labels in one emitted function.
pub(super) fn emit_instr_unpark_hook(emitter: &mut Emitter, skip: &str, coro_reg: &str) {
    let slot = emitter.target.extern_symbol("elephc_instr_unpark_fn");
    if emitter.target.arch == Arch::X86_64 {
        abi::emit_load_symbol_to_reg(emitter, "rax", &slot, 0);
        emitter.instruction("test rax, rax");                                   // is the exact profiler linked into this binary?
        emitter.instruction(&format!("jz {skip}"));                             // no capability: skip the hook entirely
        emitter.instruction(&format!("mov rdx, {coro_reg}"));                   // arg 2: the coroutine, before the loads below take rdi/rsi
        abi::emit_load_symbol_to_reg(emitter, "rdi", "_gc_allocs", 0);          // arg 0: allocations so far
        abi::emit_load_symbol_to_reg(emitter, "rsi", "_gc_frees", 0);           // arg 1: frees so far
        emitter.emit_native_bridge_call("rax", 3);                                   // call the Rust monitoring hook through the target native ABI
    } else {
        abi::emit_load_symbol_to_reg(emitter, "x9", &slot, 0);
        emitter.instruction(&format!("cbz x9, {skip}"));                        // no capability: skip the hook entirely
        // Both AArch64 symbol loads borrow x9, which is holding the slot.
        emitter.instruction("mov x11, x9");                                     // keep the hook address clear of the loads below
        emitter.instruction(&format!("mov x2, {coro_reg}"));                    // arg 2: the coroutine, before the loads below take x0/x1
        abi::emit_load_symbol_to_reg(emitter, "x0", "_gc_allocs", 0);           // arg 0: allocations so far
        abi::emit_load_symbol_to_reg(emitter, "x1", "_gc_frees", 0);            // arg 1: frees so far
        emitter.instruction("blr x11");                                         // put this coroutine's activations back before the raise
    }
    emitter.label(skip);
}
