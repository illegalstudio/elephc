//! Purpose:
//! Emits runtime helper for `json_last_error_msg()`.
//! Provides the runtime assembly used by JSON builtins on the selected target.
//!
//! Called from:
//! - `crate::codegen::runtime::system` during runtime emission.
//!
//! Key details:
//! - Message table indexing must stay in sync with the JSON_ERROR_* constants.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// __rt_json_last_error_msg: read `_json_last_error` and return the
/// PHP-compatible message string for that code.
///
/// Out-of-range codes fall back to the "No error" entry, matching PHP's
/// behaviour on uninitialized error state.
///
/// Output ABI:
///   ARM64: x1 = string ptr, x2 = string len
///   x86_64: rax = string ptr, rdx = string len (string_result_regs)
pub(crate) fn emit_json_last_error_msg(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: json_last_error_msg ---");
    emitter.label_global("__rt_json_last_error_msg");

    // -- load current error code --
    emitter.adrp("x9", "_json_last_error");                                     // load page of the runtime error-code slot
    emitter.add_lo12("x9", "x9", "_json_last_error");                           // resolve absolute address of the runtime error-code slot
    emitter.instruction("ldr x10, [x9]");                                       // load the current JSON_ERROR_* code into a scratch register

    // -- bounds check: if code < 0 or code >= count, fall back to code 0 --
    emitter.adrp("x9", "_json_err_msg_count");                                  // load page of the message-table cardinality
    emitter.add_lo12("x9", "x9", "_json_err_msg_count");                        // resolve absolute address of the message-table cardinality
    emitter.instruction("ldr x11, [x9]");                                       // load the message-table cardinality into a scratch register
    emitter.instruction("cmp x10, x11");                                        // compare the requested code against the table cardinality
    emitter.instruction("b.lo 1f");                                             // jump to the in-range branch when the code is below the cardinality
    emitter.instruction("mov x10, #0");                                         // clamp out-of-range codes to JSON_ERROR_NONE
    emitter.label("1");

    // -- index into the (ptr,len) table --
    emitter.adrp("x9", "_json_err_msg_table");                                  // load page of the per-code (ptr,len) message table
    emitter.add_lo12("x9", "x9", "_json_err_msg_table");                        // resolve absolute address of the per-code (ptr,len) message table
    emitter.instruction("lsl x10, x10, #4");                                    // multiply the code by 16 to step over a (ptr,len) pair
    emitter.instruction("add x9, x9, x10");                                     // advance to the table entry for the requested code
    emitter.instruction("ldr x1, [x9]");                                        // load the message pointer into the string-result pointer register
    emitter.instruction("ldr x2, [x9, #8]");                                    // load the message length into the string-result length register
    emitter.instruction("ret");                                                 // return the borrowed (ptr,len) message slice
}

/// Emits the x86_64-specific implementation of `__rt_json_last_error_msg`.
///
/// Loads the current JSON error code, bounds-checks it against the message-table
/// cardinality, clamps out-of-range codes to zero, indexes into the (ptr, len) table,
/// and returns the message slice via `rax` (pointer) and `rdx` (length).
///
/// ABI: rax = string ptr, rdx = string len (string_result_regs)
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_last_error_msg ---");
    emitter.label_global("__rt_json_last_error_msg");

    emitter.instruction("mov rcx, QWORD PTR [rip + _json_last_error]");         // load the current JSON_ERROR_* code into a scratch register
    emitter.instruction("mov r8, QWORD PTR [rip + _json_err_msg_count]");       // load the message-table cardinality into a scratch register
    emitter.instruction("cmp rcx, r8");                                         // compare the requested code against the table cardinality
    emitter.instruction("jb 1f");                                               // jump to the in-range branch when the code is below the cardinality
    emitter.instruction("xor rcx, rcx");                                        // clamp out-of-range codes to JSON_ERROR_NONE
    emitter.label("1");
    emitter.instruction("lea r9, [rip + _json_err_msg_table]");                 // materialize the address of the per-code (ptr,len) message table
    emitter.instruction("shl rcx, 4");                                          // multiply the code by 16 to step over a (ptr,len) pair
    emitter.instruction("add r9, rcx");                                         // advance to the table entry for the requested code
    emitter.instruction("mov rax, QWORD PTR [r9]");                             // load the message pointer into the string-result pointer register
    emitter.instruction("mov rdx, QWORD PTR [r9 + 8]");                         // load the message length into the string-result length register
    emitter.instruction("ret");                                                 // return the borrowed (ptr,len) message slice
}
