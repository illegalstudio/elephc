use crate::codegen::emit::Emitter;

/// hash: compute hash of data using named algorithm.
/// Input: x1/x2=algorithm name, x3/x4=data ptr/len
/// Output: x1/x2=hex string in concat_buf
/// Supports: "md5" (CC_MD5, 16 bytes), "sha1" (CC_SHA1, 20 bytes), "sha256" (CC_SHA256, 32 bytes)
pub fn emit_hash(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash ---");
    emitter.label_global("__rt_hash");
    emitter.instruction("sub sp, sp, #96");                                     // allocate stack frame (32 bytes hash + state)
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // set frame pointer
    emitter.instruction("stp x3, x4, [sp, #64]");                               // save data ptr/len

    // -- check algorithm name --
    // Compare first char to dispatch quickly
    emitter.instruction("ldrb w9, [x1]");                                       // load first char of algo name

    // -- check for "md5" (len=3, starts with 'm') --
    emitter.instruction("cmp w9, #109");                                        // 'm'?
    emitter.instruction("b.ne __rt_hash_try_sha");                              // no → try sha*
    emitter.instruction("ldp x0, x1, [sp, #64]");                               // x0=data ptr, x1 unused
    emitter.instruction("ldr x0, [sp, #64]");                                   // x0 = data ptr
    emitter.instruction("ldr x1, [sp, #72]");                                   // w1 = data len
    emitter.instruction("mov w1, w1");                                          // truncate to 32-bit CC_LONG
    emitter.instruction("add x2, sp, #0");                                      // output buffer
    emitter.bl_c("CC_MD5");                                          // call CommonCrypto MD5
    emitter.instruction("mov x5, #16");                                         // hash size = 16 bytes
    emitter.instruction("b __rt_hash_hex");                                     // convert to hex

    // -- check for "sha1" or "sha256" --
    emitter.label("__rt_hash_try_sha");
    emitter.instruction("cmp w9, #115");                                        // 's'?
    emitter.instruction("b.ne __rt_hash_unknown");                              // no → unknown algo

    // Disambiguate sha1 vs sha256 by length
    emitter.instruction("cmp x2, #4");                                          // algo len == 4 → "sha1"
    emitter.instruction("b.eq __rt_hash_sha1");                                 // yes
    // Otherwise assume sha256
    emitter.instruction("ldr x0, [sp, #64]");                                   // x0 = data ptr
    emitter.instruction("ldr x1, [sp, #72]");                                   // data len
    emitter.instruction("mov w1, w1");                                          // truncate to CC_LONG
    emitter.instruction("add x2, sp, #0");                                      // output buffer (32 bytes)
    emitter.bl_c("CC_SHA256");                                       // call CommonCrypto SHA256
    emitter.instruction("mov x5, #32");                                         // hash size = 32 bytes
    emitter.instruction("b __rt_hash_hex");                                     // convert to hex

    emitter.label("__rt_hash_sha1");
    emitter.instruction("ldr x0, [sp, #64]");                                   // x0 = data ptr
    emitter.instruction("ldr x1, [sp, #72]");                                   // data len
    emitter.instruction("mov w1, w1");                                          // truncate to CC_LONG
    emitter.instruction("add x2, sp, #0");                                      // output buffer (20 bytes)
    emitter.bl_c("CC_SHA1");                                         // call CommonCrypto SHA1
    emitter.instruction("mov x5, #20");                                         // hash size = 20 bytes
    emitter.instruction("b __rt_hash_hex");                                     // convert to hex

    // -- unknown algorithm: return empty string --
    emitter.label("__rt_hash_unknown");
    emitter.instruction("mov x2, #0");                                          // empty result
    emitter.instruction("b __rt_hash_done");                                    // skip hex conversion

    // -- convert raw hash bytes to hex string --
    emitter.label("__rt_hash_hex");
    emitter.adrp("x6", "_concat_off");                           // load concat offset page
    emitter.add_lo12("x6", "x6", "_concat_off");                     // resolve address
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    emitter.adrp("x7", "_concat_buf");                           // load concat buffer page
    emitter.add_lo12("x7", "x7", "_concat_buf");                     // resolve address
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("add x11, sp, #0");                                     // source = raw hash bytes
    emitter.instruction("mov x12, x5");                                         // bytes to convert

    emitter.label("__rt_hash_hex_loop");
    emitter.instruction("cbz x12, __rt_hash_hex_done");                         // all bytes converted
    emitter.instruction("ldrb w13, [x11], #1");                                 // load byte, advance
    emitter.instruction("sub x12, x12, #1");                                    // decrement counter
    // -- high nibble --
    emitter.instruction("lsr w14, w13, #4");                                    // extract high 4 bits
    emitter.instruction("cmp w14, #10");                                        // >= 10?
    emitter.instruction("b.ge __rt_hash_hi_af");                                // yes → a-f
    emitter.instruction("add w14, w14, #48");                                   // 0-9 → '0'-'9'
    emitter.instruction("b __rt_hash_hi_st");                                   // store
    emitter.label("__rt_hash_hi_af");
    emitter.instruction("add w14, w14, #87");                                   // 10-15 → 'a'-'f'
    emitter.label("__rt_hash_hi_st");
    emitter.instruction("strb w14, [x9], #1");                                  // write high hex char
    // -- low nibble --
    emitter.instruction("and w14, w13, #0xf");                                  // extract low 4 bits
    emitter.instruction("cmp w14, #10");                                        // >= 10?
    emitter.instruction("b.ge __rt_hash_lo_af");                                // yes → a-f
    emitter.instruction("add w14, w14, #48");                                   // 0-9 → '0'-'9'
    emitter.instruction("b __rt_hash_lo_st");                                   // store
    emitter.label("__rt_hash_lo_af");
    emitter.instruction("add w14, w14, #87");                                   // 10-15 → 'a'-'f'
    emitter.label("__rt_hash_lo_st");
    emitter.instruction("strb w14, [x9], #1");                                  // write low hex char
    emitter.instruction("b __rt_hash_hex_loop");                                // next byte

    emitter.label("__rt_hash_hex_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, x2");                                      // advance
    emitter.instruction("str x8, [x6]");                                        // store updated offset

    emitter.label("__rt_hash_done");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame
    emitter.instruction("add sp, sp, #96");                                     // deallocate
    emitter.instruction("ret");                                                 // return
}
