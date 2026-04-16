use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// md5: compute MD5 hash of a string, return as 32-char hex string.
/// Input: x1=string ptr, x2=string len
/// Output: x1=hex string ptr, x2=32
/// Uses macOS CommonCrypto CC_MD5(data, len, md) which outputs 16 raw bytes.
pub fn emit_md5(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_md5_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: md5 ---");
    emitter.label_global("__rt_md5");
    emitter.instruction("sub sp, sp, #64");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set frame pointer

    // -- call CC_MD5(data, len, output_buf) --
    // CC_MD5 signature: CC_MD5(const void *data, CC_LONG len, unsigned char *md)
    // x0 = data, x1 = len (32-bit), x2 = output buffer (16 bytes)
    emitter.instruction("mov x0, x1");                                          // x0 = input string pointer
    emitter.instruction("mov w1, w2");                                          // w1 = input length (CC_LONG = uint32)
    emitter.instruction("add x2, sp, #0");                                      // x2 = output buffer at bottom of frame
    emitter.bl_c("CC_MD5");                                          // call CommonCrypto MD5

    // -- convert 16 raw bytes to 32 hex chars --
    // Reuse the hex conversion logic: read from sp+0 (16 bytes), write to concat_buf
    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("add x11, sp, #0");                                     // source = raw MD5 bytes
    emitter.instruction("mov x12, #16");                                        // 16 bytes to convert

    // -- byte-to-hex loop --
    emitter.label("__rt_md5_hex_loop");
    emitter.instruction("cbz x12, __rt_md5_done");                              // all bytes converted
    emitter.instruction("ldrb w13, [x11], #1");                                 // load raw byte, advance
    emitter.instruction("sub x12, x12, #1");                                    // decrement counter
    // -- high nibble --
    emitter.instruction("lsr w14, w13, #4");                                    // extract high 4 bits
    emitter.instruction("cmp w14, #10");                                        // >= 10?
    emitter.instruction("b.ge __rt_md5_hi_af");                                 // yes → a-f
    emitter.instruction("add w14, w14, #48");                                   // 0-9 → '0'-'9'
    emitter.instruction("b __rt_md5_hi_store");                                 // store
    emitter.label("__rt_md5_hi_af");
    emitter.instruction("add w14, w14, #87");                                   // 10-15 → 'a'-'f'
    emitter.label("__rt_md5_hi_store");
    emitter.instruction("strb w14, [x9], #1");                                  // write high hex char
    // -- low nibble --
    emitter.instruction("and w14, w13, #0xf");                                  // extract low 4 bits
    emitter.instruction("cmp w14, #10");                                        // >= 10?
    emitter.instruction("b.ge __rt_md5_lo_af");                                 // yes → a-f
    emitter.instruction("add w14, w14, #48");                                   // 0-9 → '0'-'9'
    emitter.instruction("b __rt_md5_lo_store");                                 // store
    emitter.label("__rt_md5_lo_af");
    emitter.instruction("add w14, w14, #87");                                   // 10-15 → 'a'-'f'
    emitter.label("__rt_md5_lo_store");
    emitter.instruction("strb w14, [x9], #1");                                  // write low hex char
    emitter.instruction("b __rt_md5_hex_loop");                                 // next byte

    // -- finalize --
    emitter.label("__rt_md5_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("mov x2, #32");                                         // result length = 32 hex chars
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, #32");                                     // advance by 32
    emitter.instruction("str x8, [x6]");                                        // store updated offset
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame
    emitter.instruction("add sp, sp, #64");                                     // deallocate
    emitter.instruction("ret");                                                 // return
}

fn emit_md5_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: md5 ---");
    emitter.label_global("__rt_md5");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving aligned digest scratch space for the MD5 helper
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base so the raw MD5 digest buffer can be addressed across the C helper call
    emitter.instruction("sub rsp, 64");                                         // reserve aligned stack space for the raw 16-byte MD5 digest plus local scratch

    // -- call MD5(data, len, output_buf) --
    emitter.instruction("mov rdi, rax");                                        // pass the borrowed input string pointer as the first SysV C argument to MD5()
    emitter.instruction("mov esi, edx");                                        // pass the borrowed input string length as the second SysV C argument to MD5() using the expected 32-bit size
    emitter.instruction("lea rdx, [rbp - 32]");                                 // pass a stack-backed 16-byte output buffer as the third SysV C argument to MD5()
    emitter.bl_c("CC_MD5");                                                     // compute the raw MD5 digest bytes into the local stack buffer through the platform C symbol mapping

    // -- convert 16 raw bytes to 32 hex chars --
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the current concat-buffer write offset before formatting the raw MD5 digest as lowercase hexadecimal
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("lea r11, [r10 + r9]");                                 // compute the concat-buffer destination pointer where the 32-byte lowercase hexadecimal digest begins
    emitter.instruction("mov r8, r11");                                         // preserve the concat-backed digest start pointer for the returned string value after the hex loop mutates the destination cursor
    emitter.instruction("lea rcx, [rbp - 32]");                                 // seed the raw-digest source cursor with the local MD5 output buffer returned by the C helper
    emitter.instruction("mov rsi, 16");                                         // seed the remaining raw-digest byte counter because MD5 always produces 16 bytes

    emitter.label("__rt_md5_hex_loop_linux_x86_64");
    emitter.instruction("test rsi, rsi");                                       // stop once every raw MD5 byte has been formatted into lowercase hexadecimal
    emitter.instruction("jz __rt_md5_done_linux_x86_64");                       // finish once the full 16-byte raw MD5 digest has been converted to 32 lowercase hex digits
    emitter.instruction("movzx edx, BYTE PTR [rcx]");                           // load one raw MD5 byte before splitting it into high and low hexadecimal nibbles
    emitter.instruction("add rcx, 1");                                          // advance the raw-digest source cursor after consuming one MD5 byte
    emitter.instruction("sub rsi, 1");                                          // decrement the remaining raw-digest byte count after consuming one MD5 byte
    emitter.instruction("mov eax, edx");                                        // copy the raw MD5 byte before extracting its high hexadecimal nibble for lowercase formatting
    emitter.instruction("shr al, 4");                                           // isolate the high nibble of the raw MD5 byte so it can be rendered as lowercase hexadecimal
    emitter.instruction("cmp al, 10");                                          // does the high nibble require an alphabetic lowercase hexadecimal digit instead of a decimal digit?
    emitter.instruction("jae __rt_md5_hi_af_linux_x86_64");                     // map nibble values 10-15 to 'a'-'f' for the high hexadecimal digit
    emitter.instruction("add al, 48");                                          // map nibble values 0-9 to '0'-'9' for the high hexadecimal digit
    emitter.instruction("jmp __rt_md5_hi_store_linux_x86_64");                  // skip the alphabetic-nibble mapping once the high digit has been converted

    emitter.label("__rt_md5_hi_af_linux_x86_64");
    emitter.instruction("add al, 87");                                          // map nibble values 10-15 to 'a'-'f' for the high hexadecimal digit

    emitter.label("__rt_md5_hi_store_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11], al");                              // store the high lowercase hexadecimal digit of the current MD5 byte into concat storage
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the high lowercase hexadecimal digit
    emitter.instruction("mov eax, edx");                                        // reload the raw MD5 byte before extracting its low hexadecimal nibble for lowercase formatting
    emitter.instruction("and al, 15");                                          // isolate the low nibble of the raw MD5 byte so it can be rendered as lowercase hexadecimal
    emitter.instruction("cmp al, 10");                                          // does the low nibble require an alphabetic lowercase hexadecimal digit instead of a decimal digit?
    emitter.instruction("jae __rt_md5_lo_af_linux_x86_64");                     // map nibble values 10-15 to 'a'-'f' for the low hexadecimal digit
    emitter.instruction("add al, 48");                                          // map nibble values 0-9 to '0'-'9' for the low hexadecimal digit
    emitter.instruction("jmp __rt_md5_lo_store_linux_x86_64");                  // skip the alphabetic-nibble mapping once the low digit has been converted

    emitter.label("__rt_md5_lo_af_linux_x86_64");
    emitter.instruction("add al, 87");                                          // map nibble values 10-15 to 'a'-'f' for the low hexadecimal digit

    emitter.label("__rt_md5_lo_store_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11], al");                              // store the low lowercase hexadecimal digit of the current MD5 byte into concat storage
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the low lowercase hexadecimal digit
    emitter.instruction("jmp __rt_md5_hex_loop_linux_x86_64");                  // continue formatting the remaining raw MD5 bytes into lowercase hexadecimal

    emitter.label("__rt_md5_done_linux_x86_64");
    emitter.instruction("mov rax, r8");                                         // return the concat-backed start pointer of the lowercase hexadecimal MD5 digest string
    emitter.instruction("mov rdx, 32");                                         // return the fixed 32-byte lowercase hexadecimal length of the MD5 digest string
    emitter.instruction("mov rcx, QWORD PTR [rip + _concat_off]");              // reload the concat-buffer write offset before publishing the 32-byte lowercase hexadecimal MD5 digest
    emitter.instruction("add rcx, 32");                                         // advance the concat-buffer write offset by the fixed 32-byte lowercase hexadecimal MD5 digest length
    emitter.instruction("mov QWORD PTR [rip + _concat_off], rcx");              // persist the updated concat-buffer write offset after formatting the MD5 digest
    emitter.instruction("add rsp, 64");                                         // release the stack-backed MD5 digest buffer before returning the lowercase hexadecimal result string
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the lowercase hexadecimal MD5 result string
    emitter.instruction("ret");                                                 // return the lowercase hexadecimal MD5 digest in the standard x86_64 string result registers
}
