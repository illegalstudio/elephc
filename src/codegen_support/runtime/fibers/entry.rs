//! Purpose:
//! Emits the runtime trampoline that runs a Fiber body the first time it is switched into.
//! Owns callable invocation, return capture, termination marking, and transfer back to the caller.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::fibers`.
//!
//! Key details:
//! - The trampoline must keep Fiber object state, pending throws, try handlers, and transfer values balanced across switches.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::try_handlers::{TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE};
use crate::codegen_support::RuntimeFeatures;

use super::{
    FIBER_CALLABLE_OFFSET, FIBER_CALLABLE_WRAPPER_OFFSET, FIBER_CALLER_OFFSET,
    FIBER_DESCRIPTOR_ARGBOX_OFFSET, FIBER_PENDING_THROW_OFFSET, FIBER_START_ARG_COUNT_OFFSET,
    FIBER_START_ARGS_MAX, FIBER_START_ARGS_OFFSET, FIBER_STATE_OFFSET, FIBER_STATE_RUNNING,
    FIBER_STATE_TERMINATED, FIBER_TRANSFER_VALUE_OFFSET,
};

/// Emits the `__rt_fiber_entry` trampoline for the current target.
///
/// This function is the first code executed when a Fiber resumes on a fresh stack.
/// It installs a sentinel exception handler, marks the fiber Running, calls the
/// generated Fiber wrapper closure, captures the return value, marks the fiber
/// Terminated, and transfers control back to the caller.
///
/// On ARM64, x19 is used for the fiber pointer and is preserved across the closure
/// call. On x86_64, r12 is used analogously. The sentinel handler catches any
/// exception that escapes the user-visible try/catch chain; if no handler matches,
/// the exception is parked in `pending_throw` and the fiber transitions to Terminated
/// so the caller's resumption helper can re-raise it.
pub fn emit_fiber_entry(emitter: &mut Emitter, features: RuntimeFeatures) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter, features);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: fiber_entry ---");
    emitter.label_global("__rt_fiber_entry");

    // The initial switch frame already restores the creator's x28. Republishing
    // main _rt_ctx here would move a worker-local Fiber into a foreign arena.

    // -- establish a tiny frame on this fiber's fresh stack --
    emitter.instruction("sub sp, sp, #16");                                     // reserve a minimal scratch frame on the fiber stack
    emitter.instruction("str x29, [sp, #0]");                                   // store a zero-equivalent FP slot for diagnostic walkers
    emitter.instruction("mov x29, sp");                                         // anchor the frame pointer at the new bottom of the fiber stack

    // -- install a sentinel exception handler so any exception that escapes the
    //    closure's own try/catch chain unwinds back here instead of terminating
    //    the process via the standard "uncaught exception" path. --
    // Use x10 (caller-saved scratch) for register sources passed to
    // emit_store_reg_to_symbol — that helper uses x9 internally for the symbol
    // address, so source register x9 would self-clobber.
    emitter.instruction(&format!("sub sp, sp, #{}", TRY_HANDLER_SLOT_SIZE));    // reserve TRY_HANDLER_SLOT_SIZE bytes on the fiber stack for the boundary handler
    abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_handler_top", 0); // x10 = previous head of the handler chain (the fiber's saved value, typically NULL on a fresh fiber)
    emitter.instruction("str x10, [sp, #0]");                                   // handler.next = previous chain head
    emitter.instruction("str xzr, [sp, #8]");                                   // handler.activation_record = NULL → cleanup_frames unwinds the entire fiber call stack
    abi::emit_load_symbol_to_reg(emitter, "x10", "_rt_diag_suppression", 0); // x10 = current diagnostic-suppression depth
    emitter.instruction("str x10, [sp, #16]");                                  // handler.saved_diag_depth = current depth (matches user-emitted try frames)
    emitter.instruction("mov x10, sp");                                         // x10 = address of the handler base
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0); // push the boundary handler onto the global handler chain
    emitter.instruction(&format!("add x0, sp, #{}", TRY_HANDLER_JMP_BUF_OFFSET)); // x0 = jmp_buf address inside the handler (offset 24)
    emitter.bl_c("setjmp"); // setjmp returns 0 the first time; non-zero on a longjmp from __rt_throw_current
    emitter.instruction("cbnz x0, __rt_fiber_entry_escape");                    // a non-zero return means an exception unwound past every user handler

    // -- mark the fiber Running and load its captured callable --
    abi::emit_load_symbol_to_reg(emitter, "x19", "_fiber_current", 0); // x19 = pointer to the fiber object that just started
    emitter.instruction(&format!("mov x20, #{}", FIBER_STATE_RUNNING));         // FIBER_STATE_RUNNING constant
    emitter.instruction(&format!("str x20, [x19, #{}]", FIBER_STATE_OFFSET));   // state = Running

    // -- call through the generated Fiber wrapper --
    emitter.instruction(&format!(
        "ldr x10, [x19, #{}]",
        FIBER_CALLABLE_WRAPPER_OFFSET
    ));                                                                         // x10 = generated Fiber entry wrapper pointer
    emitter.instruction("cbnz x10, __rt_fiber_entry_call_wrapper");             // proceed when the constructor stored a wrapper
    abi::emit_symbol_address(emitter, "x0", "_fiber_msg_unsupported_callable"); // x0 = pointer to the static unsupported-callable message
    emitter.instruction("mov x1, #48");                                         // x1 = error message length in bytes
    emitter.instruction("bl __rt_fiber_throw_state_error");                     // raise FiberError through the boundary handler (no return)
    emitter.label("__rt_fiber_entry_call_wrapper");
    emitter.instruction("mov x0, x19");                                         // pass Fiber* to the wrapper so it can load start args and captures
    emitter.instruction("blr x10");                                             // call wrapper; x0 returns a boxed Mixed terminal value

    // -- store the return value into transfer_value (lo half) and mark Terminated --
    abi::emit_load_symbol_to_reg(emitter, "x19", "_fiber_current", 0); // reload x19 — registers were clobbered across the closure call
    emitter.instruction(&format!("str x0, [x19, #{}]", FIBER_TRANSFER_VALUE_OFFSET)); // transfer_value.lo = closure return value
    emitter.instruction(&format!(
        "str xzr, [x19, #{}]",
        FIBER_TRANSFER_VALUE_OFFSET + 8
    ));                                                                         // transfer_value.hi = 0 (raw integer/string default tag)
    emitter.instruction(&format!("mov x20, #{}", FIBER_STATE_TERMINATED));      // FIBER_STATE_TERMINATED constant
    emitter.instruction(&format!("str x20, [x19, #{}]", FIBER_STATE_OFFSET));   // state = Terminated
    emit_release_start_args(emitter, "__rt_fiber_entry_return");
    emit_release_descriptor_argbox(emitter, "__rt_fiber_entry_return", false);
    emitter.instruction("mov x0, #0");                                          // no activation record survives a normally terminated Fiber
    emitter.instruction("bl __rt_exception_cleanup_frames");                    // release lingering Fiber-frame cleanup owners before switching back
    abi::emit_load_symbol_to_reg(emitter, "x19", "_fiber_current", 0); // reload Fiber after frame cleanup clobbered caller-saved registers

    // PHP releases a Fiber callback and its captures as soon as the Fiber terminates. Besides
    // matching destructor timing, this breaks cycles where a capture points back to the Fiber.
    emitter.instruction(&format!("ldr x0, [x19, #{}]", FIBER_CALLABLE_OFFSET)); // x0 = callable descriptor whose body can no longer run
    emitter.instruction("bl __rt_callable_descriptor_release");                 // release the terminated Fiber callback and owned captures
    abi::emit_load_symbol_to_reg(emitter, "x19", "_fiber_current", 0); // reload Fiber after descriptor cleanup clobbered caller-saved registers
    emitter.instruction(&format!("str xzr, [x19, #{}]", FIBER_CALLABLE_OFFSET)); // prevent object destruction from releasing the descriptor twice

    // -- pop the boundary handler before yielding control back to the caller --
    // Use x10 — emit_store_reg_to_symbol uses x9 internally for the symbol address.
    emitter.instruction("ldr x10, [sp, #0]");                                   // x10 = handler.next (previous chain head)
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0); // restore the previous handler chain head

    // -- switch back to whoever resumed us (caller can never be NULL inside a fiber) --
    emitter.instruction(&format!("ldr x0, [x19, #{}]", FIBER_CALLER_OFFSET));   // x0 = caller fiber* (or NULL = main)
    emitter.instruction(&format!("str xzr, [x19, #{}]", FIBER_CALLER_OFFSET));  // terminal Fiber no longer retains its resumer through a raw runtime edge
    emitter.instruction("bl __rt_fiber_switch");                                // hand control back; this call never returns inside this fiber

    // -- defensive trap: a terminated fiber must never resume past the switch --
    emitter.label("__rt_fiber_entry_unreachable");
    emitter.instruction("brk #0xfffe");                                         // trap if the unreachable epilogue is ever entered

    // -- escape path: longjmp landed here because no user handler matched --
    emitter.label("__rt_fiber_entry_escape");
    abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_value", 0); // x10 = the Throwable that was unwound past every user catch
    abi::emit_load_symbol_to_reg(emitter, "x19", "_fiber_current", 0); // x19 = current fiber* (preserved through longjmp via the global)
    emitter.instruction(&format!("str x10, [x19, #{}]", FIBER_PENDING_THROW_OFFSET)); // park the escaped Throwable so the caller's helper can re-raise it
    emitter.instruction(&format!("str xzr, [x19, #{}]", FIBER_TRANSFER_VALUE_OFFSET)); // wipe transfer_value.lo so callers do not see stale data
    emitter.instruction(&format!(
        "str xzr, [x19, #{}]",
        FIBER_TRANSFER_VALUE_OFFSET + 8
    ));                                                                         // wipe transfer_value.hi as well
    emitter.instruction(&format!("mov x20, #{}", FIBER_STATE_TERMINATED));      // FIBER_STATE_TERMINATED constant — the fiber is done after an escape
    emitter.instruction(&format!("str x20, [x19, #{}]", FIBER_STATE_OFFSET));   // state = Terminated
    emit_release_start_args(emitter, "__rt_fiber_entry_escape");
    emit_clear_descriptor_argbox_after_invoker_escape(emitter, features.generator);
    emitter.instruction("mov x0, #0");                                          // the escaped callback can leave owned cleanup frames on the Fiber stack
    emitter.instruction("bl __rt_exception_cleanup_frames");                    // mirror normal termination before releasing captured callback state
    abi::emit_load_symbol_to_reg(emitter, "x19", "_fiber_current", 0); // reload Fiber after frame cleanup clobbered caller-saved registers
    emitter.instruction(&format!("ldr x0, [x19, #{}]", FIBER_CALLABLE_OFFSET)); // x0 = escaped Fiber callback descriptor that cannot run again
    emitter.instruction("bl __rt_callable_descriptor_release");                 // release captures after the unwound Fiber activation is gone
    abi::emit_load_symbol_to_reg(emitter, "x19", "_fiber_current", 0); // reload Fiber after descriptor cleanup clobbered caller-saved registers
    emitter.instruction(&format!("str xzr, [x19, #{}]", FIBER_CALLABLE_OFFSET)); // prevent object destruction from releasing the descriptor twice

    // -- pop the boundary handler from the chain (longjmp restored SP to setjmp time) --
    emit_republish_fiber_pending_throw(emitter);
    // Use x10 — emit_store_reg_to_symbol uses x9 internally for the symbol address.
    emitter.instruction("ldr x10, [sp, #0]");                                   // x10 = handler.next
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0); // restore the previous handler chain head
    emitter.instruction("ldr x10, [sp, #16]");                                  // x10 = saved diagnostic suppression depth
    abi::emit_store_reg_to_symbol(emitter, "x10", "_rt_diag_suppression", 0); // restore the diagnostic suppression depth captured at setjmp time

    // -- switch back to the caller; their helper sees Terminated + non-null pending_throw and re-raises --
    emitter.instruction(&format!("ldr x0, [x19, #{}]", FIBER_CALLER_OFFSET));   // x0 = caller fiber* (or NULL = main)
    emitter.instruction(&format!("str xzr, [x19, #{}]", FIBER_CALLER_OFFSET));  // exception termination is final as well, so break the resumer edge
    emitter.instruction("bl __rt_fiber_switch");                                // hand control back; the caller-side helper handles re-raising
    emitter.instruction("brk #0xfffe");                                         // defensive trap: a terminated fiber must never resume past the switch
}

/// Emits the x86_64-specific portion of the `__rt_fiber_entry` trampoline.
///
/// Identical in behavior to the ARM64 path but uses x86_64 registers and
/// conventions: r12 for the fiber pointer, r10 as scratch, SysV ABI for the
/// wrapper call, and ud2 for defensive traps. The sentinel setjmp/longjmp
/// handler and termination sequence are preserved.
fn emit_x86_64(emitter: &mut Emitter, features: RuntimeFeatures) {
    emitter.blank();
    emitter.comment("--- runtime: fiber_entry ---");
    emitter.label_global("__rt_fiber_entry");

    // The initial switch frame already restores the creator's r14; never replace
    // a worker context with the main context while entering its local coroutine.

    // -- establish a tiny frame on this fiber's fresh stack --
    emitter.instruction("push rbp");                                            // preserve a zero-equivalent caller frame pointer slot for walkers
    emitter.instruction("mov rbp, rsp");                                        // anchor the frame pointer at the new bottom of the fiber stack
    emitter.instruction("sub rsp, 8");                                          // align the fresh stack for SysV calls after the synthetic entry jump

    // -- install a sentinel exception handler for exceptions escaping the callback --
    emitter.instruction(&format!("sub rsp, {}", TRY_HANDLER_SLOT_SIZE));        // reserve TRY_HANDLER_SLOT_SIZE bytes for the boundary handler
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_handler_top", 0); // r10 = previous head of the handler chain
    emitter.instruction("mov QWORD PTR [rsp], r10");                            // handler.next = previous chain head
    emitter.instruction("mov QWORD PTR [rsp + 8], 0");                          // handler.activation_record = NULL
    abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_diag_suppression", 0); // r10 = current diagnostic-suppression depth
    emitter.instruction("mov QWORD PTR [rsp + 16], r10");                       // handler.saved_diag_depth = current depth
    emitter.instruction("mov r10, rsp");                                        // r10 = address of the handler base
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0); // push the boundary handler onto the global handler chain
    emitter.instruction(&format!("lea rdi, [rsp + {}]", TRY_HANDLER_JMP_BUF_OFFSET)); // rdi = jmp_buf address inside the handler
    emitter.bl_c("setjmp"); // setjmp returns 0 first, non-zero after longjmp
    emitter.instruction("test eax, eax");                                       // did control arrive through longjmp?
    emitter.instruction("jne __rt_fiber_entry_escape");                         // non-zero setjmp result means an exception escaped

    // -- mark the fiber Running and load its generated wrapper --
    abi::emit_load_symbol_to_reg(emitter, "r12", "_fiber_current", 0); // r12 = pointer to the fiber object that just started
    emitter.instruction(&format!(
        "mov QWORD PTR [r12 + {}], {}",
        FIBER_STATE_OFFSET, FIBER_STATE_RUNNING
    ));                                                                         // state = Running
    emitter.instruction(&format!(
        "mov r13, QWORD PTR [r12 + {}]",
        FIBER_CALLABLE_WRAPPER_OFFSET
    ));                                                                         // r13 = generated Fiber entry wrapper pointer
    emitter.instruction("test r13, r13");                                       // did construction provide a supported wrapper?
    emitter.instruction("jne __rt_fiber_entry_call_wrapper");                   // proceed when the constructor stored a wrapper
    abi::emit_symbol_address(emitter, "rdi", "_fiber_msg_unsupported_callable"); // rdi = pointer to the unsupported-callable message
    emitter.instruction("mov esi, 48");                                         // rsi = error message length in bytes
    emitter.instruction("call __rt_fiber_throw_state_error");                   // raise FiberError through the boundary handler

    // -- call through the generated Fiber wrapper --
    emitter.label("__rt_fiber_entry_call_wrapper");
    emitter.instruction("mov rdi, r12");                                        // pass Fiber* to the wrapper so it can load args and captures
    emitter.instruction("call r13");                                            // call wrapper; rax returns a boxed Mixed terminal value

    // -- store the return value into transfer_value and mark Terminated --
    abi::emit_load_symbol_to_reg(emitter, "r12", "_fiber_current", 0); // reload r12 because the callback may have clobbered caller-saved registers
    let store_transfer = format!("mov QWORD PTR [r12 + {}], rax", FIBER_TRANSFER_VALUE_OFFSET);
    emitter.instruction(&store_transfer);                                       // transfer_value.lo = closure return value
    let clear_transfer_high = format!("mov QWORD PTR [r12 + {}], 0", FIBER_TRANSFER_VALUE_OFFSET + 8);
    emitter.instruction(&clear_transfer_high);                                  // transfer_value.hi = 0
    let state_terminated = format!("mov QWORD PTR [r12 + {}], {}", FIBER_STATE_OFFSET, FIBER_STATE_TERMINATED);
    emitter.instruction(&state_terminated);                                     // state = Terminated
    emit_release_start_args(emitter, "__rt_fiber_entry_return_x");
    emit_release_descriptor_argbox(emitter, "__rt_fiber_entry_return_x", false);
    emitter.instruction("xor edi, edi");                                        // no activation record survives a normally terminated Fiber
    emitter.instruction("call __rt_exception_cleanup_frames");                  // release lingering Fiber-frame cleanup owners before switching back
    abi::emit_load_symbol_to_reg(emitter, "r12", "_fiber_current", 0); // reload Fiber after frame cleanup clobbered caller-saved registers

    // Release the callback at PHP's termination point, both for destructor ordering and to break
    // callable-capture cycles that the generic object graph cannot see through Fiber internals.
    emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", FIBER_CALLABLE_OFFSET)); // rax = callable descriptor whose body can no longer run
    emitter.instruction("call __rt_callable_descriptor_release");               // release the terminated Fiber callback and owned captures
    abi::emit_load_symbol_to_reg(emitter, "r12", "_fiber_current", 0); // reload Fiber after descriptor cleanup clobbered caller-saved registers
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], 0", FIBER_CALLABLE_OFFSET)); // prevent object destruction from releasing the descriptor twice

    // -- pop the boundary handler before yielding control back to the caller --
    emitter.instruction("mov r10, QWORD PTR [rsp]");                            // r10 = handler.next (previous chain head)
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0); // restore the previous handler chain head
    let caller_load_return = format!("mov rdi, QWORD PTR [r12 + {}]", FIBER_CALLER_OFFSET);
    emitter.instruction(&caller_load_return);                                   // rdi = caller fiber* (or NULL = main)
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], 0", FIBER_CALLER_OFFSET)); // terminal Fiber no longer retains its resumer through raw runtime storage
    emitter.instruction("call __rt_fiber_switch");                              // hand control back; this call never returns inside this fiber
    emitter.instruction("ud2");                                                 // defensive trap if the unreachable epilogue is ever entered

    // -- escape path: longjmp landed here because no user handler matched --
    emitter.label("__rt_fiber_entry_escape");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_value", 0); // r10 = Throwable unwound past every user catch
    abi::emit_load_symbol_to_reg(emitter, "r12", "_fiber_current", 0); // r12 = current fiber* preserved through the global
    let store_pending_throw = format!("mov QWORD PTR [r12 + {}], r10", FIBER_PENDING_THROW_OFFSET);
    emitter.instruction(&store_pending_throw);                                  // park the escaped Throwable for the caller
    let clear_transfer = format!("mov QWORD PTR [r12 + {}], 0", FIBER_TRANSFER_VALUE_OFFSET);
    emitter.instruction(&clear_transfer);                                       // wipe transfer_value.lo
    let clear_transfer_high = format!("mov QWORD PTR [r12 + {}], 0", FIBER_TRANSFER_VALUE_OFFSET + 8);
    emitter.instruction(&clear_transfer_high);                                  // wipe transfer_value.hi
    let state_terminated = format!("mov QWORD PTR [r12 + {}], {}", FIBER_STATE_OFFSET, FIBER_STATE_TERMINATED);
    emitter.instruction(&state_terminated);                                     // state = Terminated after an escape
    emit_release_start_args(emitter, "__rt_fiber_entry_escape_x");
    emit_clear_descriptor_argbox_after_invoker_escape(emitter, features.generator);
    emitter.instruction("xor edi, edi");                                        // the escaped callback can leave owned cleanup frames on the Fiber stack
    emitter.instruction("call __rt_exception_cleanup_frames");                  // mirror normal termination before releasing captured callback state
    abi::emit_load_symbol_to_reg(emitter, "r12", "_fiber_current", 0); // reload Fiber after frame cleanup clobbered caller-saved registers
    emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", FIBER_CALLABLE_OFFSET)); // rax = escaped Fiber callback descriptor that cannot run again
    emitter.instruction("call __rt_callable_descriptor_release");               // release captures after the unwound Fiber activation is gone
    abi::emit_load_symbol_to_reg(emitter, "r12", "_fiber_current", 0); // reload Fiber after descriptor cleanup clobbered caller-saved registers
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], 0", FIBER_CALLABLE_OFFSET)); // prevent object destruction from releasing the descriptor twice
    emitter.instruction("mov r10, QWORD PTR [rsp]");                            // r10 = handler.next
    emit_republish_fiber_pending_throw(emitter);
    emitter.instruction("mov r10, QWORD PTR [rsp]");                            // reload the enclosing handler after publishing the cleanup exception chain
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0); // restore the previous handler chain head
    emitter.instruction("mov r10, QWORD PTR [rsp + 16]");                       // r10 = saved diagnostic suppression depth
    abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0); // restore diagnostic suppression captured at setjmp time
    let caller_load = format!("mov rdi, QWORD PTR [r12 + {}]", FIBER_CALLER_OFFSET);
    emitter.instruction(&caller_load);                                          // rdi = caller fiber* (or NULL = main)
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], 0", FIBER_CALLER_OFFSET)); // exception termination is final as well, so break the resumer edge
    emitter.instruction("call __rt_fiber_switch");                              // hand control back; caller-side helper re-raises
    emitter.instruction("ud2");                                                 // defensive trap if a terminated fiber resumes past the switch
}

/// Releases a descriptor-invoker argument box left parked across a callback unwind.
fn emit_release_descriptor_argbox(emitter: &mut Emitter, label_prefix: &str, preserve_exception: bool) {
    let skip = format!("{}_descriptor_argbox_skip", label_prefix);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(emitter, "x19", "_fiber_current", 0);
            emitter.instruction(&format!("ldr x0, [x19, #{}]", FIBER_DESCRIPTOR_ARGBOX_OFFSET)); // load the dynamic-invoker argument container, if one escaped
            emitter.instruction(&format!("cbz x0, {}", skip));                  // direct wrappers have no descriptor argument container
            emitter.instruction(&format!("str xzr, [x19, #{}]", FIBER_DESCRIPTOR_ARGBOX_OFFSET)); // clear ownership before the nested release call
            if preserve_exception {
                abi::emit_unary_cleanup_preserving_exception(emitter, "__rt_decref_mixed", "x0");
            } else {
                abi::emit_call_label(emitter, "__rt_decref_mixed");
            }
            emitter.label(&skip);
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, "r12", "_fiber_current", 0);
            emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", FIBER_DESCRIPTOR_ARGBOX_OFFSET)); // load the dynamic-invoker argument container, if one escaped
            emitter.instruction("test rax, rax");                               // did this Fiber run through the descriptor wrapper?
            emitter.instruction(&format!("jz {}", skip));                       // direct wrappers have no descriptor argument container
            emitter.instruction(&format!("mov QWORD PTR [r12 + {}], 0", FIBER_DESCRIPTOR_ARGBOX_OFFSET)); // clear ownership before the nested release call
            if preserve_exception {
                abi::emit_unary_cleanup_preserving_exception(emitter, "__rt_decref_mixed", "rax");
            } else {
                abi::emit_call_label(emitter, "__rt_decref_mixed");
            }
            emitter.label(&skip);
        }
    }
}

/// Retires the caller-owned argument box after the ABI-v4 invoker rethrows.
///
/// The invoker borrows its incoming box; the Fiber wrapper must retire it on escape too.
/// A generator's same physical slot is its last key, so that payload stays with generator cleanup.
/// Destructor throws join the current pending chain instead of recursively escaping this cleanup.
fn emit_clear_descriptor_argbox_after_invoker_escape(emitter: &mut Emitter, has_generator: bool) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(emitter, "x19", "_fiber_current", 0);
            if has_generator {
                let skip = "__rt_fiber_entry_escape_argbox_done";
                emitter.instruction("ldr x11, [x19]");                          // x11 = current coroutine class ID
                abi::emit_load_symbol_to_reg(emitter, "x10", "_generator_class_id", 0);
                emitter.instruction("cmp x11, x10");                            // does this slot hold Generator::last_key instead?
                emitter.instruction(&format!("b.eq {skip}"));                   // leave generator-owned last_key for Generator cleanup
                emit_release_descriptor_argbox(emitter, "__rt_fiber_entry_escape_argbox", true);
                emitter.label(skip);
            } else {
                emit_release_descriptor_argbox(emitter, "__rt_fiber_entry_escape_argbox", true);
            }
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, "r12", "_fiber_current", 0);
            if has_generator {
                let skip = "__rt_fiber_entry_escape_argbox_done";
                emitter.instruction("mov r8, QWORD PTR [r12]");                 // r8 = current coroutine class ID
                abi::emit_load_symbol_to_reg(emitter, "r11", "_generator_class_id", 0);
                emitter.instruction("cmp r8, r11");                             // does this slot hold Generator::last_key instead?
                emitter.instruction(&format!("je {skip}"));                     // leave generator-owned last_key for Generator cleanup
                emit_release_descriptor_argbox(emitter, "__rt_fiber_entry_escape_argbox", true);
                emitter.label(skip);
            } else {
                emit_release_descriptor_argbox(emitter, "__rt_fiber_entry_escape_argbox", true);
            }
        }
    }
}

/// Moves the accumulated cleanup exception chain into the Fiber's caller-visible pending slot.
fn emit_republish_fiber_pending_throw(emitter: &mut Emitter) {
    let fiber = if emitter.target.arch == Arch::AArch64 { "x19" } else { "r12" };
    let pending = if emitter.target.arch == Arch::AArch64 { "x10" } else { "r10" };
    abi::emit_load_symbol_to_reg(emitter, fiber, "_fiber_current", 0);
    abi::emit_load_symbol_to_reg(emitter, pending, "_exc_value", 0);
    abi::emit_store_to_address(emitter, pending, fiber, FIBER_PENDING_THROW_OFFSET as usize);
}

/// Releases the start arguments retained at Fiber entry once the callback cannot resume again.
fn emit_release_start_args(emitter: &mut Emitter, label_prefix: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(emitter, "x19", "_fiber_current", 0);
            for index in 0..FIBER_START_ARGS_MAX {
                let skip = format!("{}_start_arg_{}_skip", label_prefix, index);
                emitter.instruction(&format!("ldr x9, [x19, #{}]", FIBER_START_ARG_COUNT_OFFSET)); // x9 = number of retained start arguments
                emitter.instruction(&format!("cmp x9, #{}", index + 1));        // does this Fiber own start argument index?
                emitter.instruction(&format!("b.lt {}", skip));                 // skip slots past the start-argument count
                emitter.instruction(&format!("ldr x0, [x19, #{}]", FIBER_START_ARGS_OFFSET + index * 8)); // x0 = retained boxed Mixed start argument
                emitter.instruction("bl __rt_decref_mixed");                    // release the Fiber's start-argument ownership
                abi::emit_load_symbol_to_reg(emitter, "x19", "_fiber_current", 0); // reload Fiber after nested cleanup
                emitter.instruction(&format!("str xzr, [x19, #{}]", FIBER_START_ARGS_OFFSET + index * 8)); // clear released start-argument slot
                emitter.label(&skip);
            }
            emitter.instruction(&format!("str xzr, [x19, #{}]", FIBER_START_ARG_COUNT_OFFSET)); // no retained start arguments remain after termination
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, "r12", "_fiber_current", 0);
            for index in 0..FIBER_START_ARGS_MAX {
                let skip = format!("{}_start_arg_{}_skip", label_prefix, index);
                emitter.instruction(&format!("mov r10, QWORD PTR [r12 + {}]", FIBER_START_ARG_COUNT_OFFSET)); // r10 = number of retained start arguments
                emitter.instruction(&format!("cmp r10, {}", index + 1));        // does this Fiber own start argument index?
                emitter.instruction(&format!("jl {}", skip));                   // skip slots past the start-argument count
                emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", FIBER_START_ARGS_OFFSET + index * 8)); // rax = retained boxed Mixed start argument
                emitter.instruction("call __rt_decref_mixed");                  // release the Fiber's start-argument ownership
                abi::emit_load_symbol_to_reg(emitter, "r12", "_fiber_current", 0); // reload Fiber after nested cleanup
                emitter.instruction(&format!("mov QWORD PTR [r12 + {}], 0", FIBER_START_ARGS_OFFSET + index * 8)); // clear released start-argument slot
                emitter.label(&skip);
            }
            emitter.instruction(&format!("mov QWORD PTR [r12 + {}], 0", FIBER_START_ARG_COUNT_OFFSET)); // no retained start arguments remain after termination
        }
    }
}
