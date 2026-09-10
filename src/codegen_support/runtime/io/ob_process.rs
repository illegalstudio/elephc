//! Purpose:
//! Completes output-buffer operations before propagating a callback or parent-sink exception.
//!
//! Called from:
//! - `super::ob_buffer::emit_ob_process_and_write()` for explicit operations and chunk flushes.
//!
//! Key details:
//! - Protected unary adapters publish results into the caller's frame, independent of return registers.
//! - Each protected step preserves previous exceptions while buffer depth and ownership are restored.
//! - FINAL operations retire the buffer before the latest pending exception is rethrown.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::runtime::exceptions::deep_cleanup::Scope;

const SCOPE: Scope = Scope { arm: 48, x86: 16 };

/// Emits completion and its two protected request adapters for the selected architecture.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.label_global("__rt_ob_process_and_write");
    if emitter.target.arch == Arch::AArch64 { aarch64(emitter); }
    else { x86_64(emitter); }
    apply_request(emitter);
    write_request(emitter);
}

/// Restores AArch64 output state, truncates or retires the slot, and then propagates any failure.
fn aarch64(emitter: &mut Emitter) {
    // Frame: slot, phase, write, replaced, pointer, length, pending, incoming handler guard.
    emitter.instruction("sub sp, sp, #80");                                     // reserve the operation record and linkage
    emitter.instruction("stp x29, x30, [sp, #64]");                             // preserve the native caller
    emitter.instruction("add x29, sp, #64");                                    // establish the operation frame
    emitter.instruction("stp x0, x1, [sp]");                                    // retain the slot and requested phase
    emitter.instruction("str x2, [sp, #16]");                                   // retain whether surviving bytes must be written
    emitter.instruction("stp xzr, xzr, [sp, #24]");                             // initialize replacement state without owned storage
    emitter.instruction("stp xzr, xzr, [sp, #40]");                             // initialize replacement length and pending exception
    abi::emit_load_symbol_to_reg(emitter, "x9", "_ob_in_handler", 0);
    emitter.instruction("str x9, [sp, #56]");                                   // preserve the caller's output guard
    emitter.instruction("mov x0, sp");                                          // pass the operation record through the unary boundary
    SCOPE.call(emitter, "__rt_ob_apply_request", true);
    emitter.instruction("ldr x10, [sp, #56]");                                  // recover the guard even when the callback escaped
    abi::emit_store_reg_to_symbol(emitter, "x10", "_ob_in_handler", 0);
    emitter.instruction("ldr x10, [sp]");                                       // recover the processed slot
    abi::emit_symbol_address(emitter, "x11", "_ob_flags");
    emitter.instruction("ldr x12, [x11, x10, lsl #3]");                         // retain permissions and earlier status bits
    emitter.instruction("orr x12, x12, #4096");                                 // record that this buffer has been started
    emitter.instruction("tbnz x12, #13, __rt_ob_process_status");               // disabled handlers stay disabled without further processing
    emitter.instruction("ldr x9, [sp, #48]");                                   // inspect whether the handler raised an exception
    emitter.instruction("cbnz x9, __rt_ob_process_disabled");                   // failed handlers subsequently pass bytes through
    abi::emit_symbol_address(emitter, "x13", "_ob_handler_stubs");
    emitter.instruction("ldr x9, [x13, x10, lsl #3]");                          // distinguish a user handler from the default buffer
    emitter.instruction("cbz x9, __rt_ob_process_processed");                   // the default handler processes bytes without a replacement
    emitter.instruction("ldr x9, [sp, #24]");                                   // inspect the user handler's replacement decision
    emitter.instruction("cbnz x9, __rt_ob_process_processed");                  // successful replacements mark the buffer processed
    emitter.label("__rt_ob_process_disabled");
    emitter.instruction("orr x12, x12, #8192");                                 // disable a throwing or false-returning handler
    emitter.instruction("b __rt_ob_process_status");                            // retain any earlier processed bit
    emitter.label("__rt_ob_process_processed");
    emitter.instruction("orr x12, x12, #16384");                                // record successful processing
    emitter.label("__rt_ob_process_status");
    emitter.instruction("str x12, [x11, x10, lsl #3]");                         // publish actual handler state
    emitter.instruction("ldr x9, [sp, #16]");                                   // inspect whether this operation forwards surviving bytes
    emitter.instruction("cbz x9, __rt_ob_process_release");                     // clean operations discard the contents
    abi::emit_store_reg_to_symbol(emitter, "x10", "_ob_level", 0);
    emitter.instruction("mov x0, sp");                                          // pass raw or replacement bytes through the protected parent write
    SCOPE.call(emitter, "__rt_ob_write_request", true);
    emitter.instruction("ldr x10, [sp]");                                       // recover the child slot after a possible parent exception
    emitter.instruction("add x10, x10, #1");                                    // restore depth before retiring child ownership
    abi::emit_store_reg_to_symbol(emitter, "x10", "_ob_level", 0);
    emitter.label("__rt_ob_process_release");
    emitter.instruction("ldr x9, [sp, #24]");                                   // inspect replacement ownership
    emitter.instruction("cbz x9, __rt_ob_process_truncate");                    // pass-through results own no replacement
    emitter.instruction("ldr x0, [sp, #32]");                                   // consume the persisted replacement string
    emitter.instruction("bl __rt_decref_any");                                  // release byte storage after every parent-write outcome
    emitter.label("__rt_ob_process_truncate");
    emitter.instruction("ldr x10, [sp]");                                       // recover the slot whose operation must complete
    abi::emit_symbol_address(emitter, "x11", "_ob_lens");
    emitter.instruction("str xzr, [x11, x10, lsl #3]");                         // empty the buffer before exposing its exception
    emitter.instruction("ldr x9, [sp, #8]");                                    // recover the requested finalization phase
    emitter.instruction("tbz x9, #3, __rt_ob_process_done");                    // clean and flush retain their emptied buffer
    emitter.instruction("mov x0, #0");                                          // the pop operation accepts no payload
    SCOPE.call(emitter, "__rt_ob_pop_free", true);
    emitter.label("__rt_ob_process_done");
    emitter.instruction("ldr x0, [sp, #48]");                                   // retain whether an exception must escape
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the native caller after all required cleanup
    emitter.instruction("add sp, sp, #80");                                     // retire the completed operation record
    emitter.instruction("cbz x0, __rt_ob_process_return");                      // return normally when no callback failed
    emitter.instruction("b __rt_throw_current");                                // use an external tail branch accepted by both ELF and Mach-O assemblers
    emitter.label("__rt_ob_process_return");
    emitter.instruction("ret");                                                 // return normally after successful completion
}

/// Provides the same operation completion order and independent pending state under SysV.
fn x86_64(emitter: &mut Emitter) {
    emitter.instruction("push rbp");                                            // preserve linkage and align subsequent calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable protected-operation frame
    emitter.instruction("sub rsp, 64");                                         // reserve the shared eight-word operation record
    emitter.instruction("mov QWORD PTR [rsp], rdi");                            // retain the buffer slot
    emitter.instruction("mov QWORD PTR [rsp + 8], rsi");                        // retain the requested phase
    emitter.instruction("mov QWORD PTR [rsp + 16], rdx");                       // retain the parent-write decision
    emitter.instruction("mov QWORD PTR [rsp + 24], 0");                         // begin with no replacement owner
    emitter.instruction("mov QWORD PTR [rsp + 32], 0");                         // initialize replacement bytes
    emitter.instruction("mov QWORD PTR [rsp + 40], 0");                         // initialize replacement length
    emitter.instruction("mov QWORD PTR [rsp + 48], 0");                         // begin with no pending exception
    abi::emit_load_symbol_to_reg(emitter, "r9", "_ob_in_handler", 0);
    emitter.instruction("mov QWORD PTR [rsp + 56], r9");                        // retain the incoming output guard
    emitter.instruction("mov rdi, rsp");                                        // pass writable request storage through the protected callback
    SCOPE.call(emitter, "__rt_ob_apply_request", true);
    emitter.instruction("mov r9, QWORD PTR [rsp + 56]");                        // recover the guard on successful and exceptional returns
    abi::emit_store_reg_to_symbol(emitter, "r9", "_ob_in_handler", 0);
    emitter.instruction("mov r10, QWORD PTR [rsp]");                            // recover the processed slot
    abi::emit_symbol_address(emitter, "r11", "_ob_flags");
    emitter.instruction("mov rdx, QWORD PTR [r11 + r10*8]");                    // retain permissions and previous status
    emitter.instruction("or rdx, 4096");                                        // publish started state for user and default buffers
    emitter.instruction("test rdx, 8192");                                      // inspect whether a previous operation disabled this handler
    emitter.instruction("jnz __rt_ob_process_status");                          // leave disabled handlers unprocessed
    emitter.instruction("cmp QWORD PTR [rsp + 48], 0");                         // inspect this callback's pending failure
    emitter.instruction("jne __rt_ob_process_disabled");                        // thrown handlers become pass-through buffers
    abi::emit_symbol_address(emitter, "r8", "_ob_handler_stubs");
    emitter.instruction("cmp QWORD PTR [r8 + r10*8], 0");                       // distinguish the default handler
    emitter.instruction("je __rt_ob_process_processed");                        // default buffers successfully process raw bytes
    emitter.instruction("cmp QWORD PTR [rsp + 24], 0");                         // inspect a user handler's replacement decision
    emitter.instruction("jne __rt_ob_process_processed");                       // successful replacements mark processing complete
    emitter.label("__rt_ob_process_disabled");
    emitter.instruction("or rdx, 8192");                                        // disable throwing and false-returning callbacks
    emitter.instruction("jmp __rt_ob_process_status");                          // preserve any earlier successful processing
    emitter.label("__rt_ob_process_processed");
    emitter.instruction("or rdx, 16384");                                       // record actual successful processing
    emitter.label("__rt_ob_process_status");
    emitter.instruction("mov QWORD PTR [r11 + r10*8], rdx");                    // publish final handler status
    emitter.instruction("cmp QWORD PTR [rsp + 16], 0");                         // inspect the requested output action
    emitter.instruction("je __rt_ob_process_release");                          // cleaning discards every surviving byte
    abi::emit_store_reg_to_symbol(emitter, "r10", "_ob_level", 0);
    emitter.instruction("mov rdi, rsp");                                        // pass the chosen bytes to the protected parent sink
    SCOPE.call(emitter, "__rt_ob_write_request", true);
    emitter.instruction("mov r10, QWORD PTR [rsp]");                            // restore the child depth even if its parent handler threw
    emitter.instruction("add r10, 1");                                          // include the still-owned child slot
    abi::emit_store_reg_to_symbol(emitter, "r10", "_ob_level", 0);
    emitter.label("__rt_ob_process_release");
    emitter.instruction("cmp QWORD PTR [rsp + 24], 0");                         // inspect replacement ownership
    emitter.instruction("je __rt_ob_process_truncate");                         // raw pass-through bytes remain buffer-owned
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // consume the persisted replacement owner
    emitter.instruction("call __rt_decref_any");                                // free replacement storage after the parent returns or throws
    emitter.label("__rt_ob_process_truncate");
    emitter.instruction("mov r10, QWORD PTR [rsp]");                            // recover the slot to empty
    abi::emit_symbol_address(emitter, "r11", "_ob_lens");
    emitter.instruction("mov QWORD PTR [r11 + r10*8], 0");                      // finish the requested cleanup before rethrowing
    emitter.instruction("test QWORD PTR [rsp + 8], 8");                         // inspect the FINAL phase bit
    emitter.instruction("jz __rt_ob_process_done");                             // ordinary clean and flush retain their buffer
    emitter.instruction("xor edi, edi");                                        // the unary pop adapter requires no payload
    SCOPE.call(emitter, "__rt_ob_pop_free", true);
    emitter.label("__rt_ob_process_done");
    emitter.instruction("mov rax, QWORD PTR [rsp + 48]");                       // retain the final exception decision
    emitter.instruction("leave");                                               // remove the completed operation frame
    emitter.instruction("test rax, rax");                                       // distinguish successful completion from a deferred throw
    emitter.instruction("jnz __rt_throw_current");                              // expose the latest exception after all required cleanup
    emitter.instruction("ret");                                                 // return normally to the completed output builtin
}

/// Publishes the handler's replacement triple through a protected unary request.
fn apply_request(emitter: &mut Emitter) {
    emitter.label("__rt_ob_apply_request");
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("sub sp, sp, #32");                                 // reserve request ownership and native linkage
        emitter.instruction("stp x29, x30, [sp, #16]");                         // preserve the protected callback's return address
        emitter.instruction("str x0, [sp]");                                    // retain the borrowed request pointer
        emitter.instruction("ldp x0, x1, [x0]");                                // load slot and phase for the shared handler ABI
        emitter.instruction("bl __rt_ob_apply_handler");                        // run the callback inside the enclosing exception boundary
        emitter.instruction("ldr x9, [sp]");                                    // recover output storage after the callback
        emitter.instruction("stp x0, x1, [x9, #24]");                           // publish replacement state and its owned pointer
        emitter.instruction("str x2, [x9, #40]");                               // publish replacement byte length
        emitter.instruction("ldp x29, x30, [sp, #16]");                         // restore protected caller linkage
        emitter.instruction("add sp, sp, #32");                                 // release request staging
    } else {
        emitter.instruction("push rbp");                                        // preserve and align the protected callback
        emitter.instruction("mov rbp, rsp");                                    // establish request-pointer staging
        emitter.instruction("sub rsp, 16");                                     // reserve one borrowed pointer with call alignment
        emitter.instruction("mov QWORD PTR [rsp], rdi");                        // retain writable replacement outputs
        emitter.instruction("mov rsi, QWORD PTR [rdi + 8]");                    // recover the handler phase
        emitter.instruction("mov rdi, QWORD PTR [rdi]");                        // recover the buffer slot
        emitter.instruction("call __rt_ob_apply_handler");                      // invoke under the enclosing PHP exception boundary
        emitter.instruction("mov r9, QWORD PTR [rsp]");                         // recover the caller's operation record
        emitter.instruction("mov QWORD PTR [r9 + 24], rax");                    // publish the replacement decision
        emitter.instruction("mov QWORD PTR [r9 + 32], rdi");                    // transfer the returned replacement owner
        emitter.instruction("mov QWORD PTR [r9 + 40], rdx");                    // publish its byte count
        emitter.instruction("leave");                                           // remove borrowed request staging
    }
    emitter.instruction("ret");                                                 // return with outputs independent of cleanup-helper registers
}

/// Chooses replacement or original bytes and tail-calls the already-routed parent sink.
fn write_request(emitter: &mut Emitter) {
    emitter.label("__rt_ob_write_request");
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("mov x9, x0");                                      // retain the borrowed operation request
        emitter.instruction("ldr x10, [x9, #24]");                              // inspect whether replacement bytes exist
        emitter.instruction("cbz x10, __rt_ob_write_raw");                      // false and throwing handlers forward original bytes
        emitter.instruction("ldp x0, x1, [x9, #32]");                           // load replacement bytes and length
        emitter.instruction("b __rt_ob_write_bytes");                           // skip raw-buffer lookup for a replacement
        emitter.label("__rt_ob_write_raw");
        emitter.instruction("ldr x10, [x9]");                                   // recover the original buffer slot
        abi::emit_symbol_address(emitter, "x11", "_ob_ptrs");
        emitter.instruction("ldr x0, [x11, x10, lsl #3]");                      // borrow the original buffer storage
        abi::emit_symbol_address(emitter, "x11", "_ob_lens");
        emitter.instruction("ldr x1, [x11, x10, lsl #3]");                      // recover the original byte count
        emitter.label("__rt_ob_write_bytes");
        emitter.instruction("cbz x1, __rt_ob_write_empty");                     // empty writes do not commit response headers or trigger parent chunks
        emitter.instruction("b __rt_stdout_write");                             // complete forwarding before any pending exception escapes
    } else {
        emitter.instruction("mov r9, rdi");                                     // retain the borrowed operation record
        emitter.instruction("cmp QWORD PTR [r9 + 24], 0");                      // inspect the replacement decision
        emitter.instruction("je __rt_ob_write_raw");                            // failed or false-returning handlers preserve raw output
        emitter.instruction("mov rdi, QWORD PTR [r9 + 32]");                    // borrow persisted replacement bytes
        emitter.instruction("mov rsi, QWORD PTR [r9 + 40]");                    // load the replacement byte count
        emitter.instruction("jmp __rt_ob_write_bytes");                         // use replacement bytes without consulting the raw slot
        emitter.label("__rt_ob_write_raw");
        emitter.instruction("mov r10, QWORD PTR [r9]");                         // recover the raw buffer slot
        abi::emit_symbol_address(emitter, "r11", "_ob_ptrs");
        emitter.instruction("mov rdi, QWORD PTR [r11 + r10*8]");                // borrow original buffer bytes
        abi::emit_symbol_address(emitter, "r11", "_ob_lens");
        emitter.instruction("mov rsi, QWORD PTR [r11 + r10*8]");                // recover the original buffer length
        emitter.label("__rt_ob_write_bytes");
        emitter.instruction("test rsi, rsi");                                   // inspect whether the parent has any bytes to receive
        emitter.instruction("jz __rt_ob_write_empty");                          // empty writes leave response commitment and chunk processing untouched
        emitter.instruction("jmp __rt_stdout_write");                           // return through the pending-exception preserving wrapper
    }
    emitter.label("__rt_ob_write_empty");
    emitter.instruction("ret");                                                 // complete an empty flush without touching the parent sink
}
