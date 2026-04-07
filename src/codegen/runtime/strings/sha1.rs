use crate::codegen::emit::Emitter;

/// sha1: compute SHA1 hash of a string, return as 40-char hex string.
/// Input: x1=string ptr, x2=string len
/// Output: x1=hex string ptr, x2=40
/// Uses macOS CommonCrypto CC_SHA1(data, len, md) which outputs 20 raw bytes.
pub fn emit_sha1(emitter: &mut Emitter) {
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
    emitter.adrp("x6", "_concat_off");                           // load concat offset page
    emitter.add_lo12("x6", "x6", "_concat_off");                     // resolve address
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    emitter.adrp("x7", "_concat_buf");                           // load concat buffer page
    emitter.add_lo12("x7", "x7", "_concat_buf");                     // resolve address
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
