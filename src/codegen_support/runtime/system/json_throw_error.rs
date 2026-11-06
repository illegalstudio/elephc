//! Purpose:
//! Emits JSON error recording and throw-on-error runtime helper.
//! Provides the runtime assembly used by JSON builtins on the selected target.
//!
//! Called from:
//! - `crate::codegen_support::runtime::system` during runtime emission.
//!
//! Key details:
//! - The helper must set `_json_last_error` before optionally constructing JsonException.
//! - Line and column are materialized from the candidate pointer only when an error
//!   is recorded, and the source pointer is cleared before any throw.

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
///   - Returning path clobbers x0, x9-x14 and NZCV (ARM64), or rax, rcx,
///     rdx, r8-r11 and flags (x86_64). Stack and linkage are restored.
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
    emitter.instruction("bl __rt_json_set_error_location");                     // materialize line/column before any throw or return

    // Fast path: when JSON_THROW_ON_ERROR is clear, just return.
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_json_active_flags");
    emitter.instruction("ldr x9, [x9]");                                        // load the active flag bitmask
    emitter.instruction("mov x10, #4194304");                                   // JSON_THROW_ON_ERROR = bit 4194304
    emitter.instruction("tst x9, x10");                                         // is the throw bit set?
    emitter.instruction("b.eq __rt_json_throw_error_return");                   // bail out when throwing is not requested

    // Allocate a 56-byte JsonException payload (class_id + message, code, and previous slots).
    emitter.instruction("mov x0, #56");                                         // size = class_id + message + code + previous slots
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the JsonException payload
    emitter.instruction("mov x9, #6");                                          // heap kind 6 = object
    emitter.instruction("str x9, [x0, #-8]");                                   // tag the allocation as an object in the uniform header
    emitter.instruction("bl __rt_object_handle_acquire");                       // bind the new object to its PHP object handle

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
    crate::codegen_support::sentinels::emit_throwable_creation_line_unknown(emitter, "x0");
    emitter.instruction("str xzr, [x0, #40]");                                  // previous defaults to null

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

    emit_json_note_error_candidate_aarch64(emitter);
    emit_json_set_error_location_aarch64(emitter);
}

/// Emits AArch64 `__rt_json_note_error_candidate`.
///
/// Input `x0` is an absolute pointer into the original decode or validate source,
/// or the exclusive source end for EOF. The helper stores that pointer and
/// does not read the source.
fn emit_json_note_error_candidate_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_note_error_candidate ---");
    emitter.label_global("__rt_json_note_error_candidate");
    crate::codegen_support::abi::emit_store_reg_to_symbol(
        emitter,
        "x0",
        "_json_error_candidate_ptr",
        0,
    );
    emitter.instruction("ret");                                                 // return after the O(1) candidate store
}

/// Emits AArch64 `__rt_json_set_error_location`.
///
/// Reads `_json_error_candidate_ptr` when a nonempty diagnostic source is active,
/// scans only `[source, min(candidate, source+len))`. The byte at `source+len`
/// is never loaded. LF (`\n`) advances the line; CR counts as a column byte.
/// After publishing numeric coordinates the helper clears the source and
/// candidate pointers so a later throw cannot rescan or replace them.
fn emit_json_set_error_location_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_set_error_location ---");
    emitter.label_global("__rt_json_set_error_location");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // save frame pointer and return address while scanning the source
    emitter.instruction("mov x29, sp");                                         // establish a stable frame for location calculation

    // -- load the original diagnostic source, or drop a stale candidate --
    crate::codegen_support::abi::emit_symbol_address(emitter, "x14", "_json_error_source_ptr");
    emitter.instruction("ldr x9, [x14]");                                       // load the original decode or validate source pointer
    emitter.instruction("cbz x9, __rt_json_set_error_location_drop");           // leave numeric coordinates unchanged when no JSON diagnostic source is active
    crate::codegen_support::abi::emit_symbol_address(emitter, "x14", "_json_error_source_len");
    emitter.instruction("ldr x14, [x14]");                                      // load the original source length in bytes
    emitter.instruction("cbz x14, __rt_json_set_error_location_drop");          // empty input has no source location and performs no scan
    crate::codegen_support::abi::emit_symbol_address(emitter, "x0", "_json_error_candidate_ptr");
    emitter.instruction("ldr x0, [x0]");                                        // load the absolute candidate pointer noted during parsing
    emitter.instruction("mov x10, #1");                                         // line counter starts at 1
    emitter.instruction("mov x11, #1");                                         // column counter starts at 1
    emitter.instruction("mov x13, #0");                                         // no source byte has been loaded yet
    emitter.instruction("cmp x0, x9");                                          // is the candidate before or at the source start?
    emitter.instruction("b.ls __rt_json_set_error_location_store");             // clamp out-of-range candidates to 1:1 without a load
    emitter.instruction("add x14, x9, x14");                                    // exclusive end of the original source; do not dereference it
    emitter.instruction("cmp x0, x14");                                         // does the candidate stay inside the original source?
    emitter.instruction("b.ls __rt_json_set_error_location_limit");             // keep an in-range candidate, including one-past-end
    emitter.instruction("mov x0, x14");                                         // clamp a past-the-end candidate to the source end
    emitter.label("__rt_json_set_error_location_limit");

    // -- count lines and Unicode code points before the candidate --
    emitter.label("__rt_json_set_error_location_loop");
    emitter.instruction("cmp x9, x0");                                          // have we reached the exclusive scan limit?
    emitter.instruction("b.ge __rt_json_set_error_location_store");             // stop before loading the candidate or the source end
    emitter.instruction("ldrb w12, [x9], #1");                                  // consume one source byte while advancing the scan pointer
    emitter.instruction("add x13, x13, #1");                                    // count the loaded byte toward the diagnostic total
    emitter.instruction("cmp w12, #10");                                        // newline resets the column and advances the line
    emitter.instruction("b.eq __rt_json_set_error_location_newline");           // handle LF as the JSON line separator
    emitter.instruction("and w12, w12, #0xC0");                                 // classify a UTF-8 continuation byte
    emitter.instruction("cmp w12, #0x80");                                      // continuation bytes do not start another code point
    emitter.instruction("b.eq __rt_json_set_error_location_loop");              // keep the same character column inside a code point
    emitter.instruction("add x11, x11, #1");                                    // advance the one-based character column
    emitter.instruction("b __rt_json_set_error_location_loop");                 // continue scanning toward the candidate pointer
    emitter.label("__rt_json_set_error_location_newline");
    emitter.instruction("add x10, x10, #1");                                    // advance to the next one-based line
    emitter.instruction("mov x11, #1");                                         // reset the one-based column at the start of the next line
    emitter.instruction("b __rt_json_set_error_location_loop");                 // continue scanning after the newline

    // -- publish numeric coordinates, then drop source references --
    emitter.label("__rt_json_set_error_location_store");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x14", "_json_error_line");
    emitter.instruction("str x10, [x14]");                                      // store the calculated one-based line
    crate::codegen_support::abi::emit_symbol_address(emitter, "x14", "_json_error_column");
    emitter.instruction("str x11, [x14]");                                      // store the calculated one-based column
    crate::codegen_support::abi::emit_symbol_address(emitter, "x14", "_json_error_location_active");
    emitter.instruction("mov x12, #1");                                         // mark that the last JSON error has location metadata
    emitter.instruction("str x12, [x14]");                                      // publish the active location flag
    crate::codegen_support::abi::emit_symbol_address(emitter, "x14", "_json_error_scan_count");
    emitter.instruction("ldr x12, [x14]");                                      // load the number of completed location scans
    emitter.instruction("add x12, x12, #1");                                    // count this scan once
    emitter.instruction("str x12, [x14]");                                      // publish the updated scan count
    crate::codegen_support::abi::emit_symbol_address(emitter, "x14", "_json_error_scan_bytes");
    emitter.instruction("ldr x12, [x14]");                                      // load the number of bytes loaded by location scans
    emitter.instruction("add x12, x12, x13");                                   // add only the bytes this scan loaded
    emitter.instruction("str x12, [x14]");                                      // publish the updated scanned-byte total
    emitter.instruction("b __rt_json_set_error_location_clear");                // drop the source after the coordinates are stored

    emitter.label("__rt_json_set_error_location_drop");
    emitter.label("__rt_json_set_error_location_clear");
    crate::codegen_support::abi::emit_store_zero_to_symbol(emitter, "_json_error_source_ptr", 0);
    crate::codegen_support::abi::emit_store_zero_to_symbol(emitter, "_json_error_source_len", 0);
    crate::codegen_support::abi::emit_store_zero_to_symbol(emitter, "_json_error_candidate_ptr", 0);
    emitter.label("__rt_json_set_error_location_done");
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return to the JSON error path
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
    emitter.instruction("call __rt_json_set_error_location");                   // materialize line/column before any throw or return
    abi::emit_load_symbol_to_reg(emitter, "rdx", "_json_active_flags", 0);      // load the active flag bitmask
    emitter.instruction("test rdx, 0x400000");                                  // JSON_THROW_ON_ERROR = 0x400000
    emitter.instruction("je __rt_json_throw_error_return_x");                   // bail out when throwing is not requested

    emitter.instruction("mov rax, 56");                                         // size = class_id + message + code + previous slots
    emitter.instruction("call __rt_heap_alloc");                                // allocate the JsonException payload (rax = payload ptr)
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(6))); // stamp the canonical x86_64 heap-kind word (magic + kind 6 throwable)
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // tag the allocation as an object in the uniform header
    emitter.instruction("call __rt_object_handle_acquire");                     // bind the new object to its PHP object handle

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
    crate::codegen_support::sentinels::emit_throwable_creation_line_unknown(emitter, "rax");
    emitter.instruction("mov QWORD PTR [rax + 40], 0");                         // previous defaults to null

    abi::emit_store_reg_to_symbol(emitter, "rax", "_exc_value", 0);             // _exc_value = JsonException pointer
    emitter.instruction("mov rsp, rbp");                                        // unwind the helper scratch frame before tail-call
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before tail-call
    emitter.instruction("jmp __rt_throw_current");                              // tail-call the standard exception unwinder

    emitter.label("__rt_json_throw_error_return_x");
    emitter.instruction("mov rsp, rbp");                                        // unwind the helper scratch frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller without throwing

    emit_json_note_error_candidate_x86_64(emitter);
    emit_json_set_error_location_x86_64(emitter);
}

/// Emits x86_64 `__rt_json_note_error_candidate`.
///
/// Input `rax` is an absolute pointer into the original decode or validate source,
/// or the exclusive source end for EOF. The helper stores that pointer and
/// does not read the source.
fn emit_json_note_error_candidate_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_note_error_candidate ---");
    emitter.label_global("__rt_json_note_error_candidate");
    abi::emit_store_reg_to_symbol(emitter, "rax", "_json_error_candidate_ptr", 0);
    emitter.instruction("ret");                                                 // return after the O(1) candidate store
}

/// Emits x86_64 `__rt_json_set_error_location`.
///
/// Reads `_json_error_candidate_ptr` when a nonempty diagnostic source is active,
/// scans only `[source, min(candidate, source+len))`. The byte at `source+len`
/// is never loaded. LF (`\n`) advances the line; CR counts as a column byte.
/// After publishing numeric coordinates the helper clears the source and
/// candidate pointers so a later throw cannot rescan or replace them.
fn emit_json_set_error_location_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_set_error_location ---");
    emitter.label_global("__rt_json_set_error_location");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while scanning the source
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame for location calculation

    // -- load the original diagnostic source, or drop a stale candidate --
    abi::emit_load_symbol_to_reg(emitter, "r8", "_json_error_source_ptr", 0);   // load the original decode or validate source pointer
    emitter.instruction("test r8, r8");                                         // check whether a JSON operation published an input source pointer
    emitter.instruction("je __rt_json_set_error_location_drop_x");              // leave numeric coordinates unchanged when no JSON diagnostic source is active
    abi::emit_load_symbol_to_reg(emitter, "rax", "_json_error_source_len", 0);  // load the original source length in bytes
    emitter.instruction("test rax, rax");                                       // empty input has no source location
    emitter.instruction("je __rt_json_set_error_location_drop_x");              // skip the scan and keep the cleared coordinates
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_json_error_candidate_ptr", 0); // load the absolute candidate pointer noted during parsing
    emitter.instruction("mov r9, 1");                                           // line counter starts at 1
    emitter.instruction("mov r10, 1");                                          // column counter starts at 1
    emitter.instruction("xor r11, r11");                                        // no source byte has been loaded yet
    emitter.instruction("cmp rcx, r8");                                         // is the candidate before or at the source start?
    emitter.instruction("jbe __rt_json_set_error_location_store_x");            // clamp out-of-range candidates to 1:1 without a load
    emitter.instruction("add rax, r8");                                         // exclusive end of the original source; do not dereference it
    emitter.instruction("cmp rcx, rax");                                        // does the candidate stay inside the original source?
    emitter.instruction("jbe __rt_json_set_error_location_limit_x");            // keep an in-range candidate, including one-past-end
    emitter.instruction("mov rcx, rax");                                        // clamp a past-the-end candidate to the source end
    emitter.label("__rt_json_set_error_location_limit_x");

    // -- count lines and Unicode code points before the candidate --
    emitter.instruction("mov rax, r8");                                         // scan pointer starts at the original source
    emitter.label("__rt_json_set_error_location_loop_x");
    emitter.instruction("cmp rax, rcx");                                        // have we reached the exclusive scan limit?
    emitter.instruction("jae __rt_json_set_error_location_store_x");            // stop before loading the candidate or the source end
    emitter.instruction("movzx rdx, BYTE PTR [rax]");                           // load one source byte for line/column accounting
    emitter.instruction("add rax, 1");                                          // advance the scan pointer past the consumed byte
    emitter.instruction("add r11, 1");                                          // count the loaded byte toward the diagnostic total
    emitter.instruction("cmp rdx, 10");                                         // newline resets the column and advances the line
    emitter.instruction("je __rt_json_set_error_location_newline_x");           // handle LF as the JSON line separator
    emitter.instruction("and edx, 0xC0");                                       // classify a UTF-8 continuation byte
    emitter.instruction("cmp edx, 0x80");                                       // continuation bytes do not start another code point
    emitter.instruction("je __rt_json_set_error_location_loop_x");              // keep the same character column inside a code point
    emitter.instruction("add r10, 1");                                          // advance the one-based character column
    emitter.instruction("jmp __rt_json_set_error_location_loop_x");             // continue scanning toward the candidate pointer
    emitter.label("__rt_json_set_error_location_newline_x");
    emitter.instruction("add r9, 1");                                           // advance to the next one-based line
    emitter.instruction("mov r10, 1");                                          // reset the one-based column at the start of the next line
    emitter.instruction("jmp __rt_json_set_error_location_loop_x");             // continue scanning after the newline

    // -- publish numeric coordinates, then drop source references --
    emitter.label("__rt_json_set_error_location_store_x");
    abi::emit_store_reg_to_symbol(emitter, "r9", "_json_error_line", 0);        // store the calculated one-based line
    abi::emit_store_reg_to_symbol(emitter, "r10", "_json_error_column", 0);     // store the calculated one-based column
    abi::emit_store_imm_to_symbol(emitter, "_json_error_location_active", 0, 1); // publish that the last JSON error has location metadata
    abi::emit_load_symbol_to_reg(emitter, "rdx", "_json_error_scan_count", 0);  // load the number of completed location scans
    emitter.instruction("add rdx, 1");                                          // count this scan once
    abi::emit_store_reg_to_symbol(emitter, "rdx", "_json_error_scan_count", 0); // publish the updated scan count
    abi::emit_load_symbol_to_reg(emitter, "rdx", "_json_error_scan_bytes", 0);  // load the number of bytes loaded by location scans
    emitter.instruction("add rdx, r11");                                        // add only the bytes this scan loaded
    abi::emit_store_reg_to_symbol(emitter, "rdx", "_json_error_scan_bytes", 0); // publish the updated scanned-byte total
    emitter.instruction("jmp __rt_json_set_error_location_clear_x");            // drop the source after the coordinates are stored

    emitter.label("__rt_json_set_error_location_drop_x");
    emitter.label("__rt_json_set_error_location_clear_x");
    abi::emit_store_zero_to_symbol(emitter, "_json_error_source_ptr", 0);
    abi::emit_store_zero_to_symbol(emitter, "_json_error_source_len", 0);
    abi::emit_store_zero_to_symbol(emitter, "_json_error_candidate_ptr", 0);
    emitter.label("__rt_json_set_error_location_done_x");
    emitter.instruction("mov rsp, rbp");                                        // release location helper frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to the JSON error path
}
