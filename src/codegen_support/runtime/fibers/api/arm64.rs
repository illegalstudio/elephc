//! Purpose:
//! Emits the `__rt_fiber_throw_state_error`, `__rt_heap_alloc` runtime helper assembly for arm64.
//! Keeps emitted runtime labels and generated code call sites aligned across supported targets.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//!
//! Key details:
//! - Runtime labels, registers, and data symbols here are ABI shared with generated assembly call sites.

use elephc_memoir::coroutine_storage;

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;

use super::common::emit_box_null_mixed;
use super::super::switch::{fiber_initial_entry_offset, fiber_initial_stack_frame_bytes};
use super::super::{
    FIBER_CALLER_OFFSET, FIBER_DEFAULT_STACK_SIZE, FIBER_STATE_NOT_STARTED, FIBER_STATE_OFFSET, FIBER_STATE_RUNNING, FIBER_STATE_SUSPENDED,
    FIBER_STATE_TERMINATED, };

/// __rt_fiber_throw_state_error: allocate a `FiberError`, set its message, and
/// raise it through the standard exception runtime. Never returns.
/// Input:  x0 = message bytes pointer, x1 = message length
pub(super) fn emit_throw_state_error(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fiber_throw_state_error ---");
    emitter.label_global("__rt_fiber_throw_state_error");

    emitter.instruction("sub sp, sp, #32");                                     // reserve frame plus saved-callee slots
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("stp x19, x20, [sp]");                                  // preserve caller's x19/x20 — we use them to cache the message across heap_alloc
    emitter.instruction("add x29, sp, #16");                                    // anchor the new frame pointer
    emitter.instruction("mov x19, x0");                                         // x19 = message bytes pointer (callee-saved across __rt_heap_alloc)
    emitter.instruction("mov x20, x1");                                         // x20 = message length (callee-saved across __rt_heap_alloc)

    emitter.instruction("mov x0, #24");                                         // FiberError object size (8 class_id + 16 message property)
    elephc_memoir::values::object_storage::emit_allocate_object(emitter);
    abi::emit_load_symbol_to_reg(emitter, "x9", "_fiber_error_class_id", 0);    // x9 = runtime class id of FiberError
    emitter.instruction("str x9, [x0]");                                        // store FiberError class id at the object header

    emitter.instruction("str x19, [x0, #8]");                                   // message property low half = bytes pointer (matches Exception's message slot layout)
    emitter.instruction("str x20, [x0, #16]");                                  // message property high half = byte length

    abi::emit_store_reg_to_symbol(emitter, "x0", "_exc_value", 0);              // _exc_value = the freshly built FiberError, matching the standard `throw` runtime contract
    emitter.instruction("bl __rt_throw_current");                               // unwind into the active try/catch chain (no return)

    emitter.instruction("brk #0xfffe");                                         // defensive trap: __rt_throw_current must not return here
}

/// __rt_fiber_construct: allocate and initialise a Fiber object.
/// Input:  x0 = callable descriptor pointer (may be NULL for diagnostics)
///         x1 = class_id assigned by the type checker for the Fiber class
///         x2 = wrapper entry that adapts Fiber Mixed traffic to the callable ABI
/// Output: x0 = pointer to the new Fiber object (16-byte heap header sits at -8)
pub(super) fn emit_construct(emitter: &mut Emitter) {
    let initial_frame_bytes = fiber_initial_stack_frame_bytes(emitter.target.arch);
    let initial_entry_offset = fiber_initial_entry_offset(emitter.target.arch);

    emitter.blank();
    emitter.comment("--- runtime: fiber_construct ---");
    emitter.label_global("__rt_fiber_construct");

    // -- frame: keep callable/class/wrapper across the heap calls --
    emitter.instruction("sub sp, sp, #64");                                     // reserve scratch frame plus saved callee regs
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("stp x19, x20, [sp]");                                  // preserve callee-saved registers used as argument cache
    emitter.instruction("stp x21, x22, [sp, #16]");                             // preserve callee-saved registers used for object and wrapper pointers
    emitter.instruction("add x29, sp, #48");                                    // anchor the new frame pointer
    emitter.instruction("mov x19, x0");                                         // x19 = callable (preserved across heap_alloc)
    emitter.instruction("mov x20, x1");                                         // x20 = class_id (preserved across heap_alloc)
    emitter.instruction("mov x22, x2");                                         // x22 = optional Fiber entry wrapper pointer

    elephc_memoir::coroutine_storage::emit_payload_and_stack(
        emitter, FIBER_DEFAULT_STACK_SIZE, initial_frame_bytes, initial_entry_offset,
    );

    // -- finish: state = NotStarted and return the new Fiber pointer --
    emitter.instruction(&format!("mov x9, #{}", FIBER_STATE_NOT_STARTED));      // FIBER_STATE_NOT_STARTED constant
    emitter.instruction(&format!("str x9, [x21, #{}]", FIBER_STATE_OFFSET));    // state = NotStarted
    emitter.instruction("mov x0, x21");                                         // return the freshly built Fiber pointer

    // -- tear down the scratch frame and return --
    emitter.instruction("ldp x21, x22, [sp, #16]");                             // restore caller's x21/x22
    emitter.instruction("ldp x19, x20, [sp]");                                  // restore caller's x19/x20
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore caller's frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the scratch frame
    emitter.instruction("ret");                                                 // hand the new Fiber object back to the constructor caller

    emitter.label("__rt_fiber_construct_stack_failed");
    abi::emit_symbol_address(emitter, "x0", "_fiber_msg_stack_alloc_failed");   // x0 = pointer to the static stack-allocation failure message
    emitter.instruction("mov x1, #27");                                         // x1 = error message length in bytes
    emitter.instruction("bl __rt_fiber_throw_state_error");                     // raise FiberError instead of dereferencing a NULL stack top
    emitter.instruction("brk #0xfffe");                                         // defensive trap: the throw helper must not return
}

/// __rt_fiber_start: switch into a fiber for the first time.
/// Input:  x0 = fiber*
/// Output: x0 = the value the fiber yielded (via Fiber::suspend) or returned.
pub(super) fn emit_start(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fiber_start ---");
    emitter.label_global("__rt_fiber_start");

    // -- prologue: keep the fiber pointer in x19 across the cooperative switch --
    emitter.instruction("sub sp, sp, #32");                                     // reserve a frame plus a saved-x19 slot
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("str x19, [sp]");                                       // preserve caller's x19 — we are about to repurpose it
    emitter.instruction("add x29, sp, #16");                                    // anchor the new frame pointer
    emitter.instruction("mov x19, x0");                                         // x19 = fiber object pointer (callee-saved across __rt_fiber_switch)

    // -- guard: start() requires state == NotStarted; otherwise raise FiberError --
    emitter.instruction(&format!("ldr x9, [x19, #{}]", FIBER_STATE_OFFSET));    // x9 = receiver fiber state
    emitter.instruction(&format!("cmp x9, #{}", FIBER_STATE_NOT_STARTED));      // is the fiber still in the NotStarted state?
    emitter.instruction("b.eq __rt_fiber_start_state_ok");                      // proceed when the fiber has not been started yet
    abi::emit_symbol_address(emitter, "x0", "_fiber_msg_already_started");      // x0 = pointer to the static error message
    emitter.instruction("mov x1, #50");                                         // x1 = error message length in bytes
    emitter.instruction("bl __rt_fiber_throw_state_error");                     // raise FiberError; this call does not return
    emitter.label("__rt_fiber_start_state_ok");

    // -- record the resumer (current execution context) as the fiber's caller --
    abi::emit_load_symbol_to_reg(emitter, "x9", "_fiber_current", 0);           // x9 = whoever is running right now (NULL means main thread)
    emitter.instruction(&format!("str x9, [x19, #{}]", FIBER_CALLER_OFFSET));   // fiber->caller = current execution context

    // -- switch into the fiber; control returns when it suspends or terminates --
    emitter.instruction("mov x0, x19");                                         // pass fiber* as the switch target
    emitter.instruction("bl __rt_fiber_switch");                                // cooperative context switch into the fiber

    // -- check for an escaped exception parked by the trampoline and re-raise it on the caller's stack --
    emitter.instruction(&format!("ldr x9, [x19, #{}]", FIBER_STATE_OFFSET));    // x9 = current fiber state
    emitter.instruction(&format!("cmp x9, #{}", FIBER_STATE_TERMINATED));       // is the fiber terminated?
    emitter.instruction("b.ne __rt_fiber_start_no_escape");                     // skip the re-raise path when the fiber is still alive
    coroutine_storage::emit_take_pending_throw(emitter, "x19", "x10", "__rt_fiber_start_no_escape");
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_value", 0);             // _exc_value = the escaped Throwable, ready for __rt_throw_current
    emitter.instruction("bl __rt_throw_current");                               // re-raise on the caller's stack chain (no return)
    emitter.instruction("brk #0xfffe");                                         // defensive trap if __rt_throw_current ever returns
    emitter.label("__rt_fiber_start_no_escape");

    // -- harvest a suspend value, or PHP null when the fiber terminated cleanly --
    emitter.instruction(&format!("ldr x9, [x19, #{}]", FIBER_STATE_OFFSET));    // x9 = current fiber state after control returned
    emitter.instruction(&format!("cmp x9, #{}", FIBER_STATE_TERMINATED));       // did the fiber finish instead of suspending?
    emitter.instruction("b.ne __rt_fiber_start_return_yield");                  // suspended fibers return their yielded transfer value
    emit_box_null_mixed(emitter);
    emitter.instruction("b __rt_fiber_start_return_ready");                     // skip the yielded-value load after boxing PHP null
    emitter.label("__rt_fiber_start_return_yield");
    coroutine_storage::emit_take_transfer(emitter, "x19", "x0");
    emitter.label("__rt_fiber_start_return_ready");

    // -- epilogue --
    emitter.instruction("ldr x19, [sp]");                                       // restore caller's x19
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the harvested value to the caller
}

/// __rt_fiber_resume: deliver a value into a suspended fiber and let it run.
/// Input:  x0 = fiber*, x1 = value to deliver to the suspended `Fiber::suspend()` call
/// Output: x0 = the value the fiber yielded next (via suspend/return)
pub(super) fn emit_resume(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fiber_resume ---");
    emitter.label_global("__rt_fiber_resume");

    emitter.instruction("sub sp, sp, #32");                                     // reserve frame plus saved-x19 slot
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("str x19, [sp]");                                       // preserve caller's x19 across the switch
    emitter.instruction("add x29, sp, #16");                                    // anchor the new frame pointer
    emitter.instruction("mov x19, x0");                                         // x19 = fiber* (callee-saved across the switch)

    // -- guard: resume() requires state == Suspended; otherwise raise FiberError.
    //    Hold the resume value in x20 across the helper because x1 is the second
    //    argument register, which the throw helper would clobber.
    emitter.instruction("stp x20, x21, [sp, #-16]!");                           // preserve caller's x20/x21 — both are callee-saved registers we are about to repurpose
    emitter.instruction("mov x20, x1");                                         // x20 = $value to deliver, parked across the state check
    emitter.instruction(&format!("ldr x9, [x19, #{}]", FIBER_STATE_OFFSET));    // x9 = receiver fiber state
    emitter.instruction(&format!("cmp x9, #{}", FIBER_STATE_SUSPENDED));        // is the fiber currently paused at a Fiber::suspend() call?
    emitter.instruction("b.eq __rt_fiber_resume_state_ok");                     // proceed only when the fiber is suspended
    emitter.instruction("mov x0, x20");                                         // retire the transferred input when state validation rejects the resume
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    abi::emit_symbol_address(emitter, "x0", "_fiber_msg_not_suspended");        // x0 = pointer to the static error message
    emitter.instruction("mov x1, #43");                                         // x1 = error message length in bytes
    emitter.instruction("bl __rt_fiber_throw_state_error");                     // raise FiberError; this call does not return
    emitter.label("__rt_fiber_resume_state_ok");
    emitter.instruction("mov x1, x20");                                         // restore the resume value into the second argument register
    emitter.instruction("ldp x20, x21, [sp], #16");                             // restore caller's x20/x21 now that the state check is done

    // -- deliver the new value into the fiber's transfer slot --
    coroutine_storage::emit_publish_transfer(emitter, "x19", "x1");

    // -- record the resumer as the fiber's caller, then switch in --
    abi::emit_load_symbol_to_reg(emitter, "x9", "_fiber_current", 0);           // x9 = current execution context to remember as the caller
    emitter.instruction(&format!("str x9, [x19, #{}]", FIBER_CALLER_OFFSET));   // fiber->caller = current context (so suspend knows who to yield back to)
    emitter.instruction("mov x0, x19");                                         // pass fiber* as the switch target
    emitter.instruction("bl __rt_fiber_switch");                                // cooperative context switch into the fiber

    // -- check for an escaped exception parked by the trampoline and re-raise it on the caller's stack --
    emitter.instruction(&format!("ldr x9, [x19, #{}]", FIBER_STATE_OFFSET));    // x9 = current fiber state
    emitter.instruction(&format!("cmp x9, #{}", FIBER_STATE_TERMINATED));       // is the fiber terminated?
    emitter.instruction("b.ne __rt_fiber_resume_no_escape");                    // skip the re-raise path when the fiber is still alive
    coroutine_storage::emit_take_pending_throw(emitter, "x19", "x10", "__rt_fiber_resume_no_escape");
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_value", 0);             // _exc_value = the escaped Throwable, ready for __rt_throw_current
    emitter.instruction("bl __rt_throw_current");                               // re-raise on the caller's stack chain (no return)
    emitter.instruction("brk #0xfffe");                                         // defensive trap if __rt_throw_current ever returns
    emitter.label("__rt_fiber_resume_no_escape");

    // -- harvest a suspend value, or PHP null when the fiber terminated cleanly --
    emitter.instruction(&format!("ldr x9, [x19, #{}]", FIBER_STATE_OFFSET));    // x9 = current fiber state after control returned
    emitter.instruction(&format!("cmp x9, #{}", FIBER_STATE_TERMINATED));       // did the fiber finish instead of suspending again?
    emitter.instruction("b.ne __rt_fiber_resume_return_yield");                 // suspended fibers return their next yielded transfer value
    emit_box_null_mixed(emitter);
    emitter.instruction("b __rt_fiber_resume_return_ready");                    // skip the yielded-value load after boxing PHP null
    emitter.label("__rt_fiber_resume_return_yield");
    coroutine_storage::emit_take_transfer(emitter, "x19", "x0");
    emitter.label("__rt_fiber_resume_return_ready");

    emitter.instruction("ldr x19, [sp]");                                       // restore caller's x19
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the harvested value
}

/// __rt_fiber_suspend: yield control from the running fiber back to its caller.
/// Input:  x0 = value to deliver to the resumer's `start()` / `resume()` call
/// Output: x0 = the value the next resumer passes back via `resume($v)`
/// Raises FiberError before mutation when an active `unserialize()` owns global parser state.
pub(super) fn emit_suspend(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fiber_suspend ---");
    emitter.label_global("__rt_fiber_suspend");

    emitter.instruction("sub sp, sp, #32");                                     // reserve frame plus saved-x19 slot
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("str x19, [sp]");                                       // preserve caller's x19 across the switch
    emitter.instruction("add x29, sp, #16");                                    // anchor the new frame pointer

    // -- guard: suspend() must be called from inside a fiber; otherwise raise FiberError --
    // Hold the yielded value in x20 across the helper because x0 is its first argument.
    emitter.instruction("stp x20, x21, [sp, #-16]!");                           // preserve caller's x20/x21 — both are callee-saved registers we are about to repurpose
    emitter.instruction("mov x20, x0");                                         // x20 = yielded value, parked across the state check
    abi::emit_load_symbol_to_reg(emitter, "x19", "_fiber_current", 0);          // x19 = currently running fiber* (NULL means called from main)
    emitter.instruction("cbnz x19, __rt_fiber_suspend_state_ok");               // proceed when we are actually executing inside a fiber
    // Raises without returning, so the suspension site's second hook never runs.
    // x19 is NULL on this path, which is exactly what the park recorded.
    super::common::emit_instr_unpark_hook(emitter, "__rt_fiber_suspend_outside_no_instr", "x19");
    emitter.instruction("mov x0, x20");                                         // an invalid suspension still consumes its independent input owner
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    abi::emit_symbol_address(emitter, "x0", "_fiber_msg_suspend_outside");      // x0 = pointer to the static error message
    emitter.instruction("mov x1, #33");                                         // x1 = error message length in bytes
    emitter.instruction("bl __rt_fiber_throw_state_error");                     // raise FiberError; this call does not return
    emitter.label("__rt_fiber_suspend_state_ok");
    super::super::force_close::suspend_guard(emitter);
    abi::emit_load_symbol_to_reg(emitter, "x9", "_unser_active", 0);          // x9 = active unserialize nesting count
    emitter.instruction("cbz x9, __rt_fiber_suspend_unserialize_ok");           // switching is safe only when no parser context is live
    // Also raises without returning, and also before the switch.
    super::common::emit_instr_unpark_hook(emitter, "__rt_fiber_suspend_unser_no_instr", "x19");
    emitter.instruction("mov x0, x20");                                         // release the rejected transfer before raising the parser-state error
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    abi::emit_symbol_address(emitter, "x0", "_fiber_msg_suspend_unserialize"); // x0 = stable FiberError message for the forbidden switch
    emitter.instruction("mov x1, #52");                                         // x1 = error message length in bytes
    emitter.instruction("bl __rt_fiber_throw_state_error");                     // raise before publishing a yielded value or Suspended state
    emitter.label("__rt_fiber_suspend_unserialize_ok");
    emitter.instruction("mov x0, x20");                                         // restore the yielded value into x0 for the suspend logic below
    emitter.instruction("ldp x20, x21, [sp], #16");                             // restore caller's x20/x21 now that the state check is done

    // -- store the value being yielded and mark the fiber Suspended --
    coroutine_storage::emit_publish_transfer(emitter, "x19", "x0");
    emitter.instruction(&format!("mov x9, #{}", FIBER_STATE_SUSPENDED));        // FIBER_STATE_SUSPENDED constant
    emitter.instruction(&format!("str x9, [x19, #{}]", FIBER_STATE_OFFSET));    // fiber->state = Suspended

    // -- switch back to the caller; control resumes here when someone calls resume() --
    emitter.instruction(&format!("ldr x0, [x19, #{}]", FIBER_CALLER_OFFSET));   // x0 = fiber->caller (whoever should regain control)
    emitter.instruction("bl __rt_fiber_switch");                                // hand control back to the caller's resume site

    // -- on resume, mark Running again --
    emitter.instruction(&format!("mov x9, #{}", FIBER_STATE_RUNNING));          // FIBER_STATE_RUNNING constant
    emitter.instruction(&format!("str x9, [x19, #{}]", FIBER_STATE_OFFSET));    // fiber->state = Running (we are executing again)

    // -- if a Throwable was scheduled by Fiber->throw($e), raise it inside this fiber --
    // Important: hold the Throwable in x10 (caller-saved scratch). Cannot use x9
    // because emit_store_reg_to_symbol uses x9 internally to materialise the
    // symbol address, which would clobber the value we want to write.
    coroutine_storage::emit_take_pending_throw(emitter, "x19", "x10", "__rt_fiber_suspend_no_throw");
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_value", 0);             // _exc_value = the Throwable to raise; matches normal `throw` runtime contract
    // We are back on this fiber's stack and about to unwind into ITS handlers,
    // so the activation has to be live again first — the resume half of the
    // suspension site's hook is past this point and will never run.
    super::common::emit_instr_unpark_hook(emitter, "__rt_fiber_suspend_pending_no_instr", "x19");
    emitter.instruction("bl __rt_throw_current");                               // unwind into the active try/catch on the fiber's stack (no return)

    emitter.label("__rt_fiber_suspend_no_throw");
    coroutine_storage::emit_take_transfer(emitter, "x19", "x0");

    emitter.label("__rt_fiber_suspend_done");
    emitter.instruction("ldr x19, [sp]");                                       // restore caller's x19
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the resumer-delivered value
}

/// __rt_fiber_throw: schedule an exception to be raised inside the fiber on
/// resume. The Throwable is parked in `pending_throw`; the resume side of
/// `__rt_fiber_suspend` checks it, clears it, and re-raises via
/// `__rt_throw_current` so the fiber's local try/catch frames see it.
/// The pending slot retains its own reference after state validation, leaving
/// the caller's argument ownership unchanged.
/// Input:  x0 = fiber*, x1 = Throwable*
/// Output: x0 = the value the fiber yields back (or 0 if it terminates without
///         further suspends; the exception itself unwinds inside the fiber).
pub(super) fn emit_throw(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fiber_throw ---");
    emitter.label_global("__rt_fiber_throw");

    emitter.instruction("sub sp, sp, #32");                                     // reserve frame plus saved-x19 slot
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("str x19, [sp]");                                       // preserve caller's x19 across the switch
    emitter.instruction("add x29, sp, #16");                                    // anchor the new frame pointer
    emitter.instruction("mov x19, x0");                                         // x19 = fiber* (callee-saved across the switch)

    // -- guard: throw() requires state == Suspended; otherwise raise FiberError.
    //    Park the Throwable in x20 across the helper because x1 is its argument register.
    emitter.instruction("stp x20, x21, [sp, #-16]!");                           // preserve caller's x20/x21 — both are callee-saved registers we are about to repurpose
    emitter.instruction("mov x20, x1");                                         // x20 = Throwable to deliver, parked across the state check
    emitter.instruction(&format!("ldr x9, [x19, #{}]", FIBER_STATE_OFFSET));    // x9 = receiver fiber state
    emitter.instruction(&format!("cmp x9, #{}", FIBER_STATE_SUSPENDED));        // is the fiber currently paused at a Fiber::suspend() call?
    emitter.instruction("b.eq __rt_fiber_throw_state_ok");                      // proceed only when the fiber is suspended
    abi::emit_symbol_address(emitter, "x0", "_fiber_msg_throw_not_suspended");  // x0 = pointer to the static error message
    emitter.instruction("mov x1, #43");                                         // x1 = error message length in bytes
    emitter.instruction("bl __rt_fiber_throw_state_error");                     // raise FiberError; this call does not return
    emitter.label("__rt_fiber_throw_state_ok");
    emitter.instruction("mov x0, x20");                                         // move the Throwable into the retain helper input register
    emitter.instruction("bl __rt_incref");                                      // retain an owned reference for the Fiber pending-throw slot
    emitter.instruction("mov x1, x0");                                          // restore the retained Throwable into the second argument register
    emitter.instruction("ldp x20, x21, [sp], #16");                             // restore caller's x20/x21 now that the state check is done

    coroutine_storage::emit_publish_pending_throw(emitter, "x19", "x1");
    abi::emit_load_symbol_to_reg(emitter, "x9", "_fiber_current", 0);           // x9 = current execution context
    emitter.instruction(&format!("str x9, [x19, #{}]", FIBER_CALLER_OFFSET));   // fiber->caller = current context

    emitter.instruction("mov x0, x19");                                         // pass fiber* as the switch target
    emitter.instruction("bl __rt_fiber_switch");                                // cooperative switch into the fiber

    // -- check for an escaped exception parked by the trampoline and re-raise it on the caller's stack --
    emitter.instruction(&format!("ldr x9, [x19, #{}]", FIBER_STATE_OFFSET));    // x9 = current fiber state
    emitter.instruction(&format!("cmp x9, #{}", FIBER_STATE_TERMINATED));       // is the fiber terminated?
    emitter.instruction("b.ne __rt_fiber_throw_no_escape");                     // skip the re-raise path when the fiber is still alive
    coroutine_storage::emit_take_pending_throw(emitter, "x19", "x10", "__rt_fiber_throw_no_escape");
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_value", 0);             // _exc_value = the escaped Throwable, ready for __rt_throw_current
    emitter.instruction("bl __rt_throw_current");                               // re-raise on the caller's stack chain (no return)
    emitter.instruction("brk #0xfffe");                                         // defensive trap if __rt_throw_current ever returns
    emitter.label("__rt_fiber_throw_no_escape");

    // -- harvest a suspend value, or PHP null when the fiber terminated cleanly --
    emitter.instruction(&format!("ldr x9, [x19, #{}]", FIBER_STATE_OFFSET));    // x9 = current fiber state after control returned
    emitter.instruction(&format!("cmp x9, #{}", FIBER_STATE_TERMINATED));       // did the fiber finish instead of suspending again?
    emitter.instruction("b.ne __rt_fiber_throw_return_yield");                  // suspended fibers return their next yielded transfer value
    emit_box_null_mixed(emitter);
    emitter.instruction("b __rt_fiber_throw_return_ready");                     // skip the yielded-value load after boxing PHP null
    emitter.label("__rt_fiber_throw_return_yield");
    coroutine_storage::emit_take_transfer(emitter, "x19", "x0");
    emitter.label("__rt_fiber_throw_return_ready");

    emitter.instruction("ldr x19, [sp]");                                       // restore caller's x19
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the value yielded by the fiber
}

/// __rt_fiber_get_current: return the currently executing Fiber or PHP null if
/// called from the main thread.
/// Output: x0 = boxed PHP null when called outside any fiber; otherwise a PHP
///         object wrapping the current Fiber instance (mixed tag 6, low word
///         holds the object pointer, high word is unused).
pub(super) fn emit_get_current(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fiber_get_current ---");
    emitter.label_global("__rt_fiber_get_current");
    abi::emit_load_symbol_to_reg(emitter, "x1", "_fiber_current", 0);           // x1 = pointer to the currently running fiber (NULL = main thread)
    emitter.instruction("cbz x1, __rt_fiber_get_current_null");                 // main-thread calls return boxed PHP null
    emitter.instruction("mov x0, #6");                                          // runtime tag 6 = object
    emitter.instruction("mov x2, #0");                                          // object payloads use only the low word
    emitter.instruction("b __rt_mixed_from_value");                             // tail-call the boxer so the caller's link register is preserved
    emitter.label("__rt_fiber_get_current_null");
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 = PHP null
    emitter.instruction("mov x1, #0");                                          // null has no low payload word
    emitter.instruction("mov x2, #0");                                          // null has no high payload word
    emitter.instruction("b __rt_mixed_from_value");                             // tail-call the boxer so the caller's link register is preserved
}

/// __rt_fiber_get_return: read the value a terminated fiber returned.
/// Input:  x0 = fiber*
/// Output: x0 = fiber->transfer_value.lo (set by the entry trampoline at termination)
pub(super) fn emit_get_return(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fiber_get_return ---");
    emitter.label_global("__rt_fiber_get_return");

    emitter.instruction("sub sp, sp, #32");                                     // reserve frame plus saved-x19 slot
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("str x19, [sp]");                                       // preserve caller's x19 — we use it to remember the receiver across the throw helper
    emitter.instruction("add x29, sp, #16");                                    // anchor the new frame pointer
    emitter.instruction("mov x19, x0");                                         // x19 = receiver fiber* (callee-saved across __rt_fiber_throw_state_error)

    emitter.instruction("cbz x19, __rt_fiber_get_return_null");                 // null fiber pointer is treated as a diagnostic null result

    // -- guard: getReturn() requires state == Terminated; otherwise raise FiberError --
    emitter.instruction(&format!("ldr x9, [x19, #{}]", FIBER_STATE_OFFSET));    // x9 = receiver fiber state
    emitter.instruction(&format!("cmp x9, #{}", FIBER_STATE_TERMINATED));       // has the fiber finished its callable?
    emitter.instruction("b.eq __rt_fiber_get_return_state_ok");                 // proceed only when the fiber has terminated
    abi::emit_symbol_address(emitter, "x0", "_fiber_msg_not_terminated");       // x0 = pointer to the static error message
    emitter.instruction("mov x1, #57");                                         // x1 = error message length in bytes
    emitter.instruction("bl __rt_fiber_throw_state_error");                     // raise FiberError; this call does not return

    emitter.label("__rt_fiber_get_return_state_ok");
    coroutine_storage::emit_borrow_transfer(emitter, "x19", "x0");
    emitter.instruction("bl __rt_incref");                                      // retain the return Mixed so the caller owns it independently of the Fiber
    emitter.instruction("b __rt_fiber_get_return_done");                        // skip the NULL-receiver fallback once the return value is loaded

    emitter.label("__rt_fiber_get_return_null");
    emitter.instruction("mov x0, #0");                                          // safe default when a NULL receiver bypassed type checking

    emitter.label("__rt_fiber_get_return_done");
    emitter.instruction("ldr x19, [sp]");                                       // restore caller's x19
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // hand the captured value back to the caller
}

/// Generic state predicate: `x0 = fiber*`, `x1 = expected state value` → `x0 = 1 or 0`.
pub(super) fn emit_state_getter(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fiber_state_eq ---");
    emitter.label_global("__rt_fiber_state_eq");
    emitter.instruction("cbz x0, __rt_fiber_state_eq_false");                   // a NULL fiber pointer never matches any state predicate
    emitter.instruction(&format!("ldr x9, [x0, #{}]", FIBER_STATE_OFFSET));     // x9 = current state stored on the fiber
    emitter.instruction("cmp x9, x1");                                          // compare current state to the requested predicate value
    emitter.instruction("cset x0, eq");                                         // materialise the boolean result (1 when equal, 0 otherwise)
    emitter.instruction("ret");                                                 // return the predicate result
    emitter.label("__rt_fiber_state_eq_false");
    emitter.instruction("mov x0, #0");                                          // NULL fiber pointer always evaluates to false
    emitter.instruction("ret");                                                 // return false to the caller
}
