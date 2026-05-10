//! Purpose:
//! Emits the runtime trampoline that runs a Fiber body the first time it is switched into.
//! Owns callable invocation, return capture, termination marking, and transfer back to the caller.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::fibers`.
//! - `crate::codegen::runtime::x86_minimal::emit_runtime_linux_x86_64_minimal()`.
//!
//! Key details:
//! - The trampoline must keep Fiber object state, pending throws, try handlers, and transfer values balanced across switches.

use crate::codegen::abi;
use crate::codegen::context::{TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE};
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

use super::{
    FIBER_CALLABLE_WRAPPER_OFFSET, FIBER_CALLER_OFFSET, FIBER_PENDING_THROW_OFFSET,
    FIBER_STATE_OFFSET, FIBER_STATE_RUNNING, FIBER_STATE_TERMINATED, FIBER_TRANSFER_VALUE_OFFSET,
};

/// __rt_fiber_entry: trampoline executed at the start of every fiber.
/// On entry the fiber's saved stack has just been restored by __rt_fiber_switch.
/// The active fiber is `_fiber_current`.
pub fn emit_fiber_entry(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: fiber_entry ---");
    emitter.label_global("__rt_fiber_entry");

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
    abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_handler_top", 0);        // x10 = previous head of the handler chain (the fiber's saved value, typically NULL on a fresh fiber)
    emitter.instruction("str x10, [sp, #0]");                                   // handler.next = previous chain head
    emitter.instruction("str xzr, [sp, #8]");                                   // handler.activation_record = NULL → cleanup_frames unwinds the entire fiber call stack
    abi::emit_load_symbol_to_reg(emitter, "x10", "_rt_diag_suppression", 0);    // x10 = current diagnostic-suppression depth
    emitter.instruction("str x10, [sp, #16]");                                  // handler.saved_diag_depth = current depth (matches user-emitted try frames)
    emitter.instruction("mov x10, sp");                                         // x10 = address of the handler base
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);       // push the boundary handler onto the global handler chain
    emitter.instruction(&format!("add x0, sp, #{}", TRY_HANDLER_JMP_BUF_OFFSET)); // x0 = jmp_buf address inside the handler (offset 24)
    emitter.bl_c("setjmp");                                                     // setjmp returns 0 the first time; non-zero on a longjmp from __rt_throw_current
    emitter.instruction("cbnz x0, __rt_fiber_entry_escape");                    // a non-zero return means an exception unwound past every user handler

    // -- mark the fiber Running and load its captured callable --
    abi::emit_load_symbol_to_reg(emitter, "x19", "_fiber_current", 0);          // x19 = pointer to the fiber object that just started
    emitter.instruction(&format!("mov x20, #{}", FIBER_STATE_RUNNING));         // FIBER_STATE_RUNNING constant
    emitter.instruction(&format!("str x20, [x19, #{}]", FIBER_STATE_OFFSET));   // state = Running

    // -- call through the generated Fiber wrapper --
    emitter.instruction(&format!("ldr x10, [x19, #{}]", FIBER_CALLABLE_WRAPPER_OFFSET)); // x10 = generated Fiber entry wrapper pointer
    emitter.instruction("cbnz x10, __rt_fiber_entry_call_wrapper");             // proceed when the constructor stored a wrapper
    abi::emit_symbol_address(emitter, "x0", "_fiber_msg_unsupported_callable"); // x0 = pointer to the static unsupported-callable message
    emitter.instruction("mov x1, #48");                                         // x1 = error message length in bytes
    emitter.instruction("bl __rt_fiber_throw_state_error");                     // raise FiberError through the boundary handler (no return)
    emitter.label("__rt_fiber_entry_call_wrapper");
    emitter.instruction("mov x0, x19");                                         // pass Fiber* to the wrapper so it can load start args and captures
    emitter.instruction("blr x10");                                             // call wrapper; x0 returns a boxed Mixed terminal value

    // -- store the return value into transfer_value (lo half) and mark Terminated --
    abi::emit_load_symbol_to_reg(emitter, "x19", "_fiber_current", 0);          // reload x19 — registers were clobbered across the closure call
    emitter.instruction(&format!("str x0, [x19, #{}]", FIBER_TRANSFER_VALUE_OFFSET)); // transfer_value.lo = closure return value
    emitter.instruction(&format!("str xzr, [x19, #{}]", FIBER_TRANSFER_VALUE_OFFSET + 8)); // transfer_value.hi = 0 (raw integer/string default tag)
    emitter.instruction(&format!("mov x20, #{}", FIBER_STATE_TERMINATED));      // FIBER_STATE_TERMINATED constant
    emitter.instruction(&format!("str x20, [x19, #{}]", FIBER_STATE_OFFSET));   // state = Terminated

    // -- pop the boundary handler before yielding control back to the caller --
    // Use x10 — emit_store_reg_to_symbol uses x9 internally for the symbol address.
    emitter.instruction("ldr x10, [sp, #0]");                                   // x10 = handler.next (previous chain head)
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);       // restore the previous handler chain head

    // -- switch back to whoever resumed us (caller can never be NULL inside a fiber) --
    emitter.instruction(&format!("ldr x0, [x19, #{}]", FIBER_CALLER_OFFSET));   // x0 = caller fiber* (or NULL = main)
    emitter.instruction("bl __rt_fiber_switch");                                // hand control back; this call never returns inside this fiber

    // -- defensive trap: a terminated fiber must never resume past the switch --
    emitter.label("__rt_fiber_entry_unreachable");
    emitter.instruction("brk #0xfffe");                                         // trap if the unreachable epilogue is ever entered

    // -- escape path: longjmp landed here because no user handler matched --
    emitter.label("__rt_fiber_entry_escape");
    abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_value", 0);              // x10 = the Throwable that was unwound past every user catch
    abi::emit_load_symbol_to_reg(emitter, "x19", "_fiber_current", 0);          // x19 = current fiber* (preserved through longjmp via the global)
    emitter.instruction(&format!("str x10, [x19, #{}]", FIBER_PENDING_THROW_OFFSET)); // park the escaped Throwable so the caller's helper can re-raise it
    emitter.instruction(&format!("str xzr, [x19, #{}]", FIBER_TRANSFER_VALUE_OFFSET)); // wipe transfer_value.lo so callers do not see stale data
    emitter.instruction(&format!("str xzr, [x19, #{}]", FIBER_TRANSFER_VALUE_OFFSET + 8)); // wipe transfer_value.hi as well
    emitter.instruction(&format!("mov x20, #{}", FIBER_STATE_TERMINATED));      // FIBER_STATE_TERMINATED constant — the fiber is done after an escape
    emitter.instruction(&format!("str x20, [x19, #{}]", FIBER_STATE_OFFSET));   // state = Terminated

    // -- pop the boundary handler from the chain (longjmp restored SP to setjmp time) --
    // Use x10 — emit_store_reg_to_symbol uses x9 internally for the symbol address.
    emitter.instruction("ldr x10, [sp, #0]");                                   // x10 = handler.next
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);       // restore the previous handler chain head
    emitter.instruction("ldr x10, [sp, #16]");                                  // x10 = saved diagnostic suppression depth
    abi::emit_store_reg_to_symbol(emitter, "x10", "_rt_diag_suppression", 0);   // restore the diagnostic suppression depth captured at setjmp time

    // -- switch back to the caller; their helper sees Terminated + non-null pending_throw and re-raises --
    emitter.instruction(&format!("ldr x0, [x19, #{}]", FIBER_CALLER_OFFSET));   // x0 = caller fiber* (or NULL = main)
    emitter.instruction("bl __rt_fiber_switch");                                // hand control back; the caller-side helper handles re-raising
    emitter.instruction("brk #0xfffe");                                         // defensive trap: a terminated fiber must never resume past the switch
}

fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fiber_entry ---");
    emitter.label_global("__rt_fiber_entry");

    // -- establish a tiny frame on this fiber's fresh stack --
    emitter.instruction("push rbp");                                            // preserve a zero-equivalent caller frame pointer slot for walkers
    emitter.instruction("mov rbp, rsp");                                        // anchor the frame pointer at the new bottom of the fiber stack
    emitter.instruction("sub rsp, 8");                                          // align the fresh stack for SysV calls after the synthetic entry jump

    // -- install a sentinel exception handler for exceptions escaping the callback --
    emitter.instruction(&format!("sub rsp, {}", TRY_HANDLER_SLOT_SIZE));        // reserve TRY_HANDLER_SLOT_SIZE bytes for the boundary handler
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_handler_top", 0);        // r10 = previous head of the handler chain
    emitter.instruction("mov QWORD PTR [rsp], r10");                            // handler.next = previous chain head
    emitter.instruction("mov QWORD PTR [rsp + 8], 0");                          // handler.activation_record = NULL
    abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_diag_suppression", 0);    // r10 = current diagnostic-suppression depth
    emitter.instruction("mov QWORD PTR [rsp + 16], r10");                       // handler.saved_diag_depth = current depth
    emitter.instruction("mov r10, rsp");                                        // r10 = address of the handler base
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);       // push the boundary handler onto the global handler chain
    emitter.instruction(&format!("lea rdi, [rsp + {}]", TRY_HANDLER_JMP_BUF_OFFSET)); // rdi = jmp_buf address inside the handler
    emitter.bl_c("setjmp");                                                     // setjmp returns 0 first, non-zero after longjmp
    emitter.instruction("test eax, eax");                                       // did control arrive through longjmp?
    emitter.instruction("jne __rt_fiber_entry_escape");                         // non-zero setjmp result means an exception escaped

    // -- mark the fiber Running and load its generated wrapper --
    abi::emit_load_symbol_to_reg(emitter, "r12", "_fiber_current", 0);          // r12 = pointer to the fiber object that just started
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], {}", FIBER_STATE_OFFSET, FIBER_STATE_RUNNING)); // state = Running
    emitter.instruction(&format!("mov r13, QWORD PTR [r12 + {}]", FIBER_CALLABLE_WRAPPER_OFFSET)); // r13 = generated Fiber entry wrapper pointer
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
    abi::emit_load_symbol_to_reg(emitter, "r12", "_fiber_current", 0);          // reload r12 because the callback may have clobbered caller-saved registers
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], rax", FIBER_TRANSFER_VALUE_OFFSET)); // transfer_value.lo = closure return value
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], 0", FIBER_TRANSFER_VALUE_OFFSET + 8)); // transfer_value.hi = 0
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], {}", FIBER_STATE_OFFSET, FIBER_STATE_TERMINATED)); // state = Terminated

    // -- pop the boundary handler before yielding control back to the caller --
    emitter.instruction("mov r10, QWORD PTR [rsp]");                            // r10 = handler.next (previous chain head)
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);       // restore the previous handler chain head
    emitter.instruction(&format!("mov rdi, QWORD PTR [r12 + {}]", FIBER_CALLER_OFFSET)); // rdi = caller fiber* (or NULL = main)
    emitter.instruction("call __rt_fiber_switch");                              // hand control back; this call never returns inside this fiber
    emitter.instruction("ud2");                                                 // defensive trap if the unreachable epilogue is ever entered

    // -- escape path: longjmp landed here because no user handler matched --
    emitter.label("__rt_fiber_entry_escape");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_value", 0);              // r10 = Throwable unwound past every user catch
    abi::emit_load_symbol_to_reg(emitter, "r12", "_fiber_current", 0);          // r12 = current fiber* preserved through the global
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], r10", FIBER_PENDING_THROW_OFFSET)); // park the escaped Throwable for the caller
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], 0", FIBER_TRANSFER_VALUE_OFFSET)); // wipe transfer_value.lo
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], 0", FIBER_TRANSFER_VALUE_OFFSET + 8)); // wipe transfer_value.hi
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], {}", FIBER_STATE_OFFSET, FIBER_STATE_TERMINATED)); // state = Terminated after an escape
    emitter.instruction("mov r10, QWORD PTR [rsp]");                            // r10 = handler.next
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);       // restore the previous handler chain head
    emitter.instruction("mov r10, QWORD PTR [rsp + 16]");                       // r10 = saved diagnostic suppression depth
    abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0);   // restore diagnostic suppression captured at setjmp time
    emitter.instruction(&format!("mov rdi, QWORD PTR [r12 + {}]", FIBER_CALLER_OFFSET)); // rdi = caller fiber* (or NULL = main)
    emitter.instruction("call __rt_fiber_switch");                              // hand control back; caller-side helper re-raises
    emitter.instruction("ud2");                                                 // defensive trap if a terminated fiber resumes past the switch
}
