//! Purpose:
//! Emits JSON error recording and throw-on-error runtime helper.
//! Provides the runtime assembly used by JSON builtins on the selected target.
//!
//! Called from:
//! - `crate::codegen_support::runtime::system` during runtime emission.
//!
//! Key details:
//! - The helper must set `_json_last_error` before optionally constructing JsonException.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::abi;

/// Emits `__rt_json_throw_error`: records a JSON error code and, when
/// `JSON_THROW_ON_ERROR` is set in `_json_active_flags`, allocates a
/// JsonException with a PHP-compatible message and throws it via
/// `__rt_throw_current`. When the flag is clear, only the `_json_last_error`
/// slot is updated and control returns to the caller.
///
/// Input:
///   ARM64: x0 = JSON_ERROR_* code
///   x86_64: rax = JSON_ERROR_* code
///
/// Side effects:
///   - `_json_last_error` is always updated with the error code.
///   - When `JSON_THROW_ON_ERROR` is set: `_exc_value` is written, heap is
///     allocated for a JsonException object, and control does not return
///     (tail-calls `__rt_throw_current`).
///   - Clobbers: x9-x13 (ARM64), r10-r11 (x86_64), and scratch stack frame.
pub(crate) fn emit_json_throw_error(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_json_throw_error_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: json_throw_error ---");
    emitter.label_global("__rt_json_throw_error");

    // Stack layout (32 bytes): only used when we need to allocate + throw.
    //   [sp, #0]  = saved error code
    //   [sp, #16] = saved x29
    //   [sp, #24] = saved x30
    emitter.instruction("sub sp, sp, #32");                                     // reserve scratch slots for the error code and frame linkage
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish a stable frame pointer for the helper
    emitter.instruction("str x0, [sp, #0]");                                    // save the error code across helper calls

    // Always update _json_last_error so json_last_error()/json_last_error_msg()
    // observe the failure regardless of whether THROW_ON_ERROR is set.
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_json_last_error");
    emitter.instruction("str x0, [x9]");                                        // record the error code in the runtime's last-error slot

    // Fast path: when JSON_THROW_ON_ERROR is clear, just return.
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_json_active_flags");
    emitter.instruction("ldr x9, [x9]");                                        // load the active flag bitmask
    emitter.instruction("mov x10, #4194304");                                   // JSON_THROW_ON_ERROR = bit 4194304
    emitter.instruction("tst x9, x10");                                         // is the throw bit set?
    emitter.instruction("b.eq __rt_json_throw_error_return");                   // bail out when throwing is not requested

    // Allocate a 32-byte JsonException payload (class_id + message ptr/len + code).
    emitter.instruction("mov x0, #32");                                         // size = class_id (8) + message ptr/len (16) + code (8)
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the JsonException payload
    emitter.instruction("mov x9, #6");                                          // heap kind 6 = object
    emitter.instruction("str x9, [x0, #-8]");                                   // tag the allocation as an object in the uniform header

    // Stamp the per-program JsonException class id into [obj+0].
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_json_exception_class_id");
    emitter.instruction("ldr x9, [x9]");                                        // load JsonException's runtime class id (-1 when absent)
    emitter.instruction("str x9, [x0]");                                        // store the class id at the object header slot
    emitter.instruction("str x0, [sp, #8]");                                    // save the exception object while formatting its message

    // Look up and optionally suffix the message for this error code.
    emitter.instruction("bl __rt_json_error_message");                          // format the JSON error message, including decode location when present
    emitter.instruction("ldr x0, [sp, #8]");                                    // restore the exception object after message formatting
    emitter.instruction("str x1, [x0, #8]");                                    // obj.message_ptr (Exception's `message` property is the first slot)
    emitter.instruction("str x2, [x0, #16]");                                   // obj.message_len
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the saved error code for $code field
    emitter.instruction("str x10, [x0, #24]");                                  // obj.code (matches Exception's `code` property layout)

    // Publish the new exception object via _exc_value and longjmp to the
    // active catch handler through the standard throw helper.
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_exc_value");
    emitter.instruction("str x0, [x9]");                                        // _exc_value = JsonException pointer
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address before tail-call
    emitter.instruction("add sp, sp, #32");                                     // release the helper scratch frame before tail-call
    emitter.instruction("b __rt_throw_current");                                // tail-call the standard exception unwinder

    emitter.label("__rt_json_throw_error_return");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper scratch frame
    emitter.instruction("ret");                                                 // return to the caller without throwing

    emit_json_set_error_location_aarch64(emitter);
}

/// Emits AArch64 `__rt_json_set_error_location`.
///
/// Input `x0` is an absolute pointer into the current `json_decode()` source,
/// or one byte past the offending token for end-of-input diagnostics. The
/// helper scans from `_json_error_source_ptr` to that pointer, records
/// one-based line/column counters, and marks the location state active. If no
/// decode source is active, the helper returns without changing location state.
fn emit_json_set_error_location_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_set_error_location ---");
    emitter.label_global("__rt_json_set_error_location");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // save frame pointer and return address while scanning the source
    emitter.instruction("mov x29, sp");                                         // establish a stable frame for location calculation
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_json_error_source_ptr");
    emitter.instruction("ldr x9, [x9]");                                        // load the persisted json_decode source pointer
    emitter.instruction("cbz x9, __rt_json_set_error_location_done");           // skip location state when no json_decode source is active
    emitter.instruction("mov x10, #1");                                         // line counter starts at 1
    emitter.instruction("mov x11, #1");                                         // column counter starts at 1
    emitter.instruction("cmp x0, x9");                                          // is the target pointer before or at the source start?
    emitter.instruction("b.ls __rt_json_set_error_location_store");             // clamp out-of-range pointers to 1:1
    emitter.label("__rt_json_set_error_location_loop");
    emitter.instruction("cmp x9, x0");                                          // have we reached the target pointer?
    emitter.instruction("b.ge __rt_json_set_error_location_store");             // stop scanning once the target has been reached
    emitter.instruction("ldrb w12, [x9], #1");                                  // consume one source byte while advancing the scan pointer
    emitter.instruction("cmp w12, #10");                                        // newline resets the column and advances the line
    emitter.instruction("b.eq __rt_json_set_error_location_newline");           // handle LF as the JSON line separator
    emitter.instruction("add x11, x11, #1");                                    // advance the one-based column for a non-newline byte
    emitter.instruction("b __rt_json_set_error_location_loop");                 // continue scanning toward the target pointer
    emitter.label("__rt_json_set_error_location_newline");
    emitter.instruction("add x10, x10, #1");                                    // advance to the next one-based line
    emitter.instruction("mov x11, #1");                                         // reset the one-based column at the start of the next line
    emitter.instruction("b __rt_json_set_error_location_loop");                 // continue scanning after the newline
    emitter.label("__rt_json_set_error_location_store");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_json_error_line");
    emitter.instruction("str x10, [x9]");                                       // store the calculated one-based line
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_json_error_column");
    emitter.instruction("str x11, [x9]");                                       // store the calculated one-based column
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_json_error_location_active");
    emitter.instruction("mov x12, #1");                                         // mark that the last JSON error has location metadata
    emitter.instruction("str x12, [x9]");                                       // publish the active location flag
    emitter.label("__rt_json_set_error_location_done");
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return to the JSON parser error path
}

/// x86_64 Linux implementation of `emit_json_throw_error`.
///
/// Input:
///   rax = JSON_ERROR_* code
fn emit_json_throw_error_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_throw_error ---");
    emitter.label_global("__rt_json_throw_error");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving scratch space
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the helper
    emitter.instruction("sub rsp, 32");                                         // reserve a scratch slot for the error code
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the error code across the helper-call sequence

    abi::emit_store_reg_to_symbol(emitter, "rax", "_json_last_error", 0);       // record the error code in the runtime's last-error slot
    abi::emit_load_symbol_to_reg(emitter, "rdx", "_json_active_flags", 0);      // load the active flag bitmask
    emitter.instruction("test rdx, 0x400000");                                  // JSON_THROW_ON_ERROR = 0x400000
    emitter.instruction("je __rt_json_throw_error_return_x");                   // bail out when throwing is not requested

    emitter.instruction("mov rax, 32");                                         // size = class_id (8) + message ptr/len (16) + code (8)
    emitter.instruction("call __rt_heap_alloc");                                // allocate the JsonException payload (rax = payload ptr)
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(6))); // stamp the canonical x86_64 heap-kind word (magic + kind 6 throwable)
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // tag the allocation as an object in the uniform header

    abi::emit_load_symbol_to_reg(emitter, "r10", "_json_exception_class_id", 0); // load JsonException's runtime class id (-1 when absent)
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store the class id at the object header slot
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the exception object while formatting its message

    emitter.instruction("call __rt_json_error_message");                        // format the JSON error message, including decode location when present
    emitter.instruction("mov r10, rax");                                        // r10 = formatted message pointer
    emitter.instruction("mov r11, rdx");                                        // r11 = formatted message length
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // restore the exception object after message formatting
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // obj.message_ptr
    emitter.instruction("mov QWORD PTR [rax + 16], r11");                       // obj.message_len
    emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // reload the saved error code for $code field
    emitter.instruction("mov QWORD PTR [rax + 24], rcx");                       // obj.code (matches Exception's `code` property layout)

    abi::emit_store_reg_to_symbol(emitter, "rax", "_exc_value", 0);             // _exc_value = JsonException pointer
    emitter.instruction("mov rsp, rbp");                                        // unwind the helper scratch frame before tail-call
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before tail-call
    emitter.instruction("jmp __rt_throw_current");                              // tail-call the standard exception unwinder

    emitter.label("__rt_json_throw_error_return_x");
    emitter.instruction("mov rsp, rbp");                                        // unwind the helper scratch frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller without throwing

    emit_json_set_error_location_x86_64(emitter);
}

/// Emits x86_64 `__rt_json_set_error_location`.
///
/// Input `rax` is an absolute pointer into the current `json_decode()` source,
/// or one byte past the offending token for end-of-input diagnostics. The
/// helper scans from `_json_error_source_ptr` to that pointer, records
/// one-based line/column counters, and marks the location state active. If no
/// decode source is active, the helper returns without changing location state.
fn emit_json_set_error_location_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_set_error_location ---");
    emitter.label_global("__rt_json_set_error_location");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while scanning the source
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame for location calculation
    abi::emit_load_symbol_to_reg(emitter, "r8", "_json_error_source_ptr", 0);   // load the persisted json_decode source pointer
    emitter.instruction("test r8, r8");                                         // check whether json_decode published an input source pointer
    emitter.instruction("je __rt_json_set_error_location_done_x");              // skip location state when no json_decode source is active
    emitter.instruction("mov r9, 1");                                           // line counter starts at 1
    emitter.instruction("mov r10, 1");                                          // column counter starts at 1
    emitter.instruction("cmp rax, r8");                                         // is the target pointer before or at the source start?
    emitter.instruction("jbe __rt_json_set_error_location_store_x");            // clamp out-of-range pointers to 1:1
    emitter.label("__rt_json_set_error_location_loop_x");
    emitter.instruction("cmp r8, rax");                                         // have we reached the target pointer?
    emitter.instruction("jae __rt_json_set_error_location_store_x");            // stop scanning once the target has been reached
    emitter.instruction("movzx r11, BYTE PTR [r8]");                            // load one source byte for line/column accounting
    emitter.instruction("add r8, 1");                                           // advance the scan pointer past the consumed byte
    emitter.instruction("cmp r11, 10");                                         // newline resets the column and advances the line
    emitter.instruction("je __rt_json_set_error_location_newline_x");           // handle LF as the JSON line separator
    emitter.instruction("add r10, 1");                                          // advance the one-based column for a non-newline byte
    emitter.instruction("jmp __rt_json_set_error_location_loop_x");             // continue scanning toward the target pointer
    emitter.label("__rt_json_set_error_location_newline_x");
    emitter.instruction("add r9, 1");                                           // advance to the next one-based line
    emitter.instruction("mov r10, 1");                                          // reset the one-based column at the start of the next line
    emitter.instruction("jmp __rt_json_set_error_location_loop_x");             // continue scanning after the newline
    emitter.label("__rt_json_set_error_location_store_x");
    abi::emit_store_reg_to_symbol(emitter, "r9", "_json_error_line", 0);        // store the calculated one-based line
    abi::emit_store_reg_to_symbol(emitter, "r10", "_json_error_column", 0);     // store the calculated one-based column
    abi::emit_store_imm_to_symbol(emitter, "_json_error_location_active", 0, 1); // publish that the last JSON error has location metadata
    emitter.label("__rt_json_set_error_location_done_x");
    emitter.instruction("mov rsp, rbp");                                        // release location helper frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to the JSON parser error path
}
