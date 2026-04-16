use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// rawurlencode: percent-encode non-alphanumeric chars except -_.~ (spaces become %20).
/// Input: x1/x2=string. Output: x1/x2=result in concat_buf.
pub fn emit_rawurlencode(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_rawurlencode_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: rawurlencode ---");
    emitter.label_global("__rt_rawurlencode");

    // -- set up concat_buf destination --
    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("mov x11, x2");                                         // remaining byte count

    emitter.label("__rt_rawurlencode_loop");
    emitter.instruction("cbz x11, __rt_rawurlencode_done");                     // no bytes left -> done
    emitter.instruction("ldrb w12, [x1], #1");                                  // load source byte, advance
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining

    // -- check alphanumeric: A-Z --
    emitter.instruction("cmp w12, #65");                                        // >= 'A'?
    emitter.instruction("b.lt __rt_rawurlencode_chk_safe");                     // no -> check safe chars
    emitter.instruction("cmp w12, #90");                                        // <= 'Z'?
    emitter.instruction("b.le __rt_rawurlencode_pass");                         // yes -> pass through
    // -- check a-z --
    emitter.instruction("cmp w12, #97");                                        // >= 'a'?
    emitter.instruction("b.lt __rt_rawurlencode_chk_safe");                     // no -> check safe chars
    emitter.instruction("cmp w12, #122");                                       // <= 'z'?
    emitter.instruction("b.le __rt_rawurlencode_pass");                         // yes -> pass through
    // -- check 0-9 --
    emitter.instruction("cmp w12, #48");                                        // >= '0'?
    emitter.instruction("b.lt __rt_rawurlencode_chk_safe");                     // no -> check safe chars
    emitter.instruction("cmp w12, #57");                                        // <= '9'?
    emitter.instruction("b.le __rt_rawurlencode_pass");                         // yes -> pass through

    // -- check safe chars: - (45), _ (95), . (46), ~ (126) --
    emitter.label("__rt_rawurlencode_chk_safe");
    emitter.instruction("cmp w12, #45");                                        // is it '-'?
    emitter.instruction("b.eq __rt_rawurlencode_pass");                         // yes -> pass through
    emitter.instruction("cmp w12, #95");                                        // is it '_'?
    emitter.instruction("b.eq __rt_rawurlencode_pass");                         // yes -> pass through
    emitter.instruction("cmp w12, #46");                                        // is it '.'?
    emitter.instruction("b.eq __rt_rawurlencode_pass");                         // yes -> pass through
    emitter.instruction("cmp w12, #126");                                       // is it '~'?
    emitter.instruction("b.eq __rt_rawurlencode_pass");                         // yes -> pass through

    // -- percent-encode: write %XX --
    emitter.instruction("mov w13, #37");                                        // '%' character
    emitter.instruction("strb w13, [x9], #1");                                  // write '%'
    // -- high nibble --
    emitter.instruction("lsr w13, w12, #4");                                    // extract high 4 bits
    emitter.instruction("cmp w13, #10");                                        // >= 10?
    emitter.instruction("b.ge __rt_rawurlencode_hi_af");                        // yes -> use A-F
    emitter.instruction("add w13, w13, #48");                                   // convert 0-9 to '0'-'9'
    emitter.instruction("b __rt_rawurlencode_hi_st");                           // store
    emitter.label("__rt_rawurlencode_hi_af");
    emitter.instruction("add w13, w13, #55");                                   // convert 10-15 to 'A'-'F'
    emitter.label("__rt_rawurlencode_hi_st");
    emitter.instruction("strb w13, [x9], #1");                                  // write high nibble hex char
    // -- low nibble --
    emitter.instruction("and w13, w12, #0xf");                                  // extract low 4 bits
    emitter.instruction("cmp w13, #10");                                        // >= 10?
    emitter.instruction("b.ge __rt_rawurlencode_lo_af");                        // yes -> use A-F
    emitter.instruction("add w13, w13, #48");                                   // convert 0-9 to '0'-'9'
    emitter.instruction("b __rt_rawurlencode_lo_st");                           // store
    emitter.label("__rt_rawurlencode_lo_af");
    emitter.instruction("add w13, w13, #55");                                   // convert 10-15 to 'A'-'F'
    emitter.label("__rt_rawurlencode_lo_st");
    emitter.instruction("strb w13, [x9], #1");                                  // write low nibble hex char
    emitter.instruction("b __rt_rawurlencode_loop");                            // next byte

    // -- pass through byte unchanged --
    emitter.label("__rt_rawurlencode_pass");
    emitter.instruction("strb w12, [x9], #1");                                  // store byte as-is
    emitter.instruction("b __rt_rawurlencode_loop");                            // next byte

    emitter.label("__rt_rawurlencode_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, x2");                                      // advance by result length
    emitter.instruction("str x8, [x6]");                                        // store updated offset
    emitter.instruction("ret");                                                 // return
}

fn emit_rawurlencode_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: rawurlencode ---");
    emitter.label_global("__rt_rawurlencode");

    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the current concat-buffer write offset before RFC 3986 percent-encoding the borrowed source string
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("lea r11, [r10 + r9]");                                 // compute the concat-buffer destination pointer where the rawurlencoded string begins
    emitter.instruction("mov r8, r11");                                         // preserve the concat-backed result start pointer for the returned string value after the loop mutates the destination cursor
    emitter.instruction("mov rcx, rdx");                                        // seed the remaining source length counter from the borrowed input string length
    emitter.instruction("mov rsi, rax");                                        // preserve the borrowed source string cursor in a dedicated register before the loop mutates caller-saved registers

    emitter.label("__rt_rawurlencode_loop_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // stop once every source byte has been classified and copied or percent-encoded into concat storage
    emitter.instruction("jz __rt_rawurlencode_done_linux_x86_64");              // finish once the full borrowed source string has been consumed
    emitter.instruction("mov dl, BYTE PTR [rsi]");                              // load one source byte before deciding whether rawurlencode() must encode it
    emitter.instruction("add rsi, 1");                                          // advance the borrowed source string cursor after consuming one byte
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining source length after consuming one byte
    emitter.instruction("cmp dl, 65");                                          // is the current source byte at least 'A', which could make it an uppercase ASCII safe character?
    emitter.instruction("jb __rt_rawurlencode_chk_safe_linux_x86_64");          // continue with the remaining safe-byte checks when the byte falls below 'A'
    emitter.instruction("cmp dl, 90");                                          // is the current source byte at most 'Z', which keeps it inside the uppercase ASCII safe range?
    emitter.instruction("jbe __rt_rawurlencode_passthru_linux_x86_64");         // pass uppercase ASCII letters straight through without percent-encoding them
    emitter.instruction("cmp dl, 97");                                          // is the current source byte at least 'a', which could make it a lowercase ASCII safe character?
    emitter.instruction("jb __rt_rawurlencode_chk_safe_linux_x86_64");          // continue with the remaining safe-byte checks when the byte falls below 'a'
    emitter.instruction("cmp dl, 122");                                         // is the current source byte at most 'z', which keeps it inside the lowercase ASCII safe range?
    emitter.instruction("jbe __rt_rawurlencode_passthru_linux_x86_64");         // pass lowercase ASCII letters straight through without percent-encoding them
    emitter.instruction("cmp dl, 48");                                          // is the current source byte at least '0', which could make it a decimal digit safe to pass through?
    emitter.instruction("jb __rt_rawurlencode_chk_safe_linux_x86_64");          // continue with the punctuation safe-byte checks when the byte falls below '0'
    emitter.instruction("cmp dl, 57");                                          // is the current source byte at most '9', which keeps it inside the decimal-digit safe range?
    emitter.instruction("jbe __rt_rawurlencode_passthru_linux_x86_64");         // pass decimal digits straight through without percent-encoding them

    emitter.label("__rt_rawurlencode_chk_safe_linux_x86_64");
    emitter.instruction("cmp dl, 45");                                          // is the current source byte '-' which rawurlencode() leaves untouched?
    emitter.instruction("je __rt_rawurlencode_passthru_linux_x86_64");          // pass '-' straight through because it is part of the RFC 3986 safe punctuation set
    emitter.instruction("cmp dl, 95");                                          // is the current source byte '_' which rawurlencode() leaves untouched?
    emitter.instruction("je __rt_rawurlencode_passthru_linux_x86_64");          // pass '_' straight through because it is part of the RFC 3986 safe punctuation set
    emitter.instruction("cmp dl, 46");                                          // is the current source byte '.' which rawurlencode() leaves untouched?
    emitter.instruction("je __rt_rawurlencode_passthru_linux_x86_64");          // pass '.' straight through because it is part of the RFC 3986 safe punctuation set
    emitter.instruction("cmp dl, 126");                                         // is the current source byte '~' which rawurlencode() leaves untouched?
    emitter.instruction("je __rt_rawurlencode_passthru_linux_x86_64");          // pass '~' straight through because it is part of the RFC 3986 safe punctuation set

    emitter.instruction("mov BYTE PTR [r11], 37");                              // write '%' as the first byte of a percent-encoded expansion for an unsafe source byte
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the leading '%' of one percent-encoded byte
    emitter.instruction("movzx r10d, dl");                                      // widen the source byte before extracting its hexadecimal nibbles for the percent-encoded expansion
    emitter.instruction("shr r10b, 4");                                         // isolate the high nibble of the source byte so rawurlencode() can format it as uppercase hexadecimal
    emitter.instruction("cmp r10b, 10");                                        // does the high nibble require an alphabetic uppercase hexadecimal digit instead of a decimal digit?
    emitter.instruction("jae __rt_rawurlencode_hi_af_linux_x86_64");            // map nibble values 10-15 to 'A'-'F' for the high hexadecimal digit
    emitter.instruction("add r10b, 48");                                        // map nibble values 0-9 to '0'-'9' for the high hexadecimal digit
    emitter.instruction("jmp __rt_rawurlencode_hi_store_linux_x86_64");         // skip the alphabetic-nibble mapping once the high digit has been converted

    emitter.label("__rt_rawurlencode_hi_af_linux_x86_64");
    emitter.instruction("add r10b, 55");                                        // map nibble values 10-15 to 'A'-'F' for the high hexadecimal digit

    emitter.label("__rt_rawurlencode_hi_store_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11], r10b");                            // store the high hexadecimal digit of the percent-encoded expansion into concat storage
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the high hexadecimal digit
    emitter.instruction("movzx r10d, dl");                                      // reload the original source byte before extracting its low hexadecimal nibble
    emitter.instruction("and r10b, 15");                                        // isolate the low nibble of the source byte so rawurlencode() can format it as uppercase hexadecimal
    emitter.instruction("cmp r10b, 10");                                        // does the low nibble require an alphabetic uppercase hexadecimal digit instead of a decimal digit?
    emitter.instruction("jae __rt_rawurlencode_lo_af_linux_x86_64");            // map nibble values 10-15 to 'A'-'F' for the low hexadecimal digit
    emitter.instruction("add r10b, 48");                                        // map nibble values 0-9 to '0'-'9' for the low hexadecimal digit
    emitter.instruction("jmp __rt_rawurlencode_lo_store_linux_x86_64");         // skip the alphabetic-nibble mapping once the low digit has been converted

    emitter.label("__rt_rawurlencode_lo_af_linux_x86_64");
    emitter.instruction("add r10b, 55");                                        // map nibble values 10-15 to 'A'-'F' for the low hexadecimal digit

    emitter.label("__rt_rawurlencode_lo_store_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11], r10b");                            // store the low hexadecimal digit of the percent-encoded expansion into concat storage
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the low hexadecimal digit
    emitter.instruction("jmp __rt_rawurlencode_loop_linux_x86_64");             // continue encoding the remaining source bytes after percent-encoding one unsafe byte

    emitter.label("__rt_rawurlencode_passthru_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11], dl");                              // store bytes from the RFC 3986 safe set directly into concat storage unchanged
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after copying one safe source byte
    emitter.instruction("jmp __rt_rawurlencode_loop_linux_x86_64");             // continue encoding the remaining source bytes after copying one safe byte

    emitter.label("__rt_rawurlencode_done_linux_x86_64");
    emitter.instruction("mov rax, r8");                                         // return the concat-backed result start pointer after percent-encoding the full input string
    emitter.instruction("mov rdx, r11");                                        // copy the final concat-buffer destination cursor before computing the encoded string length
    emitter.instruction("sub rdx, r8");                                         // compute the encoded string length as dest_end - dest_start for the returned x86_64 string value
    emitter.instruction("mov rcx, QWORD PTR [rip + _concat_off]");              // reload the concat-buffer write offset before publishing the bytes that rawurlencode() appended
    emitter.instruction("add rcx, rdx");                                        // advance the concat-buffer write offset by the produced encoded-string length
    emitter.instruction("mov QWORD PTR [rip + _concat_off], rcx");              // persist the updated concat-buffer write offset after finishing the rawurlencode() pass
    emitter.instruction("ret");                                                 // return the concat-backed rawurlencoded string in the standard x86_64 string result registers
}
