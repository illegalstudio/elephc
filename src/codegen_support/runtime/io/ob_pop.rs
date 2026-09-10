//! Purpose:
//! Detaches a closing output buffer and retires its native or eval callback owner.
//!
//! Called from:
//! - `super::ob_buffer::emit_ob_pop_free()` and output end/get operations.
//!
//! Key details:
//! - The buffer's bytes and display name are released before any callable destructor runs.
//! - Retired slot fields are never reread, so a destructor may safely create a replacement buffer.
//! - Eval retirement transfers exceptions back to native code before any PHP unwind.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

const OWNERS: [&str; 4] = ["_ob_handler_stubs", "_ob_handler_envs", "_ob_name_ptrs", "_ob_ptrs"];
const METADATA: [&str; 6] = ["_ob_lens", "_ob_caps", "_ob_name_lens", "_ob_chunk_sizes", "_ob_flags", "_ob_started"];

/// Emits buffer detachment and the optional Rust callback-retirement trampoline.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.label_global("__rt_ob_pop_free");
    if emitter.target.arch == Arch::AArch64 { aarch64(emitter); }
    else { x86_64(emitter); }
    release_eval(emitter);
}

/// Snapshots AArch64 owners, clears the retired slot, and frees auxiliary storage before callbacks.
fn aarch64(emitter: &mut Emitter) {
    emitter.instruction("sub sp, sp, #48");                                     // reserve four detached fields plus native linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller across storage release
    emitter.instruction("add x29, sp, #32");                                    // establish the detached-buffer frame
    abi::emit_symbol_address(emitter, "x9", "_ob_level");
    emitter.instruction("ldr x10, [x9]");                                       // inspect the current output depth
    emitter.instruction("cbz x10, __rt_ob_pop_empty");                          // no buffer owns fields to release
    emitter.instruction("sub x10, x10, #1");                                    // identify the slot being removed
    emitter.instruction("str x10, [x9]");                                       // publish the closed buffer before running any cleanup
    for (index, symbol) in OWNERS.into_iter().enumerate() {
        abi::emit_symbol_address(emitter, "x11", symbol);
        emitter.instruction("ldr x12, [x11, x10, lsl #3]");                     // copy one retired slot field before external cleanup
        emitter.instruction(&format!("str x12, [sp, #{}]", index * 8));         // keep detached fields independent of any replacement buffer
        emitter.instruction("str xzr, [x11, x10, lsl #3]");                     // remove the old slot's ownership claim
    }
    for symbol in METADATA {
        abi::emit_symbol_address(emitter, "x11", symbol);
        emitter.instruction("str xzr, [x11, x10, lsl #3]");                     // clear metadata before the slot may be reused
    }
    emitter.instruction("ldr x0, [sp, #16]");                                   // consume the copied display-name owner
    emitter.instruction("bl __rt_decref_any");                                  // retire display bytes while no PHP destructor can run
    emitter.instruction("ldr x0, [sp, #24]");                                   // consume the detached capture-storage block
    emitter.instruction("bl __rt_heap_free");                                   // free buffer bytes before a callback owner can reenter output APIs
    emitter.instruction("ldr x12, [sp]");                                       // identify the detached callback representation
    emitter.instruction("ldr x0, [sp, #8]");                                    // transfer its environment word to the appropriate release path
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore linkage after every non-callback owner has been consumed
    emitter.instruction("add sp, sp, #48");                                     // leave no buffer-owned state beneath the callback destructor
    abi::emit_symbol_address(emitter, "x13", "__rt_ob_invoke_descriptor");
    emitter.instruction("cmp x12, x13");                                        // native handlers own a typed callable descriptor
    emitter.instruction("b.eq __rt_ob_pop_native");                             // consume the descriptor through its typed release operation
    abi::emit_symbol_address(emitter, "x13", "__rt_ob_eval_trampoline");
    emitter.instruction("cmp x12, x13");                                        // eval handlers own a registry identity
    emitter.instruction("b.eq __rt_ob_pop_eval");                               // keep the conditional transfer inside the current Mach-O atom
    emitter.instruction("ret");                                                 // the default handler owns no callback
    emitter.label("__rt_ob_pop_eval");
    emitter.instruction("b __rt_ob_release_eval_handler");                      // detach the registration and release its callback outside the registry lock
    emitter.label("__rt_ob_pop_native");
    emitter.instruction("b __rt_callable_descriptor_release");                  // propagate destructor exceptions only after buffer cleanup has completed
    emitter.label("__rt_ob_pop_empty");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore linkage for an empty stack
    emitter.instruction("add sp, sp, #48");                                     // release the unused detached-field frame
    emitter.instruction("ret");                                                 // leave an empty buffer stack unchanged
}

/// Performs the same owner detachment and callback-last release order under SysV.
fn x86_64(emitter: &mut Emitter) {
    emitter.instruction("push rbp");                                            // align the stack and preserve caller linkage
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame for detached owners
    emitter.instruction("sub rsp, 32");                                         // reserve the four retired slot fields
    abi::emit_symbol_address(emitter, "r9", "_ob_level");
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // inspect the current stack depth
    emitter.instruction("test r10, r10");                                       // distinguish an empty stack from a live buffer
    emitter.instruction("jz __rt_ob_pop_empty");                                // skip cleanup when no buffer is active
    emitter.instruction("sub r10, 1");                                          // identify the slot being closed
    emitter.instruction("mov QWORD PTR [r9], r10");                             // publish the removal before any cleanup callback
    for (index, symbol) in OWNERS.into_iter().enumerate() {
        abi::emit_symbol_address(emitter, "r11", symbol);
        emitter.instruction("mov rax, QWORD PTR [r11 + r10*8]");                // snapshot one field before the slot may be reused
        emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", index * 8)); // retain detached ownership across allocator calls
        emitter.instruction("mov QWORD PTR [r11 + r10*8], 0");                  // remove this field from the retired stack slot
    }
    for symbol in METADATA {
        abi::emit_symbol_address(emitter, "r11", symbol);
        emitter.instruction("mov QWORD PTR [r11 + r10*8], 0");                  // erase metadata before a destructor can replace this slot
    }
    emitter.instruction("mov rax, QWORD PTR [rsp + 16]");                       // transfer the persisted display-name owner
    emitter.instruction("call __rt_decref_any");                                // release auxiliary name bytes before callback destruction
    emitter.instruction("mov rax, QWORD PTR [rsp + 24]");                       // transfer the detached buffer allocation
    emitter.instruction("call __rt_heap_free");                                 // retire capture storage before any PHP reentry
    emitter.instruction("mov r11, QWORD PTR [rsp]");                            // recover the callback representation
    emitter.instruction("mov rax, QWORD PTR [rsp + 8]");                        // transfer the retained descriptor or eval registration id
    emitter.instruction("leave");                                               // remove the detached-owner frame before the final release
    abi::emit_symbol_address(emitter, "r8", "__rt_ob_invoke_descriptor");
    emitter.instruction("cmp r11, r8");                                         // distinguish a native descriptor from an eval registry id
    emitter.instruction("je __rt_ob_pop_native");                               // use typed descriptor retirement for native callbacks
    abi::emit_symbol_address(emitter, "r8", "__rt_ob_eval_trampoline");
    emitter.instruction("cmp r11, r8");                                         // recognize a callback owned by the eval registry
    emitter.instruction("je __rt_ob_release_eval_handler");                     // retire its registration after freeing all buffer storage
    emitter.instruction("ret");                                                 // default handlers have no callable owner
    emitter.label("__rt_ob_pop_native");
    emitter.instruction("jmp __rt_callable_descriptor_release");                // release captures with no remaining buffer allocations to abandon
    emitter.label("__rt_ob_pop_empty");
    emitter.instruction("leave");                                               // restore the caller when the output stack was empty
    emitter.instruction("ret");                                                 // complete the no-op pop
}

/// Calls the installed eval retirement action and publishes a returned Throwable after Rust exits.
fn release_eval(emitter: &mut Emitter) {
    emitter.label_global("__rt_ob_release_eval_handler");
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("sub sp, sp, #32");                                 // reserve returned Throwable ownership and caller linkage
        emitter.instruction("stp x29, x30, [sp, #16]");                         // retain the caller across Rust callback retirement
        emitter.instruction("add x29, sp, #16");                                // establish the native callback frame
        abi::emit_load_symbol_to_reg(emitter, "x10", "_elephc_eval_ob_release_fn", 0);
        emitter.instruction("cbz x10, __rt_ob_release_eval_done");              // no eval bridge is installed in an ordinary native program
        emitter.instruction("str xzr, [sp]");                                   // initialize the returned Throwable owner
        emitter.instruction("mov x1, sp");                                      // provide writable boxed-Throwable storage
        emitter.instruction("blr x10");                                         // retire the callback without unwinding through Rust
        emitter.instruction("cbz x0, __rt_ob_release_eval_done");               // successful retirement transfers no exception
        emitter.instruction("cmp x0, #2");                                      // recognize an owned pending PHP Throwable
        emitter.instruction("b.ne __rt_ob_release_eval_fatal");                 // preserve a fatal callback protocol failure
        emitter.instruction("ldr x0, [sp]");                                    // transfer the callback's boxed Throwable owner
        emitter.instruction("cbz x0, __rt_ob_release_eval_fatal");              // never publish a missing exception owner
        emitter.instruction("ldp x29, x30, [sp, #16]");                         // restore the native caller before PHP unwinding
        emitter.instruction("add sp, sp, #32");                                 // leave no Rust or callback-retirement frame active
        emitter.instruction("b __rt_destructor_throw_mixed");                   // consume the box through the shared destructor exception path
        emitter.label("__rt_ob_release_eval_fatal");
        emitter.instruction("mov x0, #1");                                      // report an unrecoverable callback protocol failure
        emitter.bl_c("exit");
        emitter.label("__rt_ob_release_eval_done");
        emitter.instruction("ldp x29, x30, [sp, #16]");                         // restore linkage after ordinary callback retirement
        emitter.instruction("add sp, sp, #32");                                 // release empty Throwable storage
        emitter.instruction("ret");                                             // return to the completed native buffer operation
    } else {
        emitter.instruction("push rbp");                                        // align the Rust callback and preserve native linkage
        emitter.instruction("mov rbp, rsp");                                    // establish the callback-retirement frame
        emitter.instruction("sub rsp, 16");                                     // reserve returned boxed-Throwable storage
        emitter.instruction("mov rdi, rax");                                    // adapt the native environment word to the C registry id
        abi::emit_load_symbol_to_reg(emitter, "r10", "_elephc_eval_ob_release_fn", 0);
        emitter.instruction("test r10, r10");                                   // inspect whether an eval retirement hook is installed
        emitter.instruction("jz __rt_ob_release_eval_done");                    // ordinary native programs own no eval registrations
        emitter.instruction("mov QWORD PTR [rsp], 0");                          // initialize the returned Throwable owner
        emitter.instruction("mov rsi, rsp");                                    // pass writable boxed-Throwable storage
        emitter.instruction("call r10");                                        // finish Rust-owned callback cleanup before inspecting its status
        emitter.instruction("test rax, rax");                                   // distinguish successful retirement from failure
        emitter.instruction("jz __rt_ob_release_eval_done");                    // no exception remains on ordinary closure
        emitter.instruction("cmp rax, 2");                                      // recognize a transferred PHP Throwable owner
        emitter.instruction("jne __rt_ob_release_eval_fatal");                  // reject an invalid callback status
        emitter.instruction("mov rax, QWORD PTR [rsp]");                        // transfer the callback-owned Throwable box
        emitter.instruction("test rax, rax");                                   // require a concrete owner before publication
        emitter.instruction("jz __rt_ob_release_eval_fatal");                   // preserve protocol failure for a missing exception box
        emitter.instruction("leave");                                           // restore native linkage after Rust retirement has returned
        emitter.instruction("jmp __rt_destructor_throw_mixed");                 // publish the box and propagate through the nearest PHP handler
        emitter.label("__rt_ob_release_eval_fatal");
        emitter.instruction("mov edi, 1");                                      // fail the process on an invalid retirement protocol
        emitter.bl_c("exit");
        emitter.label("__rt_ob_release_eval_done");
        emitter.instruction("leave");                                           // discard empty Throwable storage and restore caller linkage
        emitter.instruction("ret");                                             // finish the closed buffer operation
    }
}
