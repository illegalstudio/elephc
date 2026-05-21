//! Purpose:
//! Emits the `__rt_fiber_throw_state_error`, `__rt_heap_alloc` runtime helper assembly for arm64.
//! Keeps emitted runtime labels and generated code call sites aligned across supported targets.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()`.
//!
//! Key details:
//! - Runtime labels, registers, and data symbols here are ABI shared with generated assembly call sites.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;

use super::common::emit_box_null_mixed;
use super::super::switch::{fiber_initial_entry_offset, fiber_initial_stack_frame_bytes};
use super::super::{
    FIBER_CALLABLE_OFFSET, FIBER_CALLABLE_WRAPPER_OFFSET, FIBER_CALLER_OFFSET,
    FIBER_DEFAULT_STACK_SIZE, FIBER_FLOAT_ARGS_MAX, FIBER_FLOAT_ARGS_OFFSET,
    FIBER_OBJECT_SIZE, FIBER_OWN_CALL_FRAME_OFFSET, FIBER_OWN_EXC_HEAD_OFFSET,
    FIBER_PENDING_THROW_OFFSET, FIBER_SAVED_SP_OFFSET, FIBER_STACK_BASE_OFFSET,
    FIBER_STACK_SIZE_OFFSET, FIBER_STACK_TOP_OFFSET, FIBER_START_ARGS_MAX,
    FIBER_START_ARGS_OFFSET, FIBER_STATE_NOT_STARTED, FIBER_STATE_OFFSET,
    FIBER_STATE_RUNNING, FIBER_STATE_SUSPENDED, FIBER_STATE_TERMINATED,
    FIBER_TRANSFER_VALUE_OFFSET, FIBER_USER_ARG_MAX_OFFSET,
};

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
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = freshly allocated payload pointer (heap header at -8)

    emitter.instruction("mov x9, #4");                                          // heap kind 4 = object instance
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the kind in the uniform heap header
    abi::emit_load_symbol_to_reg(emitter, "x9", "_fiber_error_class_id", 0);    // x9 = runtime class id of FiberError
    emitter.instruction("str x9, [x0]");                                        // store FiberError class id at the object header

    emitter.instruction("str x19, [x0, #8]");                                   // message property low half = bytes pointer (matches Exception's message slot layout)
    emitter.instruction("str x20, [x0, #16]");                                  // message property high half = byte length

    abi::emit_store_reg_to_symbol(emitter, "x0", "_exc_value", 0);              // _exc_value = the freshly built FiberError, matching the standard `throw` runtime contract
    emitter.instruction("bl __rt_throw_current");                               // unwind into the active try/catch chain (no return)

    emitter.instruction("brk #0xfffe");                                         // defensive trap: __rt_throw_current must not return here
}

/// __rt_fiber_construct: allocate and initialise a Fiber object.
/// Input:  x0 = callable (closure object pointer; may be NULL for diagnostics)
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

    // -- allocate the Fiber object payload --
    emitter.instruction(&format!("mov x0, #{}", FIBER_OBJECT_SIZE));            // size in bytes for the Fiber object payload
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = pointer to the object payload (header at x0-8)
    emitter.instruction("mov x21, x0");                                         // x21 = Fiber object pointer (kept until return)
    emitter.instruction("mov x9, #4");                                          // heap kind 4 = object instance
    emitter.instruction("str x9, [x21, #-8]");                                  // stamp the kind in the uniform heap header
    emitter.instruction("str x20, [x21]");                                      // store the runtime class_id at the object header

    // -- zero-initialise every Fiber field before populating the meaningful ones --
    emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_STATE_OFFSET));   // state placeholder (overwritten below with NotStarted)
    emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_STACK_BASE_OFFSET)); // stack_base placeholder (overwritten after stack alloc)
    emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_STACK_TOP_OFFSET)); // stack_top placeholder (overwritten after stack alloc)
    emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_STACK_SIZE_OFFSET)); // stack_size placeholder (overwritten after stack alloc)
    emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_SAVED_SP_OFFSET)); // saved_sp placeholder (overwritten after fake-frame setup)
    emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_CALLABLE_OFFSET)); // callable.lo placeholder (overwritten with x19 below)
    emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_CALLABLE_WRAPPER_OFFSET)); // callable wrapper placeholder (overwritten with x22 below)
    emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_CALLER_OFFSET));  // caller starts NULL (no resumer until start/resume)
    emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_TRANSFER_VALUE_OFFSET)); // transfer_value.lo cleared
    emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_TRANSFER_VALUE_OFFSET + 8)); // transfer_value.hi cleared
    emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_PENDING_THROW_OFFSET)); // pending_throw cleared
    emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_OWN_EXC_HEAD_OFFSET)); // own_exc_head cleared (no installed handlers yet)
    emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_OWN_CALL_FRAME_OFFSET)); // own_call_frame cleared (no activation records on the fresh fiber stack yet)
    for i in 0..FIBER_START_ARGS_MAX {
        emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_START_ARGS_OFFSET + i * 8)); // start_args[i] cleared
    }
    for i in 0..FIBER_FLOAT_ARGS_MAX {
        emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_FLOAT_ARGS_OFFSET + i * 8)); // float_args[i] cleared (raw bits, loaded back into d-regs by the trampoline)
    }
    // user_arg_max defaults to FIBER_START_ARGS_MAX so start() fills every
    // start_args slot when no captures are pre-loaded into the trailing ones.
    // Codegen of `new Fiber(function() use(...) {})` lowers it to the
    // closure's user-param count to keep the captures intact.
    emitter.instruction(&format!("mov x9, #{}", FIBER_START_ARGS_MAX));         // default user_arg_max = full slot count
    emitter.instruction(&format!("str x9, [x21, #{}]", FIBER_USER_ARG_MAX_OFFSET)); // user_arg_max stored on the freshly built fiber

    // -- record the captured callable --
    emitter.instruction(&format!("str x19, [x21, #{}]", FIBER_CALLABLE_OFFSET)); // callable.lo = closure pointer
    emitter.instruction(&format!("str x22, [x21, #{}]", FIBER_CALLABLE_WRAPPER_OFFSET)); // callable wrapper = Fiber entry ABI adapter

    // -- allocate the per-fiber stack via mmap; alloc returns base/top/total --
    emitter.instruction(&format!("mov x0, #{}", FIBER_DEFAULT_STACK_SIZE));     // request the default usable fiber stack size in bytes
    emitter.instruction("bl __rt_fiber_alloc_stack");                           // x0 = stack_base (mapping start), x1 = stack_top, x2 = total mapped length
    emitter.instruction("cbz x0, __rt_fiber_construct_stack_failed");           // abort construction if mmap failed instead of writing a fake frame at NULL
    emitter.instruction(&format!("str x0, [x21, #{}]", FIBER_STACK_BASE_OFFSET)); // stack_base = mmap mapping start (includes the guard page)
    emitter.instruction(&format!("str x1, [x21, #{}]", FIBER_STACK_TOP_OFFSET)); // stack_top = initial SP target (16-byte aligned high address)
    emitter.instruction(&format!("str x2, [x21, #{}]", FIBER_STACK_SIZE_OFFSET)); // stack_size = total mapped length, needed verbatim by munmap on free

    // -- carve out a fake initial frame at the very top of the stack --
    emitter.instruction(&format!("sub x10, x1, #{}", initial_frame_bytes));     // x10 = initial saved_sp (room for the switch save area)
    emitter.instruction(&format!("str x10, [x21, #{}]", FIBER_SAVED_SP_OFFSET)); // saved_sp points at the bottom of the fake frame

    // -- zero the fake frame so callee-saved registers come back as zero on first switch --
    emitter.instruction("mov x11, x10");                                        // x11 = cursor through the fake frame
    emitter.instruction(&format!("mov x12, #{}", initial_frame_bytes / 16));    // number of 16-byte chunks to zero
    emitter.label("__rt_fiber_construct_zero_loop");
    emitter.instruction("stp xzr, xzr, [x11], #16");                            // zero a 16-byte slice and advance the cursor
    emitter.instruction("subs x12, x12, #1");                                   // decrement the chunk counter
    emitter.instruction("b.ne __rt_fiber_construct_zero_loop");                 // continue until the entire frame is zero

    // -- install __rt_fiber_entry as the saved x30 so the first switch returns into it --
    abi::emit_symbol_address(emitter, "x9", "__rt_fiber_entry");                // x9 = absolute address of the entry trampoline
    emitter.instruction(&format!("str x9, [x10, #{}]", initial_entry_offset));  // saved x30 slot = entry trampoline address

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
    emitter.instruction(&format!("ldr x10, [x19, #{}]", FIBER_PENDING_THROW_OFFSET)); // x10 = parked Throwable (NULL when termination was clean)
    emitter.instruction("cbz x10, __rt_fiber_start_no_escape");                 // skip the re-raise path when no exception escaped
    emitter.instruction(&format!("str xzr, [x19, #{}]", FIBER_PENDING_THROW_OFFSET)); // clear pending_throw so subsequent inspections see the clean terminated state
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
    emitter.instruction(&format!("ldr x0, [x19, #{}]", FIBER_TRANSFER_VALUE_OFFSET)); // x0 = fiber->transfer_value.lo (suspend yield value)
    emitter.instruction(&format!("str xzr, [x19, #{}]", FIBER_TRANSFER_VALUE_OFFSET)); // clear transfer_value.lo because ownership moves to the caller
    emitter.instruction(&format!("str xzr, [x19, #{}]", FIBER_TRANSFER_VALUE_OFFSET + 8)); // clear transfer_value.hi to leave no stale yielded payload
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
    abi::emit_symbol_address(emitter, "x0", "_fiber_msg_not_suspended");        // x0 = pointer to the static error message
    emitter.instruction("mov x1, #43");                                         // x1 = error message length in bytes
    emitter.instruction("bl __rt_fiber_throw_state_error");                     // raise FiberError; this call does not return
    emitter.label("__rt_fiber_resume_state_ok");
    emitter.instruction("mov x1, x20");                                         // restore the resume value into the second argument register
    emitter.instruction("ldp x20, x21, [sp], #16");                             // restore caller's x20/x21 now that the state check is done

    // -- deliver the new value into the fiber's transfer slot --
    emitter.instruction(&format!("str x1, [x19, #{}]", FIBER_TRANSFER_VALUE_OFFSET)); // fiber->transfer_value.lo = $value passed to resume

    // -- record the resumer as the fiber's caller, then switch in --
    abi::emit_load_symbol_to_reg(emitter, "x9", "_fiber_current", 0);           // x9 = current execution context to remember as the caller
    emitter.instruction(&format!("str x9, [x19, #{}]", FIBER_CALLER_OFFSET));   // fiber->caller = current context (so suspend knows who to yield back to)
    emitter.instruction("mov x0, x19");                                         // pass fiber* as the switch target
    emitter.instruction("bl __rt_fiber_switch");                                // cooperative context switch into the fiber

    // -- check for an escaped exception parked by the trampoline and re-raise it on the caller's stack --
    emitter.instruction(&format!("ldr x9, [x19, #{}]", FIBER_STATE_OFFSET));    // x9 = current fiber state
    emitter.instruction(&format!("cmp x9, #{}", FIBER_STATE_TERMINATED));       // is the fiber terminated?
    emitter.instruction("b.ne __rt_fiber_resume_no_escape");                    // skip the re-raise path when the fiber is still alive
    emitter.instruction(&format!("ldr x10, [x19, #{}]", FIBER_PENDING_THROW_OFFSET)); // x10 = parked Throwable (NULL when termination was clean)
    emitter.instruction("cbz x10, __rt_fiber_resume_no_escape");                // skip the re-raise path when no exception escaped
    emitter.instruction(&format!("str xzr, [x19, #{}]", FIBER_PENDING_THROW_OFFSET)); // clear pending_throw so subsequent inspections see the clean terminated state
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
    emitter.instruction(&format!("ldr x0, [x19, #{}]", FIBER_TRANSFER_VALUE_OFFSET)); // x0 = fiber->transfer_value.lo (next yield value)
    emitter.instruction(&format!("str xzr, [x19, #{}]", FIBER_TRANSFER_VALUE_OFFSET)); // clear transfer_value.lo because ownership moves to the caller
    emitter.instruction(&format!("str xzr, [x19, #{}]", FIBER_TRANSFER_VALUE_OFFSET + 8)); // clear transfer_value.hi to leave no stale yielded payload
    emitter.label("__rt_fiber_resume_return_ready");

    emitter.instruction("ldr x19, [sp]");                                       // restore caller's x19
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the harvested value
}

/// __rt_fiber_suspend: yield control from the running fiber back to its caller.
/// Input:  x0 = value to deliver to the resumer's `start()` / `resume()` call
/// Output: x0 = the value the next resumer passes back via `resume($v)`
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
    abi::emit_symbol_address(emitter, "x0", "_fiber_msg_suspend_outside");      // x0 = pointer to the static error message
    emitter.instruction("mov x1, #33");                                         // x1 = error message length in bytes
    emitter.instruction("bl __rt_fiber_throw_state_error");                     // raise FiberError; this call does not return
    emitter.label("__rt_fiber_suspend_state_ok");
    emitter.instruction("mov x0, x20");                                         // restore the yielded value into x0 for the suspend logic below
    emitter.instruction("ldp x20, x21, [sp], #16");                             // restore caller's x20/x21 now that the state check is done

    // -- store the value being yielded and mark the fiber Suspended --
    emitter.instruction(&format!("str x0, [x19, #{}]", FIBER_TRANSFER_VALUE_OFFSET)); // fiber->transfer_value.lo = the yielded value
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
    emitter.instruction(&format!("ldr x10, [x19, #{}]", FIBER_PENDING_THROW_OFFSET)); // x10 = pending Throwable* (NULL if no throw was scheduled)
    emitter.instruction("cbz x10, __rt_fiber_suspend_no_throw");                // skip the raise path when no exception is pending
    emitter.instruction(&format!("str xzr, [x19, #{}]", FIBER_PENDING_THROW_OFFSET)); // clear pending_throw before re-raising so resume() can fire again
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_value", 0);             // _exc_value = the Throwable to raise; matches normal `throw` runtime contract
    emitter.instruction("bl __rt_throw_current");                               // unwind into the active try/catch on the fiber's stack (no return)

    emitter.label("__rt_fiber_suspend_no_throw");
    emitter.instruction(&format!("ldr x0, [x19, #{}]", FIBER_TRANSFER_VALUE_OFFSET)); // x0 = fiber->transfer_value.lo (the value the resumer passed)
    emitter.instruction(&format!("str xzr, [x19, #{}]", FIBER_TRANSFER_VALUE_OFFSET)); // clear transfer_value.lo because the fiber body now owns the resume value
    emitter.instruction(&format!("str xzr, [x19, #{}]", FIBER_TRANSFER_VALUE_OFFSET + 8)); // clear transfer_value.hi to leave no stale resume payload

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
    emitter.instruction("mov x1, x20");                                         // restore the Throwable into the second argument register
    emitter.instruction("ldp x20, x21, [sp], #16");                             // restore caller's x20/x21 now that the state check is done

    emitter.instruction(&format!("str x1, [x19, #{}]", FIBER_PENDING_THROW_OFFSET)); // fiber->pending_throw = Throwable* to raise on resume
    abi::emit_load_symbol_to_reg(emitter, "x9", "_fiber_current", 0);           // x9 = current execution context
    emitter.instruction(&format!("str x9, [x19, #{}]", FIBER_CALLER_OFFSET));   // fiber->caller = current context

    emitter.instruction("mov x0, x19");                                         // pass fiber* as the switch target
    emitter.instruction("bl __rt_fiber_switch");                                // cooperative switch into the fiber

    // -- check for an escaped exception parked by the trampoline and re-raise it on the caller's stack --
    emitter.instruction(&format!("ldr x9, [x19, #{}]", FIBER_STATE_OFFSET));    // x9 = current fiber state
    emitter.instruction(&format!("cmp x9, #{}", FIBER_STATE_TERMINATED));       // is the fiber terminated?
    emitter.instruction("b.ne __rt_fiber_throw_no_escape");                     // skip the re-raise path when the fiber is still alive
    emitter.instruction(&format!("ldr x10, [x19, #{}]", FIBER_PENDING_THROW_OFFSET)); // x10 = parked Throwable (NULL when termination was clean)
    emitter.instruction("cbz x10, __rt_fiber_throw_no_escape");                 // skip the re-raise path when no exception escaped
    emitter.instruction(&format!("str xzr, [x19, #{}]", FIBER_PENDING_THROW_OFFSET)); // clear pending_throw so subsequent inspections see the clean terminated state
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
    emitter.instruction(&format!("ldr x0, [x19, #{}]", FIBER_TRANSFER_VALUE_OFFSET)); // x0 = fiber->transfer_value.lo (next yield value)
    emitter.instruction(&format!("str xzr, [x19, #{}]", FIBER_TRANSFER_VALUE_OFFSET)); // clear transfer_value.lo because ownership moves to the caller
    emitter.instruction(&format!("str xzr, [x19, #{}]", FIBER_TRANSFER_VALUE_OFFSET + 8)); // clear transfer_value.hi to leave no stale yielded payload
    emitter.label("__rt_fiber_throw_return_ready");

    emitter.instruction("ldr x19, [sp]");                                       // restore caller's x19
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the value yielded by the fiber
}

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
    emitter.instruction(&format!("ldr x0, [x19, #{}]", FIBER_TRANSFER_VALUE_OFFSET)); // x0 = fiber->transfer_value.lo (the closure's return value)
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
