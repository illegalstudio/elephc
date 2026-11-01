//! Purpose:
//! Emits `__rt_file_get_contents_range`, the runtime helper that applies PHP's
//! `file_get_contents()` `$offset`/`$length` window to bytes that have already been read.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - The EIR lowering of `file_get_contents()` in `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - The window is applied IN PLACE on an owned heap string: the kept bytes are shifted to the
//!   front and the returned length shrinks. Nothing is reallocated, so the kept byte count is
//!   bounded by the bytes that were actually read and a huge `$length` can never size a write.
//! - A negative `$offset` counts from the end. One that reaches before byte zero is php-src's
//!   "Failed to seek to position N in the stream" warning plus `false`, so the buffer is released
//!   and a null pointer is returned for `box_owned_string_or_false_result` to box as `false`.
//! - A null input pointer (the read already failed) passes straight through untouched, so the
//!   original "Failed to open stream" warning stays the only diagnostic.

use crate::codegen_support::abi;
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// The message fragment php-src writes before the requested seek position.
const SEEK_FAILED_PREFIX: &str = "Warning: file_get_contents(): Failed to seek to position ";

/// The message fragment php-src writes after the requested seek position.
const SEEK_FAILED_SUFFIX: &str = " in the stream\n";

/// The `.data` labels and bytes for the seek-failure warning, shared with the runtime data emitter.
///
/// The emitter below derives every length immediate from this table, so the bytes emitted into
/// `.data` and the lengths passed to `__rt_concat` can never drift apart.
pub const FILE_GET_CONTENTS_SEEK_MESSAGES: &[(&str, &str)] = &[
    ("_diag_fgc_seek_prefix_msg", SEEK_FAILED_PREFIX),
    ("_diag_fgc_seek_suffix_msg", SEEK_FAILED_SUFFIX),
    ("_diag_fgc_unseekable_msg", "Warning: file_get_contents(): Stream does not support seeking\n"),
];

/// Emits `__rt_file_get_contents_range` for the active target.
///
/// ## ARM64 ABI
/// - **Input**: `x1` = owned bytes pointer (0 when the read failed), `x2` = byte count,
///   `x3` = `$offset`, `x4` = `$length`, `x5` = 1 when a `$length` was supplied and 0 when it was
///   omitted or `null`
/// - **Output**: `x1` = bytes pointer, `x2` = kept byte count (`x1` = 0 on a failed seek)
///
/// ## x86_64 ABI
/// - **Input**: `rax` = owned bytes pointer, `rdx` = byte count, `rdi` = `$offset`,
///   `rsi` = `$length`, `rcx` = length-present flag
/// - **Output**: `rax` = bytes pointer, `rdx` = kept byte count
pub fn emit_file_get_contents_range(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_file_get_contents_range_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: file_get_contents_range ---");
    emitter.label_global("__rt_file_get_contents_range");

    // Stack layout: [sp, #0] = requested $offset (needed for the warning after heap_free),
    //               [sp, #16] = saved x29/x30.
    emitter.instruction("sub sp, sp, #32");                                     // reserve one spill slot plus the saved frame registers
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the helper frame pointer
    emitter.instruction("str x3, [sp, #0]");                                    // preserve the requested seek position for the failure message
    emitter.instruction("cbz x1, __rt_fgc_range_return");                       // a failed read is already PHP false and needs no window

    // -- resolve the requested start position against PHP's negative-offset rule --
    emitter.instruction("cmp x3, #0");                                          // is the requested offset counted from the end of the data?
    emitter.instruction("add x9, x2, x3");                                      // compute the end-relative start position
    emitter.instruction("csel x9, x9, x3, lt");                                 // x9 = start, end-relative for a negative offset and absolute otherwise
    emitter.instruction("cmp x9, #0");                                          // did the requested position land before the first byte?
    emitter.instruction("b.lt __rt_fgc_range_seek_failed");                     // php-src reports an unreachable seek instead of clamping
    emitter.instruction("cmp x9, x2");                                          // does the start position lie past the last byte?
    emitter.instruction("csel x9, x9, x2, lt");                                 // clamp the start position to the end so an over-large offset yields ""
    emitter.instruction("sub x10, x2, x9");                                     // x10 = bytes still available after the start position

    // -- bound the kept byte count by both $length and the available bytes --
    emitter.instruction("cmp x4, x10");                                         // compare the requested byte count with what is actually available
    emitter.instruction("csel x11, x4, x10, lt");                               // x11 = min($length, available)
    emitter.instruction("cmp x5, #0");                                          // did the caller supply a $length at all?
    emitter.instruction("csel x10, x11, x10, ne");                              // an absent $length keeps every remaining byte
    emitter.instruction("cmp x10, #0");                                         // could the bounded count still be negative?
    emitter.instruction("csel x10, x10, xzr, gt");                              // never keep a negative number of bytes

    // -- slide the kept bytes to the front of the owned buffer --
    emitter.instruction("cbz x9, __rt_fgc_range_trim");                         // a zero start position already has the bytes in place
    emitter.instruction("cbz x10, __rt_fgc_range_trim");                        // an empty window has nothing to move
    emitter.instruction("add x12, x1, x9");                                     // x12 = read cursor at the first kept byte
    emitter.instruction("mov x13, x1");                                         // x13 = write cursor at the front of the buffer
    emitter.instruction("mov x14, x10");                                        // x14 = number of bytes still to move
    emitter.label("__rt_fgc_range_move");
    emitter.instruction("ldrb w15, [x12], #1");                                 // load the next kept byte and advance the read cursor
    emitter.instruction("strb w15, [x13], #1");                                 // store it at the front and advance the write cursor
    emitter.instruction("subs x14, x14, #1");                                   // one fewer byte left to move
    emitter.instruction("b.ne __rt_fgc_range_move");                            // keep moving until the whole window has slid forward

    emitter.label("__rt_fgc_range_trim");
    emitter.instruction("mov x2, x10");                                         // publish the kept byte count as the string length
    emitter.instruction("b __rt_fgc_range_return");                             // the trimmed buffer is the result

    // -- php-src's unreachable-seek warning, then PHP false --
    emitter.label("__rt_fgc_range_seek_failed");
    emitter.instruction("mov x0, x1");                                          // release the fully read buffer the caller will never see
    emitter.instruction("bl __rt_heap_free");                                   // return the read storage before answering with false
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the requested seek position for the message
    emitter.instruction("bl __rt_itoa");                                        // render the requested position as decimal digits
    emitter.instruction("mov x3, x1");                                          // move the digits into the concat right operand
    emitter.instruction("mov x4, x2");                                          // move the digit count into the concat right operand
    abi::emit_symbol_address(emitter, "x1", FILE_GET_CONTENTS_SEEK_MESSAGES[0].0);
    emitter.instruction(&format!("mov x2, #{}", SEEK_FAILED_PREFIX.len()));     // pass the warning prefix byte length to the concat helper
    emitter.instruction("bl __rt_concat");                                      // build "…Failed to seek to position <n>"
    abi::emit_symbol_address(emitter, "x3", FILE_GET_CONTENTS_SEEK_MESSAGES[1].0);
    emitter.instruction(&format!("mov x4, #{}", SEEK_FAILED_SUFFIX.len()));     // pass the warning suffix byte length to the concat helper
    emitter.instruction("bl __rt_concat");                                      // append " in the stream\n" to the warning text
    emitter.instruction("bl __rt_diag_warning");                                // emit or suppress the unreachable-seek warning
    emitter.instruction("mov x1, #0");                                          // a null string pointer asks the caller's boxer for PHP false
    emitter.instruction("mov x2, #0");                                          // clear the unused failure length

    emitter.label("__rt_fgc_range_return");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the windowed pointer/length pair
}

/// Emits the x86_64 System V variant of `__rt_file_get_contents_range`.
///
/// Mirrors the ARM64 logic register for register: the requested seek position is spilled because
/// `__rt_heap_free` and `__rt_itoa` clobber the caller-saved registers, and the kept byte count is
/// bounded by both `$length` and the bytes that remain after the start position before any store
/// happens.
fn emit_file_get_contents_range_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: file_get_contents_range ---");
    emitter.label_global("__rt_file_get_contents_range");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer across the window helper
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the spilled seek position
    emitter.instruction("sub rsp, 16");                                         // reserve one aligned spill slot for the requested seek position
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the requested seek position for the failure message
    emitter.instruction("test rax, rax");                                       // did the read already fail and answer with a null pointer?
    emitter.instruction("jz __rt_fgc_range_return_x86");                        // a failed read is already PHP false and needs no window

    // -- resolve the requested start position against PHP's negative-offset rule --
    emitter.instruction("mov r8, rdx");                                         // stage the byte count for the end-relative start computation
    emitter.instruction("add r8, rdi");                                         // compute the end-relative start position
    emitter.instruction("test rdi, rdi");                                       // is the requested offset counted from the end of the data?
    emitter.instruction("cmovns r8, rdi");                                      // r8 = start, absolute for a non-negative offset
    emitter.instruction("test r8, r8");                                         // did the requested position land before the first byte?
    emitter.instruction("js __rt_fgc_range_seek_failed_x86");                   // php-src reports an unreachable seek instead of clamping
    emitter.instruction("cmp r8, rdx");                                         // does the start position lie past the last byte?
    emitter.instruction("cmovg r8, rdx");                                       // clamp the start position to the end so an over-large offset yields ""
    emitter.instruction("mov r9, rdx");                                         // stage the byte count for the available-bytes computation
    emitter.instruction("sub r9, r8");                                          // r9 = bytes still available after the start position

    // -- bound the kept byte count by both $length and the available bytes --
    emitter.instruction("mov r10, r9");                                         // default the kept byte count to every remaining byte
    emitter.instruction("cmp rsi, r9");                                         // compare the requested byte count with what is actually available
    emitter.instruction("cmovl r10, rsi");                                      // r10 = min($length, available)
    emitter.instruction("test rcx, rcx");                                       // did the caller supply a $length at all?
    emitter.instruction("cmovz r10, r9");                                       // an absent $length keeps every remaining byte
    emitter.instruction("xor r11d, r11d");                                      // materialize zero as the floor for the kept byte count
    emitter.instruction("cmp r10, 0");                                          // could the bounded count still be negative?
    emitter.instruction("cmovl r10, r11");                                      // never keep a negative number of bytes

    // -- slide the kept bytes to the front of the owned buffer --
    emitter.instruction("test r8, r8");                                         // is the window already at the front of the buffer?
    emitter.instruction("jz __rt_fgc_range_trim_x86");                          // a zero start position already has the bytes in place
    emitter.instruction("test r10, r10");                                       // does the window keep any bytes at all?
    emitter.instruction("jz __rt_fgc_range_trim_x86");                          // an empty window has nothing to move
    emitter.instruction("mov rsi, rax");                                        // seed the read cursor from the owned buffer base
    emitter.instruction("add rsi, r8");                                         // advance the read cursor to the first kept byte
    emitter.instruction("mov rdi, rax");                                        // seed the write cursor at the front of the owned buffer
    emitter.instruction("mov rcx, r10");                                        // seed the move counter from the kept byte count
    emitter.label("__rt_fgc_range_move_x86");
    emitter.instruction("mov r11b, BYTE PTR [rsi]");                            // load the next kept byte from the read cursor
    emitter.instruction("mov BYTE PTR [rdi], r11b");                            // store it at the write cursor near the front of the buffer
    emitter.instruction("add rsi, 1");                                          // advance the read cursor past the copied byte
    emitter.instruction("add rdi, 1");                                          // advance the write cursor past the copied byte
    emitter.instruction("sub rcx, 1");                                          // one fewer byte left to move
    emitter.instruction("jnz __rt_fgc_range_move_x86");                         // keep moving until the whole window has slid forward

    emitter.label("__rt_fgc_range_trim_x86");
    emitter.instruction("mov rdx, r10");                                        // publish the kept byte count as the string length
    emitter.instruction("jmp __rt_fgc_range_return_x86");                       // the trimmed buffer is the result

    // -- php-src's unreachable-seek warning, then PHP false --
    emitter.label("__rt_fgc_range_seek_failed_x86");
    emitter.instruction("call __rt_heap_free");                                 // release the fully read buffer the caller will never see
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the requested seek position for the message
    emitter.instruction("call __rt_itoa");                                      // render the requested position as decimal digits
    emitter.instruction("mov rdi, rax");                                        // move the digits into the concat right operand
    emitter.instruction("mov rsi, rdx");                                        // move the digit count into the concat right operand
    abi::emit_symbol_address(emitter, "rax", FILE_GET_CONTENTS_SEEK_MESSAGES[0].0);
    emitter.instruction(&format!("mov rdx, {}", SEEK_FAILED_PREFIX.len()));     // pass the warning prefix byte length to the concat helper
    emitter.instruction("call __rt_concat");                                    // build "…Failed to seek to position <n>"
    abi::emit_symbol_address(emitter, "rdi", FILE_GET_CONTENTS_SEEK_MESSAGES[1].0);
    emitter.instruction(&format!("mov rsi, {}", SEEK_FAILED_SUFFIX.len()));     // pass the warning suffix byte length to the concat helper
    emitter.instruction("call __rt_concat");                                    // append " in the stream\n" to the warning text
    emitter.instruction("mov rdi, rax");                                        // pass the warning text pointer to the diagnostic helper
    emitter.instruction("mov rsi, rdx");                                        // pass the warning text length to the diagnostic helper
    emitter.instruction("call __rt_diag_warning");                              // emit or suppress the unreachable-seek warning
    emitter.instruction("xor eax, eax");                                        // a null string pointer asks the caller's boxer for PHP false
    emitter.instruction("xor edx, edx");                                        // clear the unused failure length

    emitter.label("__rt_fgc_range_return_x86");
    emitter.instruction("add rsp, 16");                                         // release the spill slot used for the seek position
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the windowed pointer/length pair
}
