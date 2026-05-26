//! Purpose:
//! Emits JSON error recording and throw-on-error runtime helper.
//! Provides the runtime assembly used by JSON builtins on the selected target.
//!
//! Called from:
//! - `crate::codegen::runtime::system` during runtime emission.
//!
//! Key details:
//! - The helper must set `_json_last_error` before optionally constructing JsonException.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

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
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_last_error");
    emitter.instruction("str x0, [x9]");                                        // record the error code in the runtime's last-error slot

    // Fast path: when JSON_THROW_ON_ERROR is clear, just return.
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_active_flags");
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
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_exception_class_id");
    emitter.instruction("ldr x9, [x9]");                                        // load JsonException's runtime class id (-1 when absent)
    emitter.instruction("str x9, [x0]");                                        // store the class id at the object header slot

    // Look up the message for this error code in the shared message table.
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the saved error code for the message lookup
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_json_err_msg_count");
    emitter.instruction("ldr x11, [x11]");                                      // load the message-table cardinality
    emitter.instruction("cmp x10, x11");                                        // is the code within the message table?
    emitter.instruction("b.lo 1f");                                             // jump ahead when the code is in range
    emitter.instruction("mov x10, #0");                                         // clamp out-of-range codes to JSON_ERROR_NONE
    emitter.label("1");
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_json_err_msg_table");
    emitter.instruction("lsl x10, x10, #4");                                    // step over a (ptr,len) pair per code
    emitter.instruction("add x11, x11, x10");                                   // point at the requested entry
    emitter.instruction("ldr x12, [x11]");                                      // x12 = message pointer
    emitter.instruction("ldr x13, [x11, #8]");                                  // x13 = message length
    emitter.instruction("str x12, [x0, #8]");                                   // obj.message_ptr (Exception's `message` property is the first slot)
    emitter.instruction("str x13, [x0, #16]");                                  // obj.message_len
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the saved error code for $code field
    emitter.instruction("str x10, [x0, #24]");                                  // obj.code (matches Exception's `code` property layout)

    // Publish the new exception object via _exc_value and longjmp to the
    // active catch handler through the standard throw helper.
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_exc_value");
    emitter.instruction("str x0, [x9]");                                        // _exc_value = JsonException pointer
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address before tail-call
    emitter.instruction("add sp, sp, #32");                                     // release the helper scratch frame before tail-call
    emitter.instruction("b __rt_throw_current");                                // tail-call the standard exception unwinder

    emitter.label("__rt_json_throw_error_return");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper scratch frame
    emitter.instruction("ret");                                                 // return to the caller without throwing
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

    emitter.instruction("mov QWORD PTR [rip + _json_last_error], rax");         // record the error code in the runtime's last-error slot
    emitter.instruction("mov rdx, QWORD PTR [rip + _json_active_flags]");       // load the active flag bitmask
    emitter.instruction("test rdx, 0x400000");                                  // JSON_THROW_ON_ERROR = 0x400000
    emitter.instruction("je __rt_json_throw_error_return_x");                   // bail out when throwing is not requested

    emitter.instruction("mov rax, 32");                                         // size = class_id (8) + message ptr/len (16) + code (8)
    emitter.instruction("call __rt_heap_alloc");                                // allocate the JsonException payload (rax = payload ptr)
    emitter.instruction("mov r10, 0x4548504c00000006");                         // x86_64 heap-kind word: HE LP magic + kind 6 (object)
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // tag the allocation as an object in the uniform header

    emitter.instruction("mov r10, QWORD PTR [rip + _json_exception_class_id]"); // load JsonException's runtime class id (-1 when absent)
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store the class id at the object header slot

    emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // reload the saved error code
    emitter.instruction("mov r10, QWORD PTR [rip + _json_err_msg_count]");      // load the message-table cardinality
    emitter.instruction("cmp rcx, r10");                                        // is the code within the message table?
    emitter.instruction("jb 1f");                                               // jump ahead when the code is in range
    emitter.instruction("xor rcx, rcx");                                        // clamp out-of-range codes to JSON_ERROR_NONE
    emitter.label("1");
    emitter.instruction("lea r9, [rip + _json_err_msg_table]");                 // address of the (ptr,len) message table
    emitter.instruction("shl rcx, 4");                                          // step over a (ptr,len) pair per code
    emitter.instruction("add r9, rcx");                                         // point at the requested entry
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // r10 = message pointer
    emitter.instruction("mov r11, QWORD PTR [r9 + 8]");                         // r11 = message length
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // obj.message_ptr
    emitter.instruction("mov QWORD PTR [rax + 16], r11");                       // obj.message_len
    emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // reload the saved error code for $code field
    emitter.instruction("mov QWORD PTR [rax + 24], rcx");                       // obj.code (matches Exception's `code` property layout)

    emitter.instruction("mov QWORD PTR [rip + _exc_value], rax");               // _exc_value = JsonException pointer
    emitter.instruction("mov rsp, rbp");                                        // unwind the helper scratch frame before tail-call
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before tail-call
    emitter.instruction("jmp __rt_throw_current");                              // tail-call the standard exception unwinder

    emitter.label("__rt_json_throw_error_return_x");
    emitter.instruction("mov rsp, rbp");                                        // unwind the helper scratch frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller without throwing
}
