use crate::codegen::emit::Emitter;

/// __rt_ptoa: convert pointer address to hex string "0x...".
/// Input:  x0 = pointer value (64-bit address)
/// Output: x1 = string pointer (in concat_buf), x2 = string length
pub fn emit_ptoa(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for ARM64 instructions
    emitter.comment("--- runtime: ptoa (pointer to hex string) ---");
    emitter.label("__rt_ptoa");

    // -- save return address --
    emitter.instruction("str x30, [sp, #-16]!");                                // save link register

    // -- set up output buffer in concat_buf --
    emitter.instruction("adrp x1, _concat_buf@PAGE");                          // load page of concat buffer
    emitter.instruction("add x1, x1, _concat_buf@PAGEOFF");                    // resolve concat buffer address
    emitter.instruction("mov x3, x1");                                          // x3 = write cursor

    // -- write "0x" prefix --
    emitter.instruction("mov w4, #0x30");                                       // ASCII '0'
    emitter.instruction("strb w4, [x3], #1");                                   // write '0', advance cursor
    emitter.instruction("mov w4, #0x78");                                       // ASCII 'x'
    emitter.instruction("strb w4, [x3], #1");                                   // write 'x', advance cursor

    // -- handle zero specially --
    emitter.instruction("cbnz x0, __rt_ptoa_find_start");                      // non-zero, find first nibble
    emitter.instruction("mov w4, #0x30");                                       // ASCII '0'
    emitter.instruction("strb w4, [x3], #1");                                   // write single '0' for null pointer
    emitter.instruction("b __rt_ptoa_done");                                    // skip to end

    // -- find first non-zero nibble (skip leading zeros) --
    emitter.label("__rt_ptoa_find_start");
    emitter.instruction("clz x5, x0");                                          // count leading zero bits
    emitter.instruction("lsr x5, x5, #2");                                      // divide by 4 = leading zero nibbles
    emitter.instruction("mov x6, #16");                                         // total nibbles in 64-bit value
    emitter.instruction("sub x6, x6, x5");                                      // x6 = significant nibbles to emit
    emitter.instruction("lsl x5, x5, #2");                                      // x5 = bits to shift left to align first nibble
    emitter.instruction("lsl x0, x0, x5");                                      // shift value so first significant nibble is at top

    // -- emit hex digits loop --
    emitter.label("__rt_ptoa_loop");
    emitter.instruction("cbz x6, __rt_ptoa_done");                             // all nibbles emitted
    emitter.instruction("lsr x4, x0, #60");                                     // extract top 4 bits (current nibble)
    emitter.instruction("cmp x4, #10");                                         // is it >= 10?
    emitter.instruction("b.ge __rt_ptoa_hex_letter");                           // yes, use a-f
    emitter.instruction("add x4, x4, #0x30");                                   // convert 0-9 to ASCII '0'-'9'
    emitter.instruction("b __rt_ptoa_store");                                   // go store the digit

    emitter.label("__rt_ptoa_hex_letter");
    emitter.instruction("add x4, x4, #0x57");                                   // convert 10-15 to ASCII 'a'-'f' (10+0x57=0x61='a')

    emitter.label("__rt_ptoa_store");
    emitter.instruction("strb w4, [x3], #1");                                   // store hex digit, advance cursor
    emitter.instruction("lsl x0, x0, #4");                                      // shift next nibble into top position
    emitter.instruction("sub x6, x6, #1");                                      // decrement remaining nibble count
    emitter.instruction("b __rt_ptoa_loop");                                    // continue loop

    // -- compute length and return --
    emitter.label("__rt_ptoa_done");
    emitter.instruction("sub x2, x3, x1");                                      // x2 = length (cursor - start)
    emitter.instruction("ldr x30, [sp], #16");                                  // restore link register
    emitter.instruction("ret");                                                 // return to caller
}
