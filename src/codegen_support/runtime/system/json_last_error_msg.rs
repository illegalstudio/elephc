//! Purpose:
//! Emits runtime helper for `json_last_error_msg()`.
//! Provides the runtime assembly used by JSON builtins on the selected target.
//!
//! Called from:
//! - `crate::codegen_support::runtime::system` during runtime emission.
//!
//! Key details:
//! - Message table indexing must stay in sync with the JSON_ERROR_* constants.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::abi;

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
    emitter.instruction("b __rt_json_error_message");                           // share JSON message formatting with JsonException throws

    emit_message_formatter_aarch64(emitter);
}

/// Emits the shared AArch64 JSON message formatter used by `json_last_error_msg()` and throws.
fn emit_message_formatter_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_json_error_message");
    emitter.instruction("sub sp, sp, #80");                                     // reserve formatter slots for base and appended location fragments
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address across concat/itoa calls
    emitter.instruction("add x29, sp, #64");                                    // establish a stable formatter frame

    // -- load current error code --
    abi::emit_symbol_address(emitter, "x9", "_json_last_error");                // load page of the runtime error-code slot
    emitter.instruction("ldr x10, [x9]");                                       // load the current JSON_ERROR_* code into a scratch register

    // -- bounds check: if code < 0 or code >= count, fall back to code 0 --
    abi::emit_symbol_address(emitter, "x9", "_json_err_msg_count");             // load page of the message-table cardinality
    emitter.instruction("ldr x11, [x9]");                                       // load the message-table cardinality into a scratch register
    emitter.instruction("cmp x10, x11");                                        // compare the requested code against the table cardinality
    emitter.instruction("b.lo 1f");                                             // jump to the in-range branch when the code is below the cardinality
    emitter.instruction("mov x10, #0");                                         // clamp out-of-range codes to JSON_ERROR_NONE
    emitter.label("1");

    // -- index into the (ptr,len) table --
    abi::emit_symbol_address(emitter, "x9", "_json_err_msg_table");             // load page of the per-code (ptr,len) message table
    emitter.instruction("lsl x10, x10, #4");                                    // multiply the code by 16 to step over a (ptr,len) pair
    emitter.instruction("add x9, x9, x10");                                     // advance to the table entry for the requested code
    emitter.instruction("ldr x1, [x9]");                                        // load the message pointer into the string-result pointer register
    emitter.instruction("ldr x2, [x9, #8]");                                    // load the message length into the string-result length register
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save the base message slice for fallback or suffix formatting

    // -- return the base message unless decode recorded a location --
    abi::emit_symbol_address(emitter, "x9", "_json_last_error");                // load page of the runtime error-code slot
    emitter.instruction("ldr x10, [x9]");                                       // reload the unclamped JSON error code
    emitter.instruction("cbz x10, __rt_json_error_message_base_a");             // JSON_ERROR_NONE never carries a location suffix
    abi::emit_symbol_address(emitter, "x9", "_json_error_location_active");     // load page of the decode-location active flag
    emitter.instruction("ldr x10, [x9]");                                       // load whether the last error has a stored line/column
    emitter.instruction("cbz x10, __rt_json_error_message_base_a");             // errors without decode locations keep the base PHP message

    // -- append " near location " --
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // reload the base message as the concat left operand
    crate::codegen_support::abi::emit_symbol_address(emitter, "x3", "_json_err_loc_prefix");
    emitter.instruction("mov x4, #15");                                         // length of " near location "
    emitter.instruction("bl __rt_concat");                                      // append the location suffix prefix
    emitter.instruction("stp x1, x2, [sp, #16]");                               // save partial message with suffix prefix

    // -- append line number --
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_json_error_line");
    emitter.instruction("ldr x0, [x9]");                                        // load the stored decode-error line number
    emitter.instruction("bl __rt_itoa");                                        // format the line number as decimal text
    emitter.instruction("stp x1, x2, [sp, #32]");                               // save the line number string
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // reload the partial message as concat left operand
    emitter.instruction("ldp x3, x4, [sp, #32]");                               // use the line number as concat right operand
    emitter.instruction("bl __rt_concat");                                      // append the line number
    emitter.instruction("stp x1, x2, [sp, #16]");                               // save partial message with line number

    // -- append colon --
    crate::codegen_support::abi::emit_symbol_address(emitter, "x3", "_json_err_loc_colon");
    emitter.instruction("mov x4, #1");                                          // length of ":"
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // reload the partial message as concat left operand
    emitter.instruction("bl __rt_concat");                                      // append the line/column separator
    emitter.instruction("stp x1, x2, [sp, #16]");                               // save partial message with separator

    // -- append column number --
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_json_error_column");
    emitter.instruction("ldr x0, [x9]");                                        // load the stored decode-error column number
    emitter.instruction("bl __rt_itoa");                                        // format the column number as decimal text
    emitter.instruction("stp x1, x2, [sp, #32]");                               // save the column number string
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // reload the partial message as concat left operand
    emitter.instruction("ldp x3, x4, [sp, #32]");                               // use the column number as concat right operand
    emitter.instruction("bl __rt_concat");                                      // append the column number
    emitter.instruction("b __rt_json_error_message_done_a");                    // return the formatted location-aware message

    emitter.label("__rt_json_error_message_base_a");
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // return the borrowed base message slice
    emitter.label("__rt_json_error_message_done_a");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release formatter slots
    emitter.instruction("ret");                                                 // return the selected message slice
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
    emitter.instruction("jmp __rt_json_error_message");                         // share JSON message formatting with JsonException throws

    emit_message_formatter_x86_64(emitter);
}

/// Emits the shared x86_64 JSON message formatter used by `json_last_error_msg()` and throws.
fn emit_message_formatter_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_json_error_message");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer for formatter scratch slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable formatter frame
    emitter.instruction("sub rsp, 64");                                         // reserve formatter slots for base and appended location fragments

    abi::emit_load_symbol_to_reg(emitter, "rcx", "_json_last_error", 0);        // load the current JSON_ERROR_* code into a scratch register
    abi::emit_load_symbol_to_reg(emitter, "r8", "_json_err_msg_count", 0);      // load the message-table cardinality into a scratch register
    emitter.instruction("cmp rcx, r8");                                         // compare the requested code against the table cardinality
    emitter.instruction("jb 1f");                                               // jump to the in-range branch when the code is below the cardinality
    emitter.instruction("xor rcx, rcx");                                        // clamp out-of-range codes to JSON_ERROR_NONE
    emitter.label("1");
    abi::emit_symbol_address(emitter, "r9", "_json_err_msg_table");             // materialize the address of the per-code (ptr,len) message table
    emitter.instruction("shl rcx, 4");                                          // multiply the code by 16 to step over a (ptr,len) pair
    emitter.instruction("add r9, rcx");                                         // advance to the table entry for the requested code
    emitter.instruction("mov rax, QWORD PTR [r9]");                             // load the message pointer into the string-result pointer register
    emitter.instruction("mov rdx, QWORD PTR [r9 + 8]");                         // load the message length into the string-result length register
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the base message pointer for fallback or suffix formatting
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the base message length for fallback or suffix formatting

    abi::emit_load_symbol_to_reg(emitter, "rcx", "_json_last_error", 0);        // reload the unclamped JSON error code
    emitter.instruction("test rcx, rcx");                                       // JSON_ERROR_NONE never carries a location suffix
    emitter.instruction("je __rt_json_error_message_base_x");                   // return base message for JSON_ERROR_NONE
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_json_error_location_active", 0); // load whether the last error has a stored line/column
    emitter.instruction("test rcx, rcx");                                       // check whether a decode location is available
    emitter.instruction("je __rt_json_error_message_base_x");                   // errors without decode locations keep the base PHP message

    abi::emit_symbol_address(emitter, "rdi", "_json_err_loc_prefix");           // right operand pointer = " near location "
    emitter.instruction("mov rsi, 15");                                         // right operand length = 15
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // left operand pointer = base message
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // left operand length = base message length
    emitter.instruction("call __rt_concat");                                    // append the location suffix prefix
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save partial message pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // save partial message length

    abi::emit_load_symbol_to_reg(emitter, "rax", "_json_error_line", 0);        // load the stored decode-error line number
    emitter.instruction("call __rt_itoa");                                      // format the line number as decimal text
    emitter.instruction("mov rdi, rax");                                        // right operand pointer = line digits
    emitter.instruction("mov rsi, rdx");                                        // right operand length = line digit count
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // left operand pointer = partial message
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // left operand length = partial message length
    emitter.instruction("call __rt_concat");                                    // append the line number
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save partial message pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // save partial message length

    abi::emit_symbol_address(emitter, "rdi", "_json_err_loc_colon");            // right operand pointer = ":"
    emitter.instruction("mov rsi, 1");                                          // right operand length = 1
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // left operand pointer = partial message
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // left operand length = partial message length
    emitter.instruction("call __rt_concat");                                    // append the line/column separator
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save partial message pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // save partial message length

    abi::emit_load_symbol_to_reg(emitter, "rax", "_json_error_column", 0);      // load the stored decode-error column number
    emitter.instruction("call __rt_itoa");                                      // format the column number as decimal text
    emitter.instruction("mov rdi, rax");                                        // right operand pointer = column digits
    emitter.instruction("mov rsi, rdx");                                        // right operand length = column digit count
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // left operand pointer = partial message
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // left operand length = partial message length
    emitter.instruction("call __rt_concat");                                    // append the column number
    emitter.instruction("jmp __rt_json_error_message_done_x");                  // return the formatted location-aware message

    emitter.label("__rt_json_error_message_base_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the borrowed base message pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // return the borrowed base message length
    emitter.label("__rt_json_error_message_done_x");
    emitter.instruction("mov rsp, rbp");                                        // release formatter slots
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the selected message slice
}
