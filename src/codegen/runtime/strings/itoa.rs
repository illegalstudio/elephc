//! Purpose:
//! Emits the `__rt_itoa`, `__rt_itoa_positive` runtime helper assembly for integer-to-string conversion.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::strings`.
//!
//! Key details:
//! - String helpers use PHP pointer/length pairs and target ABI return registers; heap-backed results must remain refcount-compatible.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Emits the `__rt_itoa` runtime helper: converts a signed 64-bit integer to a decimal string.
/// Uses a 21-byte scratch area in `_concat_buf` written right-to-left, then returns a pointer to
/// the first digit and the total length.
///
/// # ABI
/// - ARM64: input in `x0`, returns pointer in `x1`, length in `x2`.
/// - x86_64 Linux: input in `rax`, returns pointer in `rax`, length in `rdx`.
/// - x86_64 macOS uses `emit_itoa_linux_x86_64` with the same convention.
///
/// # Side effects
/// - Advances `_concat_off` by 21 to reserve the scratch area.
/// - Scratch area is written right-to-left starting 21 bytes past the current concat_buf offset.
pub fn emit_itoa(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_itoa_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: itoa ---");
    emitter.label_global("__rt_itoa");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #16");                                     // allocate 16 bytes on the stack
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer

    // -- get concat_buf write position --
    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current offset into concat_buf
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // compute write position: buf + offset
    emitter.instruction("add x9, x9, #20");                                     // advance to end of 21-byte scratch area (digits written right-to-left)

    // -- initialize counters --
    emitter.instruction("mov x10, #0");                                         // digit count = 0
    emitter.instruction("mov x11, #0");                                         // negative flag = 0 (not negative)

    // -- handle sign --
    emitter.instruction("cmp x0, #0");                                          // check if input is negative
    emitter.instruction("b.ge __rt_itoa_positive");                             // skip negation if >= 0
    emitter.instruction("mov x11, #1");                                         // set negative flag
    emitter.instruction("neg x0, x0");                                          // negate to make value positive

    // -- handle zero special case --
    emitter.label("__rt_itoa_positive");
    emitter.instruction("cbnz x0, __rt_itoa_loop");                             // if value != 0, start digit extraction loop
    emitter.instruction("mov w12, #48");                                        // ASCII '0'
    emitter.instruction("strb w12, [x9]");                                      // store '0' at current position
    emitter.instruction("sub x9, x9, #1");                                      // move write cursor left
    emitter.instruction("mov x10, #1");                                         // digit count = 1
    emitter.instruction("b __rt_itoa_done");                                    // skip to finalization

    // -- extract digits right-to-left via repeated division by 10 --
    emitter.label("__rt_itoa_loop");
    emitter.instruction("cbz x0, __rt_itoa_sign");                              // if quotient is 0, all digits extracted
    emitter.instruction("mov x12, #10");                                        // divisor = 10
    emitter.instruction("udiv x13, x0, x12");                                   // quotient = value / 10
    emitter.instruction("msub x14, x13, x12, x0");                              // remainder = value - (quotient * 10)
    emitter.instruction("add x14, x14, #48");                                   // convert remainder to ASCII digit
    emitter.instruction("strb w14, [x9]");                                      // store digit at current position
    emitter.instruction("sub x9, x9, #1");                                      // move write cursor left (right-to-left)
    emitter.instruction("add x10, x10, #1");                                    // increment digit count
    emitter.instruction("mov x0, x13");                                         // value = quotient for next iteration
    emitter.instruction("b __rt_itoa_loop");                                    // continue extracting digits

    // -- prepend minus sign if negative --
    emitter.label("__rt_itoa_sign");
    emitter.instruction("cbz x11, __rt_itoa_done");                             // skip if not negative
    emitter.instruction("mov w12, #45");                                        // ASCII '-'
    emitter.instruction("strb w12, [x9]");                                      // store minus sign
    emitter.instruction("sub x9, x9, #1");                                      // move cursor left past the sign
    emitter.instruction("add x10, x10, #1");                                    // count the sign in total length

    // -- finalize: update concat_buf offset and return ptr/len --
    emitter.label("__rt_itoa_done");
    emitter.instruction("add x8, x8, #21");                                     // advance concat_off by scratch area size
    emitter.instruction("str x8, [x6]");                                        // store updated offset back to _concat_off
    emitter.instruction("add x1, x9, #1");                                      // result ptr = one past last written position
    emitter.instruction("mov x2, x10");                                         // result length = digit count

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the `__rt_itoa` runtime helper for x86_64 Linux.
///
/// # ABI
/// Input is expected in `rax`; the function returns the string pointer in `rax` and the length
/// in `rdx`. This is the System V AMD64 ABI convention used on Linux.
///
/// # Implementation notes
/// Mirrors the ARM64 logic: writes digits right-to-left into the 21-byte `_concat_buf` scratch
/// area, handles the zero special case, prepends a minus sign for negative values, then updates
/// `_concat_off` before returning.
fn emit_itoa_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: itoa ---");
    emitter.label_global("__rt_itoa");

    // -- set up stack frame --
    emitter.instruction("push rbp");                                            // save the caller frame pointer before using rbp locally
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame pointer for the routine

    // -- get concat_buf write position --
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the current concat buffer offset
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("add r10, r9");                                         // compute the current concat buffer write position
    emitter.instruction("add r10, 20");                                         // advance to the end of the 21-byte scratch area for right-to-left digit writes

    // -- initialize counters --
    emitter.instruction("xor ecx, ecx");                                        // digit count = 0
    emitter.instruction("xor r11d, r11d");                                      // negative flag = 0

    // -- handle sign --
    emitter.instruction("test rax, rax");                                       // check whether the input integer is negative
    emitter.instruction("jns __rt_itoa_positive");                              // skip negation when the input is already non-negative
    emitter.instruction("mov r11d, 1");                                         // remember that we need to prepend a minus sign later
    emitter.instruction("neg rax");                                             // negate the value so the digit loop can use unsigned division

    // -- handle zero special case --
    emitter.label("__rt_itoa_positive");
    emitter.instruction("test rax, rax");                                       // check whether the absolute value is zero
    emitter.instruction("jne __rt_itoa_loop");                                  // start the digit extraction loop when the value is non-zero
    emitter.instruction("mov BYTE PTR [r10], 48");                              // store ASCII '0' into the scratch area
    emitter.instruction("dec r10");                                             // move the write cursor left after the single digit
    emitter.instruction("mov ecx, 1");                                          // digit count = 1 for the zero special case
    emitter.instruction("jmp __rt_itoa_done");                                  // skip the generic digit extraction loop

    // -- extract digits right-to-left via repeated division by 10 --
    emitter.label("__rt_itoa_loop");
    emitter.instruction("mov esi, 10");                                         // divisor = 10 for decimal digit extraction
    emitter.instruction("xor edx, edx");                                        // clear the high dividend half before unsigned division
    emitter.instruction("div rsi");                                             // quotient -> rax, remainder -> rdx
    emitter.instruction("add dl, 48");                                          // convert the decimal remainder to its ASCII digit
    emitter.instruction("mov BYTE PTR [r10], dl");                              // store the digit at the current scratch position
    emitter.instruction("dec r10");                                             // move the write cursor left for the next digit
    emitter.instruction("inc ecx");                                             // increment the output length after storing one digit
    emitter.instruction("test rax, rax");                                       // check whether more quotient digits remain
    emitter.instruction("jne __rt_itoa_loop");                                  // continue until the quotient reaches zero

    // -- prepend minus sign if negative --
    emitter.label("__rt_itoa_sign");
    emitter.instruction("test r11, r11");                                       // check whether the original value was negative
    emitter.instruction("jz __rt_itoa_done");                                   // skip sign emission for non-negative values
    emitter.instruction("mov BYTE PTR [r10], 45");                              // store ASCII '-' before the first digit
    emitter.instruction("dec r10");                                             // move the cursor left past the sign
    emitter.instruction("inc ecx");                                             // count the sign in the returned string length

    // -- finalize: update concat_buf offset and return ptr/len --
    emitter.label("__rt_itoa_done");
    emitter.instruction("add r9, 21");                                          // advance concat_off by the fixed scratch area size
    emitter.instruction("mov QWORD PTR [r8], r9");                              // store the updated concat buffer offset back to global storage
    emitter.instruction("lea rax, [r10 + 1]");                                  // return the string pointer as one byte past the last decremented position
    emitter.instruction("mov rdx, rcx");                                        // return the string length in the second string-result register

    // -- restore frame and return --
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return to the caller with rax=ptr and rdx=len
}
