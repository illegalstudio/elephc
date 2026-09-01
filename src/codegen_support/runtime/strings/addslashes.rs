//! Purpose:
//! Emits the `__rt_addslashes`, `__rt_addslashes_loop` runtime helper assembly for addslashes.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - String helpers use PHP pointer/length pairs and target ABI return registers; heap-backed results must remain refcount-compatible.
//! - The worst-case `2 * len` escaped result is reserved through `__rt_concat_reserve` before
//!   the first store, so long inputs fall back to heap storage instead of running off the end
//!   of the 64 KiB concat scratch buffer.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_addslashes` runtime helper for PHP's `addslashes()`.
///
/// Escapes NUL as `\0` and prefixes single quotes (`'`), double quotes (`"`),
/// and backslashes (`\`) with a backslash. Operates on raw byte strings using
/// PHP's pointer/length ABI convention.
///
/// ## ARM64 ABI (default)
/// - Input: `x1` = source string pointer, `x2` = source string length
/// - Output: `x1` = result string pointer, `x2` = result string length
/// - Reserves the worst-case `2 * len` expansion through `__rt_concat_reserve` (concat scratch
///   while it fits, owned heap storage otherwise) and finishes through `__rt_concat_publish`.
///
/// ## x86_64 Linux ABI
/// - Input: `rax` = source string pointer, `rdx` = source string length
/// - Output: `rax` = result string pointer, `rdx` = result string length
/// - Same reservation contract as the ARM64 path.
///
/// Both paths clobber every caller-saved register, because the reservation can reach
/// `__rt_heap_alloc`, and a wrapped `2 * len` product reports PHP's allocation-overflow fatal.
pub fn emit_addslashes(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_addslashes_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: addslashes ---");
    emitter.label_global("__rt_addslashes");

    // -- reserve the worst-case two-bytes-per-input-byte escaped result before writing anything --
    emitter.instruction("sub sp, sp, #32");                                     // allocate spill space for the borrowed source string
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address across the reservation call
    emitter.instruction("add x29, sp, #16");                                    // establish the addslashes helper frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save the source pointer and length across the reservation call
    emitter.instruction("adds x0, x2, x2");                                     // compute the worst-case escaped result size and record unsigned wrap
    emitter.instruction("b.cs __rt_addslashes_size_overflow");                  // reject a wrapped size instead of reserving a too-small destination
    emitter.instruction("bl __rt_concat_reserve");                              // reserve scratch or heap storage for the escaped result
    emitter.instruction("mov x9, x0");                                          // destination pointer
    emitter.instruction("mov x10, x0");                                         // save result start
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload the borrowed source pointer and length
    emitter.instruction("mov x11, x2");                                         // remaining byte count

    emitter.label("__rt_addslashes_loop");
    emitter.instruction("cbz x11, __rt_addslashes_done");                       // no bytes left → done
    emitter.instruction("ldrb w12, [x1], #1");                                  // load source byte, advance
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining
    // -- check if char needs escaping --
    emitter.instruction("cbz w12, __rt_addslashes_nul");                         // NUL becomes the two printable bytes `\0`
    emitter.instruction("cmp w12, #39");                                        // single quote?
    emitter.instruction("b.eq __rt_addslashes_esc");                            // yes → escape it
    emitter.instruction("cmp w12, #34");                                        // double quote?
    emitter.instruction("b.eq __rt_addslashes_esc");                            // yes → escape it
    emitter.instruction("cmp w12, #92");                                        // backslash?
    emitter.instruction("b.eq __rt_addslashes_esc");                            // yes → escape it
    // -- store unescaped byte --
    emitter.instruction("strb w12, [x9], #1");                                  // store byte as-is
    emitter.instruction("b __rt_addslashes_loop");                              // next byte

    emitter.label("__rt_addslashes_nul");
    emitter.instruction("mov w13, #92");                                        // backslash character
    emitter.instruction("strb w13, [x9], #1");                                  // write the NUL escape prefix
    emitter.instruction("mov w13, #48");                                        // ASCII `0`
    emitter.instruction("strb w13, [x9], #1");                                  // finish the printable `\0` escape
    emitter.instruction("b __rt_addslashes_loop");                              // next byte

    emitter.label("__rt_addslashes_esc");
    emitter.instruction("mov w13, #92");                                        // backslash character
    emitter.instruction("strb w13, [x9], #1");                                  // write escape backslash
    emitter.instruction("strb w12, [x9], #1");                                  // write the original char
    emitter.instruction("b __rt_addslashes_loop");                              // next byte

    emitter.label("__rt_addslashes_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("bl __rt_concat_publish");                              // advance the concat scratch offset only for scratch-backed results
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the addslashes helper frame
    emitter.instruction("ret");                                                 // return

    // -- impossible result size: report the shared allocation-overflow fatal error --
    emitter.label("__rt_addslashes_size_overflow");
    emitter.instruction("b __rt_alloc_overflow");                               // unconditional branch keeps the fatal trampoline cross-atom safe
}

/// Emits the x86_64 Linux variant of `__rt_addslashes`.
///
/// Identical behavior to the ARM64 variant but uses x86_64 System V ABI
/// registers: `rax`/`rdx` for pointer/length, `r8`-`r11` and `rcx` as temporaries.
/// Reserves the worst-case `2 * len` expansion through `__rt_concat_reserve` and publishes the
/// written length through `__rt_concat_publish`.
fn emit_addslashes_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: addslashes ---");
    emitter.label_global("__rt_addslashes");

    // -- reserve the worst-case two-bytes-per-input-byte escaped result before writing anything --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer across the reservation and publish calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the borrowed source string
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots for the source pointer and length
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the borrowed source pointer across the reservation call
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the borrowed source length across the reservation call
    emitter.instruction("imul rax, rdx, 2");                                    // compute the worst-case escaped result size as 2 * source length
    emitter.instruction("jo __rt_addslashes_size_overflow_linux_x86_64");       // reject a wrapped size instead of reserving a too-small destination
    emitter.instruction("call __rt_concat_reserve");                            // reserve scratch or heap storage for the escaped result
    emitter.instruction("mov r9, rax");                                         // compute the destination write pointer where the escaped string begins
    emitter.instruction("mov r10, r9");                                         // preserve the escaped-string start pointer for the final result slice
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // track how many source bytes remain to be escaped
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the borrowed source cursor the escape loop advances through

    emitter.label("__rt_addslashes_loop");
    emitter.instruction("test rcx, rcx");                                       // have we consumed every byte of the source string?
    emitter.instruction("je __rt_addslashes_done");                             // finish once no source bytes remain
    emitter.instruction("movzx r11d, BYTE PTR [rax]");                          // load the next source byte and widen it for unsigned escape comparisons
    emitter.instruction("add rax, 1");                                          // advance the source pointer after consuming the current byte
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining source-byte count after the load
    emitter.instruction("test r11b, r11b");                                     // does the source byte equal NUL?
    emitter.instruction("je __rt_addslashes_nul");                              // render NUL as the printable two-byte `\0` escape
    emitter.instruction("cmp r11b, 39");                                        // does the source byte equal a single quote?
    emitter.instruction("je __rt_addslashes_esc");                              // prefix single quotes with a backslash escape
    emitter.instruction("cmp r11b, 34");                                        // does the source byte equal a double quote?
    emitter.instruction("je __rt_addslashes_esc");                              // prefix double quotes with a backslash escape
    emitter.instruction("cmp r11b, 92");                                        // does the source byte equal a backslash?
    emitter.instruction("je __rt_addslashes_esc");                              // double existing backslashes in the escaped output
    emitter.instruction("mov BYTE PTR [r9], r11b");                             // copy ordinary bytes directly into the concat buffer without adding an escape prefix
    emitter.instruction("add r9, 1");                                           // advance the concat-buffer write pointer past the copied ordinary byte
    emitter.instruction("jmp __rt_addslashes_loop");                            // continue escaping the remaining source bytes

    emitter.label("__rt_addslashes_nul");
    emitter.instruction("mov BYTE PTR [r9], 92");                               // write the NUL escape backslash
    emitter.instruction("mov BYTE PTR [r9 + 1], 48");                          // write ASCII `0` after the escape prefix
    emitter.instruction("add r9, 2");                                           // advance past the printable `\0` escape
    emitter.instruction("jmp __rt_addslashes_loop");                            // continue escaping the remaining source bytes

    emitter.label("__rt_addslashes_esc");
    emitter.instruction("mov BYTE PTR [r9], 92");                               // write the escape backslash before the escaped source byte
    emitter.instruction("mov BYTE PTR [r9 + 1], r11b");                         // write the original source byte after the escape backslash prefix
    emitter.instruction("add r9, 2");                                           // advance the concat-buffer write pointer past the two-byte escape sequence
    emitter.instruction("jmp __rt_addslashes_loop");                            // continue escaping the remaining source bytes

    emitter.label("__rt_addslashes_done");
    emitter.instruction("mov rax, r10");                                        // return the escaped-string start pointer in the x86_64 string result pointer register
    emitter.instruction("mov rdx, r9");                                         // snapshot the final destination write pointer before computing the escaped result length
    emitter.instruction("sub rdx, r10");                                        // compute the escaped result length from the write pointer minus the start pointer
    emitter.instruction("call __rt_concat_publish");                            // advance the concat scratch offset only for scratch-backed results
    emitter.instruction("add rsp, 32");                                         // release the addslashes spill slots before returning the escaped string
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the escaped string
    emitter.instruction("ret");                                                 // return to the caller with the escaped string slice in rax/rdx

    // -- impossible result size: report the shared allocation-overflow fatal error --
    emitter.label("__rt_addslashes_size_overflow_linux_x86_64");
    emitter.instruction("jmp __rt_alloc_overflow");                             // unconditional branch keeps the fatal trampoline reachable from every caller
}
