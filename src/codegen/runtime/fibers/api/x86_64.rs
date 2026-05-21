//! Purpose:
//! Emits the `__rt_fiber_throw_state_error`, `__rt_heap_alloc` runtime helper assembly for x86 64.
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
    FIBER_DEFAULT_STACK_SIZE, FIBER_OBJECT_SIZE, FIBER_PENDING_THROW_OFFSET,
    FIBER_SAVED_SP_OFFSET, FIBER_STACK_BASE_OFFSET, FIBER_STACK_SIZE_OFFSET,
    FIBER_STACK_TOP_OFFSET, FIBER_START_ARGS_MAX, FIBER_STATE_NOT_STARTED,
    FIBER_STATE_OFFSET, FIBER_STATE_RUNNING, FIBER_STATE_SUSPENDED,
    FIBER_STATE_TERMINATED, FIBER_TRANSFER_VALUE_OFFSET, FIBER_USER_ARG_MAX_OFFSET,
};

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

pub(super) fn emit_throw_state_error_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fiber_throw_state_error ---");
    emitter.label_global("__rt_fiber_throw_state_error");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while building FiberError
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the throw helper
    emitter.instruction("push r12");                                            // preserve the message pointer across heap allocation
    emitter.instruction("push r13");                                            // preserve the message length across heap allocation
    emitter.instruction("mov r12, rdi");                                        // r12 = message bytes pointer
    emitter.instruction("mov r13, rsi");                                        // r13 = message length

    emitter.instruction("mov rax, 24");                                         // FiberError object size (8 class_id + 16 message property)
    emitter.instruction("call __rt_heap_alloc");                                // rax = freshly allocated payload pointer
    emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 4)); // materialize the object heap kind word
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the kind in the uniform heap header
    abi::emit_load_symbol_to_reg(emitter, "r10", "_fiber_error_class_id", 0);   // r10 = runtime class id of FiberError
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store FiberError class id at the object header
    emitter.instruction("mov QWORD PTR [rax + 8], r12");                        // message property low half = bytes pointer
    emitter.instruction("mov QWORD PTR [rax + 16], r13");                       // message property high half = byte length
    abi::emit_store_reg_to_symbol(emitter, "rax", "_exc_value", 0);             // _exc_value = the freshly built FiberError
    emitter.instruction("call __rt_throw_current");                             // unwind into the active try/catch chain (no return)
    emitter.instruction("ud2");                                                 // defensive trap: __rt_throw_current must not return here
}

pub(super) fn emit_construct_x86_64(emitter: &mut Emitter) {
    let initial_frame_bytes = fiber_initial_stack_frame_bytes(emitter.target.arch);
    let initial_entry_offset = fiber_initial_entry_offset(emitter.target.arch);

    emitter.blank();
    emitter.comment("--- runtime: fiber_construct ---");
    emitter.label_global("__rt_fiber_construct");

    // -- frame: keep callable/class/wrapper/object across heap and stack calls --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer for the constructor helper
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base while nested helpers run
    emitter.instruction("push r12");                                            // preserve callable across heap allocation
    emitter.instruction("push r13");                                            // preserve class_id across heap allocation
    emitter.instruction("push r14");                                            // preserve the allocated Fiber object pointer
    emitter.instruction("push r15");                                            // preserve the generated Fiber wrapper pointer
    emitter.instruction("mov r12, rdi");                                        // r12 = callable pointer
    emitter.instruction("mov r13, rsi");                                        // r13 = Fiber class_id
    emitter.instruction("mov r15, rdx");                                        // r15 = generated Fiber wrapper pointer

    // -- allocate the Fiber object payload --
    emitter.instruction(&format!("mov rax, {}", FIBER_OBJECT_SIZE));            // size in bytes for the Fiber object payload
    emitter.instruction("call __rt_heap_alloc");                                // rax = pointer to the object payload
    emitter.instruction("mov r14, rax");                                        // r14 = Fiber object pointer kept until return
    emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 4)); // materialize the object heap kind word
    emitter.instruction("mov QWORD PTR [r14 - 8], r10");                        // stamp the allocation as an object instance
    emitter.instruction("mov QWORD PTR [r14], r13");                            // store the runtime class_id at the object header

    // -- zero-initialise every Fiber field before populating meaningful ones --
    emitter.instruction("lea r10, [r14 + 8]");                                  // r10 = first runtime-managed Fiber field after class_id
    emitter.instruction(&format!("mov r11, {}", (FIBER_OBJECT_SIZE - 8) / 8));  // r11 = number of qword fields to clear
    emitter.label("__rt_fiber_construct_zero_object_loop");
    emitter.instruction("mov QWORD PTR [r10], 0");                              // clear the current runtime-managed Fiber qword field
    emitter.instruction("add r10, 8");                                          // advance to the next qword field
    emitter.instruction("sub r11, 1");                                          // consume one field from the clear count
    emitter.instruction("jne __rt_fiber_construct_zero_object_loop");           // continue until every runtime-managed field is zeroed

    emitter.instruction(&format!("mov r10, {}", FIBER_START_ARGS_MAX));         // default user_arg_max = full slot count
    emitter.instruction(&format!("mov QWORD PTR [r14 + {}], r10", FIBER_USER_ARG_MAX_OFFSET)); // user_arg_max stored on the freshly built fiber
    emitter.instruction(&format!("mov QWORD PTR [r14 + {}], r12", FIBER_CALLABLE_OFFSET)); // callable.lo = closure pointer
    emitter.instruction(&format!("mov QWORD PTR [r14 + {}], r15", FIBER_CALLABLE_WRAPPER_OFFSET)); // callable wrapper = Fiber entry ABI adapter

    // -- allocate the per-fiber stack via mmap; alloc returns base/top/total --
    emitter.instruction(&format!("mov edi, {}", FIBER_DEFAULT_STACK_SIZE));     // request the default usable fiber stack size in bytes
    emitter.instruction("call __rt_fiber_alloc_stack");                         // rax = stack_base, rdx = stack_top, rcx = total mapped length
    emitter.instruction("test rax, rax");                                       // did mmap return a real stack mapping?
    emitter.instruction("jz __rt_fiber_construct_stack_failed");                // abort construction before writing a fake frame at NULL
    emitter.instruction(&format!("mov QWORD PTR [r14 + {}], rax", FIBER_STACK_BASE_OFFSET)); // stack_base = mmap mapping start
    emitter.instruction(&format!("mov QWORD PTR [r14 + {}], rdx", FIBER_STACK_TOP_OFFSET)); // stack_top = initial SP target
    emitter.instruction(&format!("mov QWORD PTR [r14 + {}], rcx", FIBER_STACK_SIZE_OFFSET)); // stack_size = total mapped length

    // -- carve out and zero a fake initial frame at the top of the stack --
    emitter.instruction(&format!("lea r10, [rdx - {}]", initial_frame_bytes));  // r10 = initial saved_sp for the switch restore path
    emitter.instruction(&format!("mov QWORD PTR [r14 + {}], r10", FIBER_SAVED_SP_OFFSET)); // saved_sp points at the fake switch frame
    emitter.instruction("mov r11, r10");                                        // r11 = cursor through the fake frame
    emitter.instruction(&format!("mov rcx, {}", initial_frame_bytes / 8));      // rcx = number of qwords to zero in the fake frame
    emitter.label("__rt_fiber_construct_zero_frame_loop");
    emitter.instruction("mov QWORD PTR [r11], 0");                              // zero one saved-register or saved-return slot
    emitter.instruction("add r11, 8");                                          // advance to the next fake-frame qword
    emitter.instruction("sub rcx, 1");                                          // consume one fake-frame qword
    emitter.instruction("jne __rt_fiber_construct_zero_frame_loop");            // continue until the fake frame is zeroed
    abi::emit_symbol_address(emitter, "r11", "__rt_fiber_entry");              // r11 = absolute address of the entry trampoline
    emitter.instruction(&format!("mov QWORD PTR [r10 + {}], r11", initial_entry_offset)); // saved return address = entry trampoline

    // -- finish: state = NotStarted and return the new Fiber pointer --
    emitter.instruction(&format!("mov QWORD PTR [r14 + {}], {}", FIBER_STATE_OFFSET, FIBER_STATE_NOT_STARTED)); // state = NotStarted
    emitter.instruction("mov rax, r14");                                        // return the freshly built Fiber pointer
    emitter.instruction("pop r15");                                             // restore caller's r15
    emitter.instruction("pop r14");                                             // restore caller's r14
    emitter.instruction("pop r13");                                             // restore caller's r13
    emitter.instruction("pop r12");                                             // restore caller's r12
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // hand the new Fiber object back to the constructor caller

    emitter.label("__rt_fiber_construct_stack_failed");
    abi::emit_symbol_address(emitter, "rdi", "_fiber_msg_stack_alloc_failed");  // rdi = pointer to the static stack-allocation failure message
    emitter.instruction("mov esi, 27");                                         // rsi = error message length in bytes
    emitter.instruction("call __rt_fiber_throw_state_error");                   // raise FiberError instead of dereferencing a NULL stack top
    emitter.instruction("ud2");                                                 // defensive trap: the throw helper must not return
}

pub(super) fn emit_start_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fiber_start ---");
    emitter.label_global("__rt_fiber_start");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while switching fibers
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the start helper
    emitter.instruction("push r12");                                            // preserve the receiver Fiber pointer across the cooperative switch
    emitter.instruction("sub rsp, 8");                                          // keep the SysV stack aligned after saving one callee-saved register
    emitter.instruction("mov r12, rdi");                                        // r12 = fiber object pointer
    emitter.instruction(&format!("mov r10, QWORD PTR [r12 + {}]", FIBER_STATE_OFFSET)); // r10 = receiver fiber state
    emitter.instruction(&format!("cmp r10, {}", FIBER_STATE_NOT_STARTED));      // is the fiber still in the NotStarted state?
    emitter.instruction("je __rt_fiber_start_state_ok");                        // proceed when the fiber has not been started yet
    abi::emit_symbol_address(emitter, "rdi", "_fiber_msg_already_started");     // rdi = pointer to the static error message
    emitter.instruction("mov esi, 50");                                         // rsi = error message length in bytes
    emitter.instruction("call __rt_fiber_throw_state_error");                   // raise FiberError; this call does not return
    emitter.label("__rt_fiber_start_state_ok");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_fiber_current", 0);          // r10 = whoever is running right now
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], r10", FIBER_CALLER_OFFSET)); // fiber->caller = current execution context
    emitter.instruction("mov rdi, r12");                                        // pass fiber* as the switch target
    emitter.instruction("call __rt_fiber_switch");                              // cooperative context switch into the fiber
    emit_check_escape_x86_64(emitter, "start");
    emitter.instruction(&format!("mov r10, QWORD PTR [r12 + {}]", FIBER_STATE_OFFSET)); // r10 = current fiber state after control returned
    emitter.instruction(&format!("cmp r10, {}", FIBER_STATE_TERMINATED));       // did the fiber finish instead of suspending?
    emitter.instruction("jne __rt_fiber_start_return_yield");                   // suspended fibers return their yielded transfer value
    emit_box_null_mixed(emitter);
    emitter.instruction("jmp __rt_fiber_start_return_ready");                   // skip the yielded-value load after boxing PHP null
    emitter.label("__rt_fiber_start_return_yield");
    emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", FIBER_TRANSFER_VALUE_OFFSET)); // rax = fiber->transfer_value.lo
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], 0", FIBER_TRANSFER_VALUE_OFFSET)); // clear transfer_value.lo because ownership moves to the caller
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], 0", FIBER_TRANSFER_VALUE_OFFSET + 8)); // clear transfer_value.hi to leave no stale yielded payload
    emitter.label("__rt_fiber_start_return_ready");
    emitter.instruction("add rsp, 8");                                          // drop the alignment pad
    emitter.instruction("pop r12");                                             // restore caller's r12
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the harvested value to the caller
}

pub(super) fn emit_resume_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fiber_resume ---");
    emitter.label_global("__rt_fiber_resume");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while switching fibers
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the resume helper
    emitter.instruction("push r12");                                            // preserve the receiver Fiber pointer across the cooperative switch
    emitter.instruction("push r13");                                            // preserve the resume value across state validation
    emitter.instruction("mov r12, rdi");                                        // r12 = fiber object pointer
    emitter.instruction("mov r13, rsi");                                        // r13 = boxed Mixed value to deliver
    emitter.instruction(&format!("mov r10, QWORD PTR [r12 + {}]", FIBER_STATE_OFFSET)); // r10 = receiver fiber state
    emitter.instruction(&format!("cmp r10, {}", FIBER_STATE_SUSPENDED));        // is the fiber currently paused at Fiber::suspend()?
    emitter.instruction("je __rt_fiber_resume_state_ok");                       // proceed only when the fiber is suspended
    abi::emit_symbol_address(emitter, "rdi", "_fiber_msg_not_suspended");       // rdi = pointer to the static error message
    emitter.instruction("mov esi, 43");                                         // rsi = error message length in bytes
    emitter.instruction("call __rt_fiber_throw_state_error");                   // raise FiberError; this call does not return
    emitter.label("__rt_fiber_resume_state_ok");
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], r13", FIBER_TRANSFER_VALUE_OFFSET)); // fiber->transfer_value.lo = resume value
    abi::emit_load_symbol_to_reg(emitter, "r10", "_fiber_current", 0);          // r10 = current execution context to remember as caller
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], r10", FIBER_CALLER_OFFSET)); // fiber->caller = current execution context
    emitter.instruction("mov rdi, r12");                                        // pass fiber* as the switch target
    emitter.instruction("call __rt_fiber_switch");                              // cooperative context switch into the fiber
    emit_check_escape_x86_64(emitter, "resume");
    emitter.instruction(&format!("mov r10, QWORD PTR [r12 + {}]", FIBER_STATE_OFFSET)); // r10 = current fiber state after control returned
    emitter.instruction(&format!("cmp r10, {}", FIBER_STATE_TERMINATED));       // did the fiber finish instead of suspending again?
    emitter.instruction("jne __rt_fiber_resume_return_yield");                  // suspended fibers return their next yielded transfer value
    emit_box_null_mixed(emitter);
    emitter.instruction("jmp __rt_fiber_resume_return_ready");                  // skip the yielded-value load after boxing PHP null
    emitter.label("__rt_fiber_resume_return_yield");
    emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", FIBER_TRANSFER_VALUE_OFFSET)); // rax = fiber->transfer_value.lo
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], 0", FIBER_TRANSFER_VALUE_OFFSET)); // clear transfer_value.lo because ownership moves to the caller
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], 0", FIBER_TRANSFER_VALUE_OFFSET + 8)); // clear transfer_value.hi to leave no stale yielded payload
    emitter.label("__rt_fiber_resume_return_ready");
    emitter.instruction("pop r13");                                             // restore caller's r13
    emitter.instruction("pop r12");                                             // restore caller's r12
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the harvested value to the caller
}

pub(super) fn emit_suspend_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fiber_suspend ---");
    emitter.label_global("__rt_fiber_suspend");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while yielding
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the suspend helper
    emitter.instruction("push r12");                                            // preserve the current Fiber pointer across the cooperative switch
    emitter.instruction("push r13");                                            // preserve the yielded value across state validation
    emitter.instruction("mov r13, rdi");                                        // r13 = boxed Mixed value being yielded
    abi::emit_load_symbol_to_reg(emitter, "r12", "_fiber_current", 0);          // r12 = currently running fiber* (NULL means main)
    emitter.instruction("test r12, r12");                                       // are we executing inside a Fiber?
    emitter.instruction("jne __rt_fiber_suspend_state_ok");                     // proceed when suspend() is called from a Fiber
    abi::emit_symbol_address(emitter, "rdi", "_fiber_msg_suspend_outside");     // rdi = pointer to the static error message
    emitter.instruction("mov esi, 33");                                         // rsi = error message length in bytes
    emitter.instruction("call __rt_fiber_throw_state_error");                   // raise FiberError; this call does not return
    emitter.label("__rt_fiber_suspend_state_ok");
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], r13", FIBER_TRANSFER_VALUE_OFFSET)); // fiber->transfer_value.lo = yielded value
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], {}", FIBER_STATE_OFFSET, FIBER_STATE_SUSPENDED)); // fiber->state = Suspended
    emitter.instruction(&format!("mov rdi, QWORD PTR [r12 + {}]", FIBER_CALLER_OFFSET)); // rdi = fiber->caller
    emitter.instruction("call __rt_fiber_switch");                              // hand control back to the caller's resume site
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], {}", FIBER_STATE_OFFSET, FIBER_STATE_RUNNING)); // fiber->state = Running
    emitter.instruction(&format!("mov r10, QWORD PTR [r12 + {}]", FIBER_PENDING_THROW_OFFSET)); // r10 = pending Throwable*
    emitter.instruction("test r10, r10");                                       // did Fiber->throw() schedule an exception?
    emitter.instruction("je __rt_fiber_suspend_no_throw");                      // skip the raise path when no exception is pending
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], 0", FIBER_PENDING_THROW_OFFSET)); // clear pending_throw before re-raising
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_value", 0);             // _exc_value = Throwable to raise inside this Fiber
    emitter.instruction("call __rt_throw_current");                             // unwind into the active try/catch on the fiber stack
    emitter.label("__rt_fiber_suspend_no_throw");
    emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", FIBER_TRANSFER_VALUE_OFFSET)); // rax = value delivered by resume()
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], 0", FIBER_TRANSFER_VALUE_OFFSET)); // clear transfer_value.lo because the fiber body now owns the resume value
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], 0", FIBER_TRANSFER_VALUE_OFFSET + 8)); // clear transfer_value.hi to leave no stale resume payload
    emitter.instruction("pop r13");                                             // restore caller's r13
    emitter.instruction("pop r12");                                             // restore caller's r12
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the resumer-delivered value
}

pub(super) fn emit_throw_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fiber_throw ---");
    emitter.label_global("__rt_fiber_throw");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while switching fibers
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the throw helper
    emitter.instruction("push r12");                                            // preserve the receiver Fiber pointer across the cooperative switch
    emitter.instruction("push r13");                                            // preserve the Throwable pointer across state validation
    emitter.instruction("mov r12, rdi");                                        // r12 = fiber object pointer
    emitter.instruction("mov r13, rsi");                                        // r13 = Throwable to deliver
    emitter.instruction(&format!("mov r10, QWORD PTR [r12 + {}]", FIBER_STATE_OFFSET)); // r10 = receiver fiber state
    emitter.instruction(&format!("cmp r10, {}", FIBER_STATE_SUSPENDED));        // is the fiber currently paused at Fiber::suspend()?
    emitter.instruction("je __rt_fiber_throw_state_ok");                        // proceed only when the fiber is suspended
    abi::emit_symbol_address(emitter, "rdi", "_fiber_msg_throw_not_suspended"); // rdi = pointer to the static error message
    emitter.instruction("mov esi, 43");                                         // rsi = error message length in bytes
    emitter.instruction("call __rt_fiber_throw_state_error");                   // raise FiberError; this call does not return
    emitter.label("__rt_fiber_throw_state_ok");
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], r13", FIBER_PENDING_THROW_OFFSET)); // fiber->pending_throw = Throwable*
    abi::emit_load_symbol_to_reg(emitter, "r10", "_fiber_current", 0);          // r10 = current execution context
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], r10", FIBER_CALLER_OFFSET)); // fiber->caller = current context
    emitter.instruction("mov rdi, r12");                                        // pass fiber* as the switch target
    emitter.instruction("call __rt_fiber_switch");                              // cooperative switch into the fiber
    emit_check_escape_x86_64(emitter, "throw");
    emitter.instruction(&format!("mov r10, QWORD PTR [r12 + {}]", FIBER_STATE_OFFSET)); // r10 = current fiber state after control returned
    emitter.instruction(&format!("cmp r10, {}", FIBER_STATE_TERMINATED));       // did the fiber finish instead of suspending again?
    emitter.instruction("jne __rt_fiber_throw_return_yield");                   // suspended fibers return their next yielded transfer value
    emit_box_null_mixed(emitter);
    emitter.instruction("jmp __rt_fiber_throw_return_ready");                   // skip the yielded-value load after boxing PHP null
    emitter.label("__rt_fiber_throw_return_yield");
    emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", FIBER_TRANSFER_VALUE_OFFSET)); // rax = fiber->transfer_value.lo
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], 0", FIBER_TRANSFER_VALUE_OFFSET)); // clear transfer_value.lo because ownership moves to the caller
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], 0", FIBER_TRANSFER_VALUE_OFFSET + 8)); // clear transfer_value.hi to leave no stale yielded payload
    emitter.label("__rt_fiber_throw_return_ready");
    emitter.instruction("pop r13");                                             // restore caller's r13
    emitter.instruction("pop r12");                                             // restore caller's r12
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the value yielded by the fiber
}

pub(super) fn emit_get_current_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fiber_get_current ---");
    emitter.label_global("__rt_fiber_get_current");

    abi::emit_load_symbol_to_reg(emitter, "rdi", "_fiber_current", 0);          // rdi = pointer to the currently running fiber
    emitter.instruction("test rdi, rdi");                                       // is this call running on the main thread?
    emitter.instruction("je __rt_fiber_get_current_null");                      // main-thread calls return boxed PHP null
    emitter.instruction("mov rax, 6");                                          // runtime tag 6 = object
    emitter.instruction("xor esi, esi");                                        // object payloads use only the low word
    emitter.instruction("jmp __rt_mixed_from_value");                           // tail-call the boxer so the caller's return address is preserved
    emitter.label("__rt_fiber_get_current_null");
    emitter.instruction("mov rax, 8");                                          // runtime tag 8 = PHP null
    emitter.instruction("xor edi, edi");                                        // null has no low payload word
    emitter.instruction("xor esi, esi");                                        // null has no high payload word
    emitter.instruction("jmp __rt_mixed_from_value");                           // tail-call the boxer so the caller's return address is preserved
}

pub(super) fn emit_get_return_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fiber_get_return ---");
    emitter.label_global("__rt_fiber_get_return");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while checking state
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for getReturn
    emitter.instruction("push r12");                                            // preserve the receiver Fiber pointer across a potential throw
    emitter.instruction("sub rsp, 8");                                          // keep the SysV stack aligned after saving one callee-saved register
    emitter.instruction("mov r12, rdi");                                        // r12 = receiver fiber pointer
    emitter.instruction("test r12, r12");                                       // is the receiver defensively NULL?
    emitter.instruction("je __rt_fiber_get_return_null");                       // null fiber pointers return a safe default
    emitter.instruction(&format!("mov r10, QWORD PTR [r12 + {}]", FIBER_STATE_OFFSET)); // r10 = receiver fiber state
    emitter.instruction(&format!("cmp r10, {}", FIBER_STATE_TERMINATED));       // has the fiber finished its callable?
    emitter.instruction("je __rt_fiber_get_return_state_ok");                   // proceed only when the fiber has terminated
    abi::emit_symbol_address(emitter, "rdi", "_fiber_msg_not_terminated");      // rdi = pointer to the static error message
    emitter.instruction("mov esi, 57");                                         // rsi = error message length in bytes
    emitter.instruction("call __rt_fiber_throw_state_error");                   // raise FiberError; this call does not return
    emitter.label("__rt_fiber_get_return_state_ok");
    emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", FIBER_TRANSFER_VALUE_OFFSET)); // rax = fiber->transfer_value.lo
    emitter.instruction("call __rt_incref");                                    // retain the return Mixed so the caller owns it independently of the Fiber
    emitter.instruction("jmp __rt_fiber_get_return_done");                      // skip the NULL-receiver fallback once the value is loaded
    emitter.label("__rt_fiber_get_return_null");
    emitter.instruction("xor eax, eax");                                        // safe default when a NULL receiver bypassed type checking
    emitter.label("__rt_fiber_get_return_done");
    emitter.instruction("add rsp, 8");                                          // drop the alignment pad
    emitter.instruction("pop r12");                                             // restore caller's r12
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // hand the captured value back to the caller
}

pub(super) fn emit_state_getter_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fiber_state_eq ---");
    emitter.label_global("__rt_fiber_state_eq");

    emitter.instruction("test rdi, rdi");                                       // a NULL fiber pointer never matches any state predicate
    emitter.instruction("je __rt_fiber_state_eq_false");                        // return false for NULL receivers
    emitter.instruction(&format!("mov r10, QWORD PTR [rdi + {}]", FIBER_STATE_OFFSET)); // r10 = current state stored on the fiber
    emitter.instruction("cmp r10, rsi");                                        // compare current state to the requested predicate value
    emitter.instruction("sete al");                                             // materialize the boolean result in the low result byte
    emitter.instruction("movzx eax, al");                                       // widen the boolean result to the canonical integer register
    emitter.instruction("ret");                                                 // return the predicate result
    emitter.label("__rt_fiber_state_eq_false");
    emitter.instruction("xor eax, eax");                                        // NULL fiber pointer always evaluates to false
    emitter.instruction("ret");                                                 // return false to the caller
}

fn emit_check_escape_x86_64(emitter: &mut Emitter, prefix: &str) {
    emitter.instruction(&format!("mov r10, QWORD PTR [r12 + {}]", FIBER_STATE_OFFSET)); // r10 = current fiber state
    emitter.instruction(&format!("cmp r10, {}", FIBER_STATE_TERMINATED));       // is the fiber terminated?
    emitter.instruction(&format!("jne __rt_fiber_{}_no_escape", prefix));       // skip re-raise when the fiber is still alive
    emitter.instruction(&format!("mov r11, QWORD PTR [r12 + {}]", FIBER_PENDING_THROW_OFFSET)); // r11 = parked Throwable
    emitter.instruction("test r11, r11");                                       // did an exception escape from the fiber entry boundary?
    emitter.instruction(&format!("je __rt_fiber_{}_no_escape", prefix));        // skip re-raise when termination was clean
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], 0", FIBER_PENDING_THROW_OFFSET)); // clear pending_throw before re-raising
    abi::emit_store_reg_to_symbol(emitter, "r11", "_exc_value", 0);             // _exc_value = escaped Throwable ready for __rt_throw_current
    emitter.instruction("call __rt_throw_current");                             // re-raise on the caller's stack chain
    emitter.instruction("ud2");                                                 // defensive trap if __rt_throw_current ever returns
    emitter.label(&format!("__rt_fiber_{}_no_escape", prefix));
}
