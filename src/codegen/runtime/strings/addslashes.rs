//! Purpose:
//! Emits the `__rt_addslashes`, `__rt_addslashes_loop` runtime helper assembly for addslashes.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::strings`.
//!
//! Key details:
//! - String helpers use PHP pointer/length pairs and target ABI return registers; heap-backed results must remain refcount-compatible.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_addslashes` runtime helper for PHP's `addslashes()`.
///
/// Escapes single quotes (`'`), double quotes (`"`), and backslashes (`\`)
/// by prefixing each with a backslash. Operates on raw byte strings using
/// PHP's pointer/length ABI convention.
///
/// ## ARM64 ABI (default)
/// - Input: `x1` = source string pointer, `x2` = source string length
/// - Output: `x1` = result string pointer, `x2` = result string length
/// - Uses the concat buffer (`_concat_buf` / `_concat_off`) for output storage
/// - Clobbers: `x8`-`x13`
///
/// ## x86_64 Linux ABI
/// - Input: `rax` = source string pointer, `rdx` = source string length
/// - Output: `rax` = result string pointer, `rdx` = result string length
/// - Uses the concat buffer (`_concat_buf` / `_concat_off`) for output storage
/// - Clobbers: `r8`-`r11`, `rcx`
pub fn emit_addslashes(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_addslashes_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: addslashes ---");
    emitter.label_global("__rt_addslashes");

    // -- set up concat_buf destination --
    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("mov x11, x2");                                         // remaining byte count

    emitter.label("__rt_addslashes_loop");
    emitter.instruction("cbz x11, __rt_addslashes_done");                       // no bytes left → done
    emitter.instruction("ldrb w12, [x1], #1");                                  // load source byte, advance
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining
    // -- check if char needs escaping --
    emitter.instruction("cmp w12, #39");                                        // single quote?
    emitter.instruction("b.eq __rt_addslashes_esc");                            // yes → escape it
    emitter.instruction("cmp w12, #34");                                        // double quote?
    emitter.instruction("b.eq __rt_addslashes_esc");                            // yes → escape it
    emitter.instruction("cmp w12, #92");                                        // backslash?
    emitter.instruction("b.eq __rt_addslashes_esc");                            // yes → escape it
    // -- store unescaped byte --
    emitter.instruction("strb w12, [x9], #1");                                  // store byte as-is
    emitter.instruction("b __rt_addslashes_loop");                              // next byte

    emitter.label("__rt_addslashes_esc");
    emitter.instruction("mov w13, #92");                                        // backslash character
    emitter.instruction("strb w13, [x9], #1");                                  // write escape backslash
    emitter.instruction("strb w12, [x9], #1");                                  // write the original char
    emitter.instruction("b __rt_addslashes_loop");                              // next byte

    emitter.label("__rt_addslashes_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, x2");                                      // advance by result length
    emitter.instruction("str x8, [x6]");                                        // store updated offset
    emitter.instruction("ret");                                                 // return
}

/// Emits the x86_64 Linux variant of `__rt_addslashes`.
///
/// Identical behavior to the ARM64 variant but uses x86_64 System V ABI
/// registers: `rax`/`rdx` for pointer/length, `r8`-`r11` and `rcx` as temporaries.
fn emit_addslashes_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: addslashes ---");
    emitter.label_global("__rt_addslashes");

    emitter.instruction("mov r8, QWORD PTR [rip + _concat_off]");               // load the current concat-buffer absolute offset before appending the escaped string
    emitter.instruction("lea r9, [rip + _concat_buf]");                         // materialize the concat-buffer base pointer for the escaped string write
    emitter.instruction("add r9, r8");                                          // compute the current concat-buffer write pointer from the base plus offset
    emitter.instruction("mov r10, r9");                                         // preserve the escaped-string start pointer for the final result slice
    emitter.instruction("mov rcx, rdx");                                        // track how many source bytes remain to be escaped

    emitter.label("__rt_addslashes_loop");
    emitter.instruction("test rcx, rcx");                                       // have we consumed every byte of the source string?
    emitter.instruction("je __rt_addslashes_done");                             // finish once no source bytes remain
    emitter.instruction("movzx r11d, BYTE PTR [rax]");                          // load the next source byte and widen it for unsigned escape comparisons
    emitter.instruction("add rax, 1");                                          // advance the source pointer after consuming the current byte
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining source-byte count after the load
    emitter.instruction("cmp r11b, 39");                                        // does the source byte equal a single quote?
    emitter.instruction("je __rt_addslashes_esc");                              // prefix single quotes with a backslash escape
    emitter.instruction("cmp r11b, 34");                                        // does the source byte equal a double quote?
    emitter.instruction("je __rt_addslashes_esc");                              // prefix double quotes with a backslash escape
    emitter.instruction("cmp r11b, 92");                                        // does the source byte equal a backslash?
    emitter.instruction("je __rt_addslashes_esc");                              // double existing backslashes in the escaped output
    emitter.instruction("mov BYTE PTR [r9], r11b");                             // copy ordinary bytes directly into the concat buffer without adding an escape prefix
    emitter.instruction("add r9, 1");                                           // advance the concat-buffer write pointer past the copied ordinary byte
    emitter.instruction("jmp __rt_addslashes_loop");                            // continue escaping the remaining source bytes

    emitter.label("__rt_addslashes_esc");
    emitter.instruction("mov BYTE PTR [r9], 92");                               // write the escape backslash before the escaped source byte
    emitter.instruction("mov BYTE PTR [r9 + 1], r11b");                         // write the original source byte after the escape backslash prefix
    emitter.instruction("add r9, 2");                                           // advance the concat-buffer write pointer past the two-byte escape sequence
    emitter.instruction("jmp __rt_addslashes_loop");                            // continue escaping the remaining source bytes

    emitter.label("__rt_addslashes_done");
    emitter.instruction("mov rax, r10");                                        // return the escaped-string start pointer in the x86_64 string result pointer register
    emitter.instruction("mov rdx, r9");                                         // snapshot the final concat-buffer write pointer before computing the escaped result length
    emitter.instruction("sub rdx, r10");                                        // compute the escaped result length from the write pointer minus the start pointer
    emitter.instruction("mov r8, QWORD PTR [rip + _concat_off]");               // reload the previous concat-buffer absolute offset before publishing the appended slice
    emitter.instruction("add r8, rdx");                                         // advance the concat-buffer absolute offset by the escaped result length
    emitter.instruction("mov QWORD PTR [rip + _concat_off], r8");               // publish the updated concat-buffer absolute offset for later writers
    emitter.instruction("ret");                                                 // return to the caller with the escaped string slice in rax/rdx
}
