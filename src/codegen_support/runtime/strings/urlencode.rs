//! Purpose:
//! Emits the `__rt_urlencode`, `__rt_urlencode_loop` runtime helper assembly for urlencode.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - URL encoding helpers are emitted byte scanners that must preserve PHP escaping rules for supported encodings.
//! - The worst-case `3 * len` percent-encoded result is reserved through `__rt_concat_reserve`
//!   before the first store, so long inputs fall back to heap storage instead of running off
//!   the end of the 64 KiB concat scratch buffer.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// urlencode: percent-encode non-alphanumeric chars except -_. and space->+.
/// Input: x1/x2=string. Output: x1/x2=result.
/// Reserves the worst-case `3 * len` expansion through `__rt_concat_reserve` (concat scratch
/// while it fits, owned heap storage otherwise) and finishes through `__rt_concat_publish`.
pub fn emit_urlencode(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_urlencode_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: urlencode ---");
    emitter.label_global("__rt_urlencode");

    // -- reserve the worst-case three-bytes-per-input-byte result before writing anything --
    emitter.instruction("sub sp, sp, #32");                                     // allocate spill space for the borrowed source string
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address across the reservation call
    emitter.instruction("add x29, sp, #16");                                    // establish the urlencode helper frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save the source pointer and length across the reservation call
    emitter.instruction("mov x9, #3");                                          // worst-case percent-encoded expansion factor
    emitter.instruction("umulh x10, x2, x9");                                   // capture the high half of the 3 * length product
    emitter.instruction("cbnz x10, __rt_urlencode_size_overflow");              // reject a wrapped size instead of reserving a too-small destination
    emitter.instruction("mul x0, x2, x9");                                      // compute the worst-case percent-encoded result size
    emitter.instruction("bl __rt_concat_reserve");                              // reserve scratch or heap storage for the percent-encoded result
    emitter.instruction("mov x9, x0");                                          // destination pointer
    emitter.instruction("mov x10, x0");                                         // save result start
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload the borrowed source pointer and length
    emitter.instruction("mov x11, x2");                                         // remaining byte count

    emitter.label("__rt_urlencode_loop");
    emitter.instruction("cbz x11, __rt_urlencode_done");                        // no bytes left -> done
    emitter.instruction("ldrb w12, [x1], #1");                                  // load source byte, advance
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining

    // -- check if space -> '+' --
    emitter.instruction("cmp w12, #32");                                        // is it space?
    emitter.instruction("b.ne __rt_urlencode_chk_alnum");                       // no -> check alphanumeric
    emitter.instruction("mov w13, #43");                                        // '+' character
    emitter.instruction("strb w13, [x9], #1");                                  // write '+'
    emitter.instruction("b __rt_urlencode_loop");                               // next byte

    // -- check alphanumeric and safe chars --
    //
    // IN ASCII ORDER, and that is the whole point: each "below this range" branch has to fall to
    // the NEXT range, not to the punctuation check at the bottom. Testing 'A' first sent every
    // digit — which is below 'A' — straight past its own arm to a check that only knows `-`, `_`
    // and `.`, so `urlencode("0123456789")` answered `%30%31...` and the 0-9 arm was unreachable.
    emitter.label("__rt_urlencode_chk_alnum");
    // -- check 0-9 --
    emitter.instruction("cmp w12, #48");                                        // >= '0'?
    emitter.instruction("b.lt __rt_urlencode_chk_safe");                        // below every alphanumeric range
    emitter.instruction("cmp w12, #57");                                        // <= '9'?
    emitter.instruction("b.le __rt_urlencode_passthru");                        // yes -> pass through
    // -- check A-Z --
    emitter.instruction("cmp w12, #65");                                        // >= 'A'?
    emitter.instruction("b.lt __rt_urlencode_chk_safe");                        // between '9' and 'A'
    emitter.instruction("cmp w12, #90");                                        // <= 'Z'?
    emitter.instruction("b.le __rt_urlencode_passthru");                        // yes -> pass through
    // -- check a-z --
    emitter.instruction("cmp w12, #97");                                        // >= 'a'?
    emitter.instruction("b.lt __rt_urlencode_chk_safe");                        // between 'Z' and 'a'
    emitter.instruction("cmp w12, #122");                                       // <= 'z'?
    emitter.instruction("b.le __rt_urlencode_passthru");                        // yes -> pass through

    // -- check safe chars: - (45), _ (95), . (46) --
    emitter.label("__rt_urlencode_chk_safe");
    emitter.instruction("cmp w12, #45");                                        // is it '-'?
    emitter.instruction("b.eq __rt_urlencode_passthru");                        // yes -> pass through
    emitter.instruction("cmp w12, #95");                                        // is it '_'?
    emitter.instruction("b.eq __rt_urlencode_passthru");                        // yes -> pass through
    emitter.instruction("cmp w12, #46");                                        // is it '.'?
    emitter.instruction("b.eq __rt_urlencode_passthru");                        // yes -> pass through

    // -- percent-encode: write %XX --
    emitter.instruction("mov w13, #37");                                        // '%' character
    emitter.instruction("strb w13, [x9], #1");                                  // write '%'
    // -- high nibble --
    emitter.instruction("lsr w13, w12, #4");                                    // extract high 4 bits
    emitter.instruction("cmp w13, #10");                                        // >= 10?
    emitter.instruction("b.ge __rt_urlencode_hi_af");                           // yes -> use A-F
    emitter.instruction("add w13, w13, #48");                                   // convert 0-9 to '0'-'9'
    emitter.instruction("b __rt_urlencode_hi_st");                              // store
    emitter.label("__rt_urlencode_hi_af");
    emitter.instruction("add w13, w13, #55");                                   // convert 10-15 to 'A'-'F'
    emitter.label("__rt_urlencode_hi_st");
    emitter.instruction("strb w13, [x9], #1");                                  // write high nibble hex char
    // -- low nibble --
    emitter.instruction("and w13, w12, #0xf");                                  // extract low 4 bits
    emitter.instruction("cmp w13, #10");                                        // >= 10?
    emitter.instruction("b.ge __rt_urlencode_lo_af");                           // yes -> use A-F
    emitter.instruction("add w13, w13, #48");                                   // convert 0-9 to '0'-'9'
    emitter.instruction("b __rt_urlencode_lo_st");                              // store
    emitter.label("__rt_urlencode_lo_af");
    emitter.instruction("add w13, w13, #55");                                   // convert 10-15 to 'A'-'F'
    emitter.label("__rt_urlencode_lo_st");
    emitter.instruction("strb w13, [x9], #1");                                  // write low nibble hex char
    emitter.instruction("b __rt_urlencode_loop");                               // next byte

    // -- pass through byte unchanged --
    emitter.label("__rt_urlencode_passthru");
    emitter.instruction("strb w12, [x9], #1");                                  // store byte as-is
    emitter.instruction("b __rt_urlencode_loop");                               // next byte

    emitter.label("__rt_urlencode_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("bl __rt_concat_publish");                              // advance the concat scratch offset only for scratch-backed results
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the urlencode helper frame
    emitter.instruction("ret");                                                 // return

    // -- impossible result size: report the shared allocation-overflow fatal error --
    emitter.label("__rt_urlencode_size_overflow");
    emitter.instruction("b __rt_alloc_overflow");                               // unconditional branch keeps the fatal trampoline cross-atom safe
}

/// Emits x86_64 Linux–specific urlencode runtime helper.
/// Consumes source string in rax/rdx, writes percent-encoded output to the concat buffer,
/// updates `_concat_off`, and returns the result pointer/length in rax/rdx.
fn emit_urlencode_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: urlencode ---");
    emitter.label_global("__rt_urlencode");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer across the reservation and publish calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the borrowed source string
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots for the source pointer and length
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the borrowed source pointer across the reservation call
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the borrowed source length across the reservation call
    emitter.instruction("imul rax, rdx, 3");                                    // compute the worst-case percent-encoded result size as 3 * source length
    emitter.instruction("jo __rt_urlencode_size_overflow_linux_x86_64");        // reject a wrapped size instead of reserving a too-small destination
    emitter.instruction("call __rt_concat_reserve");                            // reserve scratch or heap storage for the percent-encoded result
    emitter.instruction("mov r11, rax");                                        // compute the destination pointer where the urlencoded string begins
    emitter.instruction("mov r8, r11");                                         // preserve the result start pointer for the returned string value after the loop mutates the destination cursor
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // seed the remaining source length counter from the borrowed input string length
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // preserve the borrowed source string cursor in a dedicated register before the loop mutates caller-saved registers

    emitter.label("__rt_urlencode_loop_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // stop once every source byte has been classified and copied or percent-encoded into concat storage
    emitter.instruction("jz __rt_urlencode_done_linux_x86_64");                 // finish once the full borrowed source string has been consumed
    emitter.instruction("mov dl, BYTE PTR [rsi]");                              // load one source byte before deciding whether urlencode() must encode it
    emitter.instruction("add rsi, 1");                                          // advance the borrowed source string cursor after consuming one byte
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining source length after consuming one byte
    emitter.instruction("cmp dl, 32");                                          // is the current source byte a space that query-style urlencode() should turn into '+'?
    emitter.instruction("jne __rt_urlencode_chk_alnum_linux_x86_64");           // continue with the safe-byte checks when the current byte is not a space
    emitter.instruction("mov BYTE PTR [r11], 43");                              // write '+' for a space because query-style urlencode() maps spaces to plus signs
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting one query-style space replacement
    emitter.instruction("jmp __rt_urlencode_loop_linux_x86_64");                // continue encoding the remainder of the source string after replacing one space

    // See the AArch64 arm: the ranges are tested IN ASCII ORDER so each "below" branch falls to
    // the next range instead of to the punctuation check, which is what made every digit
    // percent-encoded.
    emitter.label("__rt_urlencode_chk_alnum_linux_x86_64");
    emitter.instruction("cmp dl, 48");                                          // is the current source byte at least '0', which could make it a decimal digit safe to pass through?
    emitter.instruction("jb __rt_urlencode_chk_safe_linux_x86_64");             // below every alphanumeric range, so only the punctuation set can save it
    emitter.instruction("cmp dl, 57");                                          // is the current source byte at most '9', which keeps it inside the decimal-digit safe range?
    emitter.instruction("jbe __rt_urlencode_passthru_linux_x86_64");            // pass decimal digits straight through without percent-encoding them
    emitter.instruction("cmp dl, 65");                                          // is the current source byte at least 'A', which could make it an ASCII letter safe to pass through?
    emitter.instruction("jb __rt_urlencode_chk_safe_linux_x86_64");             // between '9' and 'A'
    emitter.instruction("cmp dl, 90");                                          // is the current source byte at most 'Z', which keeps it inside the uppercase ASCII safe range?
    emitter.instruction("jbe __rt_urlencode_passthru_linux_x86_64");            // pass uppercase ASCII letters straight through without percent-encoding them
    emitter.instruction("cmp dl, 97");                                          // is the current source byte at least 'a', which could make it a lowercase ASCII letter safe to pass through?
    emitter.instruction("jb __rt_urlencode_chk_safe_linux_x86_64");             // between 'Z' and 'a'
    emitter.instruction("cmp dl, 122");                                         // is the current source byte at most 'z', which keeps it inside the lowercase ASCII safe range?
    emitter.instruction("jbe __rt_urlencode_passthru_linux_x86_64");            // pass lowercase ASCII letters straight through without percent-encoding them

    emitter.label("__rt_urlencode_chk_safe_linux_x86_64");
    emitter.instruction("cmp dl, 45");                                          // is the current source byte '-' which query-style urlencode() leaves untouched?
    emitter.instruction("je __rt_urlencode_passthru_linux_x86_64");             // pass '-' straight through because it is part of the safe punctuation set
    emitter.instruction("cmp dl, 95");                                          // is the current source byte '_' which query-style urlencode() leaves untouched?
    emitter.instruction("je __rt_urlencode_passthru_linux_x86_64");             // pass '_' straight through because it is part of the safe punctuation set
    emitter.instruction("cmp dl, 46");                                          // is the current source byte '.' which query-style urlencode() leaves untouched?
    emitter.instruction("je __rt_urlencode_passthru_linux_x86_64");             // pass '.' straight through because it is part of the safe punctuation set

    emitter.instruction("mov BYTE PTR [r11], 37");                              // write '%' as the first byte of a percent-encoded expansion for an unsafe source byte
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the leading '%' of one percent-encoded byte
    emitter.instruction("movzx r10d, dl");                                      // widen the source byte before extracting its hexadecimal nibbles for the percent-encoded expansion
    emitter.instruction("shr r10b, 4");                                         // isolate the high nibble of the source byte so urlencode() can format it as uppercase hexadecimal
    emitter.instruction("cmp r10b, 10");                                        // does the high nibble require an alphabetic uppercase hexadecimal digit instead of a decimal digit?
    emitter.instruction("jae __rt_urlencode_hi_af_linux_x86_64");               // map nibble values 10-15 to 'A'-'F' for the high hexadecimal digit
    emitter.instruction("add r10b, 48");                                        // map nibble values 0-9 to '0'-'9' for the high hexadecimal digit
    emitter.instruction("jmp __rt_urlencode_hi_store_linux_x86_64");            // skip the alphabetic-nibble mapping once the high digit has been converted

    emitter.label("__rt_urlencode_hi_af_linux_x86_64");
    emitter.instruction("add r10b, 55");                                        // map nibble values 10-15 to 'A'-'F' for the high hexadecimal digit

    emitter.label("__rt_urlencode_hi_store_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11], r10b");                            // store the high hexadecimal digit of the percent-encoded expansion into concat storage
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the high hexadecimal digit
    emitter.instruction("movzx r10d, dl");                                      // reload the original source byte before extracting its low hexadecimal nibble
    emitter.instruction("and r10b, 15");                                        // isolate the low nibble of the source byte so urlencode() can format it as uppercase hexadecimal
    emitter.instruction("cmp r10b, 10");                                        // does the low nibble require an alphabetic uppercase hexadecimal digit instead of a decimal digit?
    emitter.instruction("jae __rt_urlencode_lo_af_linux_x86_64");               // map nibble values 10-15 to 'A'-'F' for the low hexadecimal digit
    emitter.instruction("add r10b, 48");                                        // map nibble values 0-9 to '0'-'9' for the low hexadecimal digit
    emitter.instruction("jmp __rt_urlencode_lo_store_linux_x86_64");            // skip the alphabetic-nibble mapping once the low digit has been converted

    emitter.label("__rt_urlencode_lo_af_linux_x86_64");
    emitter.instruction("add r10b, 55");                                        // map nibble values 10-15 to 'A'-'F' for the low hexadecimal digit

    emitter.label("__rt_urlencode_lo_store_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11], r10b");                            // store the low hexadecimal digit of the percent-encoded expansion into concat storage
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the low hexadecimal digit
    emitter.instruction("jmp __rt_urlencode_loop_linux_x86_64");                // continue encoding the remaining source bytes after percent-encoding one unsafe byte

    emitter.label("__rt_urlencode_passthru_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11], dl");                              // store bytes from the safe query-style character set directly into concat storage unchanged
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after copying one safe source byte
    emitter.instruction("jmp __rt_urlencode_loop_linux_x86_64");                // continue encoding the remaining source bytes after copying one safe byte

    emitter.label("__rt_urlencode_done_linux_x86_64");
    emitter.instruction("mov rax, r8");                                         // return the reserved result start pointer after percent-encoding the full input string
    emitter.instruction("mov rdx, r11");                                        // copy the final destination cursor before computing the encoded string length
    emitter.instruction("sub rdx, r8");                                         // compute the encoded string length as dest_end - dest_start for the returned x86_64 string value
    emitter.instruction("call __rt_concat_publish");                            // advance the concat scratch offset only for scratch-backed results
    emitter.instruction("add rsp, 32");                                         // release the urlencode spill slots before returning the encoded string
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the encoded string
    emitter.instruction("ret");                                                 // return the urlencoded string in the standard x86_64 string result registers

    // -- impossible result size: report the shared allocation-overflow fatal error --
    emitter.label("__rt_urlencode_size_overflow_linux_x86_64");
    emitter.instruction("jmp __rt_alloc_overflow");                             // unconditional branch keeps the fatal trampoline reachable from every caller
}
