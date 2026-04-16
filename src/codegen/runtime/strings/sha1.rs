use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// sha1: compute SHA1 hash of a string, return as 40-char hex string.
/// Input: x1=string ptr, x2=string len
/// Output: x1=hex string ptr, x2=40
/// Uses macOS CommonCrypto CC_SHA1(data, len, md) which outputs 20 raw bytes.
pub fn emit_sha1(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_sha1_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: sha1 ---");
    emitter.label_global("__rt_sha1");
    emitter.instruction("sub sp, sp, #64");                                     // allocate stack frame (20 bytes for hash + padding)
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set frame pointer

    // -- call CC_SHA1(data, len, output_buf) --
    emitter.instruction("mov x0, x1");                                          // x0 = input string pointer
    emitter.instruction("mov w1, w2");                                          // w1 = input length (CC_LONG = uint32)
    emitter.instruction("add x2, sp, #0");                                      // x2 = output buffer (20 bytes at bottom of frame)
    emitter.bl_c("CC_SHA1");                                         // call CommonCrypto SHA1

    // -- convert 20 raw bytes to 40 hex chars --
    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("add x11, sp, #0");                                     // source = raw SHA1 bytes
    emitter.instruction("mov x12, #20");                                        // 20 bytes to convert

    // -- byte-to-hex loop --
    emitter.label("__rt_sha1_hex_loop");
    emitter.instruction("cbz x12, __rt_sha1_done");                             // all bytes converted
    emitter.instruction("ldrb w13, [x11], #1");                                 // load raw byte, advance
    emitter.instruction("sub x12, x12, #1");                                    // decrement counter
    // -- high nibble --
    emitter.instruction("lsr w14, w13, #4");                                    // extract high 4 bits
    emitter.instruction("cmp w14, #10");                                        // >= 10?
    emitter.instruction("b.ge __rt_sha1_hi_af");                                // yes → a-f
    emitter.instruction("add w14, w14, #48");                                   // 0-9 → '0'-'9'
    emitter.instruction("b __rt_sha1_hi_store");                                // store
    emitter.label("__rt_sha1_hi_af");
    emitter.instruction("add w14, w14, #87");                                   // 10-15 → 'a'-'f'
    emitter.label("__rt_sha1_hi_store");
    emitter.instruction("strb w14, [x9], #1");                                  // write high hex char
    // -- low nibble --
    emitter.instruction("and w14, w13, #0xf");                                  // extract low 4 bits
    emitter.instruction("cmp w14, #10");                                        // >= 10?
    emitter.instruction("b.ge __rt_sha1_lo_af");                                // yes → a-f
    emitter.instruction("add w14, w14, #48");                                   // 0-9 → '0'-'9'
    emitter.instruction("b __rt_sha1_lo_store");                                // store
    emitter.label("__rt_sha1_lo_af");
    emitter.instruction("add w14, w14, #87");                                   // 10-15 → 'a'-'f'
    emitter.label("__rt_sha1_lo_store");
    emitter.instruction("strb w14, [x9], #1");                                  // write low hex char
    emitter.instruction("b __rt_sha1_hex_loop");                                // next byte

    // -- finalize --
    emitter.label("__rt_sha1_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("mov x2, #40");                                         // result length = 40 hex chars
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, #40");                                     // advance by 40
    emitter.instruction("str x8, [x6]");                                        // store updated offset
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame
    emitter.instruction("add sp, sp, #64");                                     // deallocate
    emitter.instruction("ret");                                                 // return
}

fn emit_sha1_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: sha1 ---");
    emitter.label_global("__rt_sha1");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving aligned digest scratch space for the SHA1 helper
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base so the raw SHA1 digest buffer can be addressed across the C helper call
    emitter.instruction("sub rsp, 64");                                         // reserve aligned stack space for the raw 20-byte SHA1 digest plus local scratch

    // -- call SHA1(data, len, output_buf) --
    emitter.instruction("mov rdi, rax");                                        // pass the borrowed input string pointer as the first SysV C argument to SHA1()
    emitter.instruction("mov esi, edx");                                        // pass the borrowed input string length as the second SysV C argument to SHA1() using the expected 32-bit size
    emitter.instruction("lea rdx, [rbp - 32]");                                 // pass a stack-backed 20-byte output buffer as the third SysV C argument to SHA1()
    emitter.bl_c("CC_SHA1");                                                    // compute the raw SHA1 digest bytes into the local stack buffer through the platform C symbol mapping

    // -- convert 20 raw bytes to 40 hex chars --
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the current concat-buffer write offset before formatting the raw SHA1 digest as lowercase hexadecimal
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("lea r11, [r10 + r9]");                                 // compute the concat-buffer destination pointer where the 40-byte lowercase hexadecimal digest begins
    emitter.instruction("mov r8, r11");                                         // preserve the concat-backed digest start pointer for the returned string value after the hex loop mutates the destination cursor
    emitter.instruction("lea rcx, [rbp - 32]");                                 // seed the raw-digest source cursor with the local SHA1 output buffer returned by the C helper
    emitter.instruction("mov rsi, 20");                                         // seed the remaining raw-digest byte counter because SHA1 always produces 20 bytes

    emitter.label("__rt_sha1_hex_loop_linux_x86_64");
    emitter.instruction("test rsi, rsi");                                       // stop once every raw SHA1 byte has been formatted into lowercase hexadecimal
    emitter.instruction("jz __rt_sha1_done_linux_x86_64");                      // finish once the full 20-byte raw SHA1 digest has been converted to 40 lowercase hex digits
    emitter.instruction("movzx edx, BYTE PTR [rcx]");                           // load one raw SHA1 byte before splitting it into high and low hexadecimal nibbles
    emitter.instruction("add rcx, 1");                                          // advance the raw-digest source cursor after consuming one SHA1 byte
    emitter.instruction("sub rsi, 1");                                          // decrement the remaining raw-digest byte count after consuming one SHA1 byte
    emitter.instruction("mov eax, edx");                                        // copy the raw SHA1 byte before extracting its high hexadecimal nibble for lowercase formatting
    emitter.instruction("shr al, 4");                                           // isolate the high nibble of the raw SHA1 byte so it can be rendered as lowercase hexadecimal
    emitter.instruction("cmp al, 10");                                          // does the high nibble require an alphabetic lowercase hexadecimal digit instead of a decimal digit?
    emitter.instruction("jae __rt_sha1_hi_af_linux_x86_64");                    // map nibble values 10-15 to 'a'-'f' for the high hexadecimal digit
    emitter.instruction("add al, 48");                                          // map nibble values 0-9 to '0'-'9' for the high hexadecimal digit
    emitter.instruction("jmp __rt_sha1_hi_store_linux_x86_64");                 // skip the alphabetic-nibble mapping once the high digit has been converted

    emitter.label("__rt_sha1_hi_af_linux_x86_64");
    emitter.instruction("add al, 87");                                          // map nibble values 10-15 to 'a'-'f' for the high hexadecimal digit

    emitter.label("__rt_sha1_hi_store_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11], al");                              // store the high lowercase hexadecimal digit of the current SHA1 byte into concat storage
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the high lowercase hexadecimal digit
    emitter.instruction("mov eax, edx");                                        // reload the raw SHA1 byte before extracting its low hexadecimal nibble for lowercase formatting
    emitter.instruction("and al, 15");                                          // isolate the low nibble of the raw SHA1 byte so it can be rendered as lowercase hexadecimal
    emitter.instruction("cmp al, 10");                                          // does the low nibble require an alphabetic lowercase hexadecimal digit instead of a decimal digit?
    emitter.instruction("jae __rt_sha1_lo_af_linux_x86_64");                    // map nibble values 10-15 to 'a'-'f' for the low hexadecimal digit
    emitter.instruction("add al, 48");                                          // map nibble values 0-9 to '0'-'9' for the low hexadecimal digit
    emitter.instruction("jmp __rt_sha1_lo_store_linux_x86_64");                 // skip the alphabetic-nibble mapping once the low digit has been converted

    emitter.label("__rt_sha1_lo_af_linux_x86_64");
    emitter.instruction("add al, 87");                                          // map nibble values 10-15 to 'a'-'f' for the low hexadecimal digit

    emitter.label("__rt_sha1_lo_store_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11], al");                              // store the low lowercase hexadecimal digit of the current SHA1 byte into concat storage
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the low lowercase hexadecimal digit
    emitter.instruction("jmp __rt_sha1_hex_loop_linux_x86_64");                 // continue formatting the remaining raw SHA1 bytes into lowercase hexadecimal

    emitter.label("__rt_sha1_done_linux_x86_64");
    emitter.instruction("mov rax, r8");                                         // return the concat-backed start pointer of the lowercase hexadecimal SHA1 digest string
    emitter.instruction("mov rdx, 40");                                         // return the fixed 40-byte lowercase hexadecimal length of the SHA1 digest string
    emitter.instruction("mov rcx, QWORD PTR [rip + _concat_off]");              // reload the concat-buffer write offset before publishing the 40-byte lowercase hexadecimal SHA1 digest
    emitter.instruction("add rcx, 40");                                         // advance the concat-buffer write offset by the fixed 40-byte lowercase hexadecimal SHA1 digest length
    emitter.instruction("mov QWORD PTR [rip + _concat_off], rcx");              // persist the updated concat-buffer write offset after formatting the SHA1 digest
    emitter.instruction("add rsp, 64");                                         // release the stack-backed SHA1 digest buffer before returning the lowercase hexadecimal result string
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the lowercase hexadecimal SHA1 result string
    emitter.instruction("ret");                                                 // return the lowercase hexadecimal SHA1 digest in the standard x86_64 string result registers
}
