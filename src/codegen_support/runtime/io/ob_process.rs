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

use elephc_memoir::buffers::output::{self, ControlField, ProcessField, SlotField};

use crate::codegen_support::{emit::Emitter, platform::Arch};
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
    output::emit_store_process_field(emitter, "x0", "sp", ProcessField::Slot);
    output::emit_store_process_field(emitter, "x1", "sp", ProcessField::Phase);
    output::emit_store_process_field(emitter, "x2", "sp", ProcessField::Write);
    for field in [
        ProcessField::Replaced,
        ProcessField::ReplacementPointer,
        ProcessField::ReplacementLength,
        ProcessField::Pending,
    ] {
        output::emit_clear_process_field(emitter, "sp", field);
    }
    output::emit_load_control(emitter, "x9", ControlField::InHandler);
    output::emit_store_process_field(emitter, "x9", "sp", ProcessField::IncomingGuard);
    emitter.instruction("mov x0, sp");                                          // pass the operation record through the unary boundary
    SCOPE.call_local(emitter, "__rt_ob_apply_request", true);
    output::emit_load_process_field(emitter, "x10", "sp", ProcessField::IncomingGuard);
    output::emit_store_control(emitter, "x10", ControlField::InHandler);
    output::emit_load_process_field(emitter, "x10", "sp", ProcessField::Slot);
    output::emit_slot_field_address(emitter, "x11", SlotField::Flags);
    emitter.instruction("ldr x12, [x11, x10, lsl #3]");                         // retain permissions and earlier status bits
    emitter.instruction("orr x12, x12, #4096");                                 // record that this buffer has been started
    emitter.instruction("tbnz x12, #13, __rt_ob_process_status");               // disabled handlers stay disabled without further processing
    output::emit_load_process_field(emitter, "x9", "sp", ProcessField::Pending);
    emitter.instruction("cbnz x9, __rt_ob_process_disabled");                   // failed handlers subsequently pass bytes through
    output::emit_slot_field_address(emitter, "x13", SlotField::HandlerStub);
    emitter.instruction("ldr x9, [x13, x10, lsl #3]");                          // distinguish a user handler from the default buffer
    emitter.instruction("cbz x9, __rt_ob_process_processed");                   // the default handler processes bytes without a replacement
    output::emit_load_process_field(emitter, "x9", "sp", ProcessField::Replaced);
    emitter.instruction("cbnz x9, __rt_ob_process_processed");                  // successful replacements mark the buffer processed
    emitter.label("__rt_ob_process_disabled");
    emitter.instruction("orr x12, x12, #8192");                                 // disable a throwing or false-returning handler
    emitter.instruction("b __rt_ob_process_status");                            // retain any earlier processed bit
    emitter.label("__rt_ob_process_processed");
    emitter.instruction("orr x12, x12, #16384");                                // record successful processing
    emitter.label("__rt_ob_process_status");
    emitter.instruction("str x12, [x11, x10, lsl #3]");                         // publish actual handler state
    output::emit_load_process_field(emitter, "x9", "sp", ProcessField::Write);
    emitter.instruction("cbz x9, __rt_ob_process_release");                     // clean operations discard the contents
    output::emit_store_depth(emitter, "x10");
    emitter.instruction("mov x0, sp");                                          // pass raw or replacement bytes through the protected parent write
    SCOPE.call_local(emitter, "__rt_ob_write_request", true);
    output::emit_load_process_field(emitter, "x10", "sp", ProcessField::Slot);
    emitter.instruction("add x10, x10, #1");                                    // restore depth before retiring child ownership
    output::emit_store_depth(emitter, "x10");
    emitter.label("__rt_ob_process_release");
    output::emit_load_process_field(emitter, "x9", "sp", ProcessField::Replaced);
    emitter.instruction("cbz x9, __rt_ob_process_truncate");                    // pass-through results own no replacement
    output::emit_load_process_field(emitter, "x0", "sp", ProcessField::ReplacementPointer);
    emitter.instruction("bl __rt_decref_any");                                  // release byte storage after every parent-write outcome
    emitter.label("__rt_ob_process_truncate");
    output::emit_load_process_field(emitter, "x10", "sp", ProcessField::Slot);
    elephc_memoir::buffers::output::emit_truncate_slot(emitter);
    output::emit_load_process_field(emitter, "x9", "sp", ProcessField::Phase);
    emitter.instruction("tbz x9, #3, __rt_ob_process_done");                    // clean and flush retain their emptied buffer
    emitter.instruction("mov x0, #0");                                          // the pop operation accepts no payload
    SCOPE.call(emitter, "__rt_ob_pop_free", true);
    emitter.label("__rt_ob_process_done");
    output::emit_load_process_field(emitter, "x0", "sp", ProcessField::Pending);
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
    emitter.instruction(&format!("sub rsp, {}", output::PROCESS_RECORD_BYTES)); // reserve the shared eight-word operation record
    output::emit_store_process_field(emitter, "rdi", "rsp", ProcessField::Slot);
    output::emit_store_process_field(emitter, "rsi", "rsp", ProcessField::Phase);
    output::emit_store_process_field(emitter, "rdx", "rsp", ProcessField::Write);
    for field in [
        ProcessField::Replaced,
        ProcessField::ReplacementPointer,
        ProcessField::ReplacementLength,
        ProcessField::Pending,
    ] {
        output::emit_clear_process_field(emitter, "rsp", field);
    }
    output::emit_load_control(emitter, "r9", ControlField::InHandler);
    output::emit_store_process_field(emitter, "r9", "rsp", ProcessField::IncomingGuard);
    emitter.instruction("mov rdi, rsp");                                        // pass writable request storage through the protected callback
    SCOPE.call_local(emitter, "__rt_ob_apply_request", true);
    output::emit_load_process_field(emitter, "r9", "rsp", ProcessField::IncomingGuard);
    output::emit_store_control(emitter, "r9", ControlField::InHandler);
    output::emit_load_process_field(emitter, "r10", "rsp", ProcessField::Slot);
    output::emit_slot_field_address(emitter, "r11", SlotField::Flags);
    emitter.instruction("mov rdx, QWORD PTR [r11 + r10*8]");                    // retain permissions and previous status
    emitter.instruction("or rdx, 4096");                                        // publish started state for user and default buffers
    emitter.instruction("test rdx, 8192");                                      // inspect whether a previous operation disabled this handler
    emitter.instruction("jnz __rt_ob_process_status");                          // leave disabled handlers unprocessed
    output::emit_load_process_field(emitter, "r9", "rsp", ProcessField::Pending);
    emitter.instruction("test r9, r9");                                         // inspect this callback's pending failure
    emitter.instruction("jne __rt_ob_process_disabled");                        // thrown handlers become pass-through buffers
    output::emit_slot_field_address(emitter, "r8", SlotField::HandlerStub);
    emitter.instruction("cmp QWORD PTR [r8 + r10*8], 0");                       // distinguish the default handler
    emitter.instruction("je __rt_ob_process_processed");                        // default buffers successfully process raw bytes
    output::emit_load_process_field(emitter, "r9", "rsp", ProcessField::Replaced);
    emitter.instruction("test r9, r9");                                         // inspect a user handler's replacement decision
    emitter.instruction("jne __rt_ob_process_processed");                       // successful replacements mark processing complete
    emitter.label("__rt_ob_process_disabled");
    emitter.instruction("or rdx, 8192");                                        // disable throwing and false-returning callbacks
    emitter.instruction("jmp __rt_ob_process_status");                          // preserve any earlier successful processing
    emitter.label("__rt_ob_process_processed");
    emitter.instruction("or rdx, 16384");                                       // record actual successful processing
    emitter.label("__rt_ob_process_status");
    emitter.instruction("mov QWORD PTR [r11 + r10*8], rdx");                    // publish final handler status
    output::emit_load_process_field(emitter, "r9", "rsp", ProcessField::Write);
    emitter.instruction("test r9, r9");                                         // inspect the requested output action
    emitter.instruction("je __rt_ob_process_release");                          // cleaning discards every surviving byte
    output::emit_store_depth(emitter, "r10");
    emitter.instruction("mov rdi, rsp");                                        // pass the chosen bytes to the protected parent sink
    SCOPE.call_local(emitter, "__rt_ob_write_request", true);
    output::emit_load_process_field(emitter, "r10", "rsp", ProcessField::Slot);
    emitter.instruction("add r10, 1");                                          // include the still-owned child slot
    output::emit_store_depth(emitter, "r10");
    emitter.label("__rt_ob_process_release");
    output::emit_load_process_field(emitter, "r9", "rsp", ProcessField::Replaced);
    emitter.instruction("test r9, r9");                                         // inspect replacement ownership
    emitter.instruction("je __rt_ob_process_truncate");                         // raw pass-through bytes remain buffer-owned
    output::emit_load_process_field(emitter, "rax", "rsp", ProcessField::ReplacementPointer);
    emitter.instruction("call __rt_decref_any");                                // free replacement storage after the parent returns or throws
    emitter.label("__rt_ob_process_truncate");
    output::emit_load_process_field(emitter, "r10", "rsp", ProcessField::Slot);
    elephc_memoir::buffers::output::emit_truncate_slot(emitter);
    output::emit_load_process_field(emitter, "r9", "rsp", ProcessField::Phase);
    emitter.instruction("test r9, 8");                                          // inspect the FINAL phase bit
    emitter.instruction("jz __rt_ob_process_done");                             // ordinary clean and flush retain their buffer
    emitter.instruction("xor edi, edi");                                        // the unary pop adapter requires no payload
    SCOPE.call(emitter, "__rt_ob_pop_free", true);
    emitter.label("__rt_ob_process_done");
    output::emit_load_process_field(emitter, "rax", "rsp", ProcessField::Pending);
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
        output::emit_load_process_field(emitter, "x1", "x0", ProcessField::Phase);
        output::emit_load_process_field(emitter, "x0", "x0", ProcessField::Slot);
        emitter.instruction("bl __rt_ob_apply_handler");                        // run the callback inside the enclosing exception boundary
        emitter.instruction("ldr x9, [sp]");                                    // recover output storage after the callback
        output::emit_store_process_field(emitter, "x0", "x9", ProcessField::Replaced);
        output::emit_store_process_field(emitter, "x1", "x9", ProcessField::ReplacementPointer);
        output::emit_store_process_field(emitter, "x2", "x9", ProcessField::ReplacementLength);
        emitter.instruction("ldp x29, x30, [sp, #16]");                         // restore protected caller linkage
        emitter.instruction("add sp, sp, #32");                                 // release request staging
    } else {
        emitter.instruction("push rbp");                                        // preserve and align the protected callback
        emitter.instruction("mov rbp, rsp");                                    // establish request-pointer staging
        emitter.instruction("sub rsp, 16");                                     // reserve one borrowed pointer with call alignment
        emitter.instruction("mov QWORD PTR [rsp], rdi");                        // retain writable replacement outputs
        output::emit_load_process_field(emitter, "rsi", "rdi", ProcessField::Phase);
        output::emit_load_process_field(emitter, "rdi", "rdi", ProcessField::Slot);
        emitter.instruction("call __rt_ob_apply_handler");                      // invoke under the enclosing PHP exception boundary
        emitter.instruction("mov r9, QWORD PTR [rsp]");                         // recover the caller's operation record
        output::emit_store_process_field(emitter, "rax", "r9", ProcessField::Replaced);
        output::emit_store_process_field(emitter, "rdi", "r9", ProcessField::ReplacementPointer);
        output::emit_store_process_field(emitter, "rdx", "r9", ProcessField::ReplacementLength);
        emitter.instruction("leave");                                           // remove borrowed request staging
    }
    emitter.instruction("ret");                                                 // return with outputs independent of cleanup-helper registers
}

/// Chooses replacement or original bytes and tail-calls the already-routed parent sink.
fn write_request(emitter: &mut Emitter) {
    emitter.label("__rt_ob_write_request");
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("mov x9, x0");                                      // retain the borrowed operation request
        output::emit_load_process_field(emitter, "x10", "x9", ProcessField::Replaced);
        emitter.instruction("cbz x10, __rt_ob_write_raw");                      // false and throwing handlers forward original bytes
        output::emit_load_process_field(emitter, "x0", "x9", ProcessField::ReplacementPointer);
        output::emit_load_process_field(emitter, "x1", "x9", ProcessField::ReplacementLength);
        emitter.instruction("b __rt_ob_write_bytes");                           // skip raw-buffer lookup for a replacement
        emitter.label("__rt_ob_write_raw");
        output::emit_load_process_field(emitter, "x10", "x9", ProcessField::Slot);
        output::emit_load_buffer_field(emitter, output::BufferField::Pointer, "x11", "x10", "x0");
        output::emit_load_buffer_field(emitter, output::BufferField::Length, "x11", "x10", "x1");
        emitter.label("__rt_ob_write_bytes");
        emitter.instruction("cbz x1, __rt_ob_write_empty");                     // empty writes do not commit response headers or trigger parent chunks
        emitter.instruction("b __rt_stdout_write");                             // complete forwarding before any pending exception escapes
    } else {
        emitter.instruction("mov r9, rdi");                                     // retain the borrowed operation record
        output::emit_load_process_field(emitter, "r10", "r9", ProcessField::Replaced);
        emitter.instruction("test r10, r10");                                   // inspect the replacement decision
        emitter.instruction("je __rt_ob_write_raw");                            // failed or false-returning handlers preserve raw output
        output::emit_load_process_field(emitter, "rdi", "r9", ProcessField::ReplacementPointer);
        output::emit_load_process_field(emitter, "rsi", "r9", ProcessField::ReplacementLength);
        emitter.instruction("jmp __rt_ob_write_bytes");                         // use replacement bytes without consulting the raw slot
        emitter.label("__rt_ob_write_raw");
        output::emit_load_process_field(emitter, "r10", "r9", ProcessField::Slot);
        output::emit_load_buffer_field(emitter, output::BufferField::Pointer, "r11", "r10", "rdi");
        output::emit_load_buffer_field(emitter, output::BufferField::Length, "r11", "r10", "rsi");
        emitter.label("__rt_ob_write_bytes");
        emitter.instruction("test rsi, rsi");                                   // inspect whether the parent has any bytes to receive
        emitter.instruction("jz __rt_ob_write_empty");                          // empty writes leave response commitment and chunk processing untouched
        emitter.instruction("jmp __rt_stdout_write");                           // return through the pending-exception preserving wrapper
    }
    emitter.label("__rt_ob_write_empty");
    emitter.instruction("ret");                                                 // complete an empty flush without touching the parent sink
}
