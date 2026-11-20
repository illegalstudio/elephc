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

use elephc_memoir::buffers::output;


/// Emits buffer detachment and the optional Rust callback-retirement trampoline.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.label_global("__rt_ob_pop_free");
    output::emit_pop_storage(emitter);
    if emitter.target.arch == Arch::AArch64 { aarch64(emitter); }
    else { x86_64(emitter); }
    output::emit_pop_empty_return(emitter);
    release_eval(emitter);
}

/// Dispatches the detached AArch64 handler owner to its release path.
fn aarch64(emitter: &mut Emitter) {
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
}

/// Dispatches the detached SysV handler owner to its release path.
fn x86_64(emitter: &mut Emitter) {
    abi::emit_symbol_address(emitter, "r8", "__rt_ob_invoke_descriptor");
    emitter.instruction("cmp r11, r8");                                         // distinguish a native descriptor from an eval registry id
    emitter.instruction("je __rt_ob_pop_native");                               // use typed descriptor retirement for native callbacks
    abi::emit_symbol_address(emitter, "r8", "__rt_ob_eval_trampoline");
    emitter.instruction("cmp r11, r8");                                         // recognize a callback owned by the eval registry
    emitter.instruction("je __rt_ob_release_eval_handler");                     // retire its registration after freeing all buffer storage
    emitter.instruction("ret");                                                 // default handlers have no callable owner
    emitter.label("__rt_ob_pop_native");
    emitter.instruction("jmp __rt_callable_descriptor_release");                // release captures with no remaining buffer allocations to abandon
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
