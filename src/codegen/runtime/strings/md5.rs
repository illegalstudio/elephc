use crate::codegen::emit::Emitter;

/// md5: compute MD5 hash of a string, return as 32-char hex string.
/// Input: x1=string ptr, x2=string len
/// Output: x1=hex string ptr, x2=32
/// Uses macOS CommonCrypto CC_MD5(data, len, md) which outputs 16 raw bytes.
pub fn emit_md5(emitter: &mut Emitter) {
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
