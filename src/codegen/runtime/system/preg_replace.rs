use crate::codegen::emit::Emitter;

/// __rt_preg_replace: replace all regex matches in subject string.
/// Input:  x1=pattern ptr, x2=pattern len, x3=replacement ptr, x4=replacement len,
///         x5=subject ptr, x6=subject len
/// Output: x1=result ptr, x2=result len
///
/// Stack layout (240 bytes):
///   sp+0..31:    regex_t (32 bytes)
///   sp+32..47:   regmatch_t (16 bytes)
///   sp+48..55:   pattern ptr
///   sp+56..63:   pattern len
///   sp+64..71:   replacement ptr
///   sp+72..79:   replacement len
///   sp+80..87:   subject ptr
///   sp+88..95:   subject len
///   sp+96..103:  flags
///   sp+104..111: pattern C string
///   sp+112..119: subject C string
///   sp+120..127: output start
///   sp+128..135: output write pos
///   sp+136..143: current C string pos
///   sp+144..191: padding
///   sp+192..207: padding
///   sp+208..223: saved x29, x30
pub(crate) fn emit_preg_replace(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: preg_replace ---");
    emitter.label_global("__rt_preg_replace");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #240");                                    // allocate 240 bytes
    emitter.instruction("stp x29, x30, [sp, #224]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #224");                                   // set new frame pointer

    // -- save all inputs --
    emitter.instruction("str x1, [sp, #48]");                                   // pattern ptr
    emitter.instruction("str x2, [sp, #56]");                                   // pattern len
    emitter.instruction("str x3, [sp, #64]");                                   // replacement ptr
    emitter.instruction("str x4, [sp, #72]");                                   // replacement len
    emitter.instruction("str x5, [sp, #80]");                                   // subject ptr
    emitter.instruction("str x6, [sp, #88]");                                   // subject len

    // -- strip delimiters from pattern --
    emitter.instruction("bl __rt_preg_strip");                                  // → x1, x2, x3=flags
    emitter.instruction("str x3, [sp, #96]");                                   // save flags

    // -- convert pattern from PCRE to POSIX and null-terminate --
    emitter.instruction("bl __rt_pcre_to_posix");                               // → x0=C string with PCRE shorthands converted
    emitter.instruction("str x0, [sp, #104]");                                  // save pattern C string

    // -- compile regex --
    emitter.instruction("mov x0, sp");                                          // regex_t at sp
    emitter.instruction("ldr x1, [sp, #104]");                                  // pattern
    emitter.instruction("mov x2, #1");                                          // REG_EXTENDED
    emitter.instruction("ldr x9, [sp, #96]");                                   // flags
    emitter.instruction("tst x9, #1");                                          // test icase
    emitter.instruction("b.eq __rt_preg_replace_nc");                           // skip
    emitter.instruction("orr x2, x2, #2");                                      // REG_ICASE
    emitter.label("__rt_preg_replace_nc");
    emitter.bl_c("regcomp");                                         // compile
    emitter.instruction("cbnz x0, __rt_preg_replace_fail");                     // fail → return original

    // -- null-terminate subject --
    emitter.instruction("ldr x1, [sp, #80]");                                   // subject ptr
    emitter.instruction("ldr x2, [sp, #88]");                                   // subject len
    emitter.instruction("bl __rt_cstr2");                                       // → x0=subject C string
    emitter.instruction("str x0, [sp, #112]");                                  // save subject C string

    // -- set up output buffer in concat_buf --
    emitter.adrp("x9", "_concat_off");                           // load page of concat offset
    emitter.add_lo12("x9", "x9", "_concat_off");                     // resolve address
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    emitter.adrp("x11", "_concat_buf");                          // load page of concat buffer
    emitter.add_lo12("x11", "x11", "_concat_buf");                   // resolve address
    emitter.instruction("add x11, x11, x10");                                   // output position
    emitter.instruction("str x11, [sp, #120]");                                 // save output start
    emitter.instruction("str x11, [sp, #128]");                                 // save output write pos

    // -- initialize current position --
    emitter.instruction("ldr x9, [sp, #112]");                                  // subject C string start
    emitter.instruction("str x9, [sp, #136]");                                  // save current pos

    // -- replacement loop --
    emitter.label("__rt_preg_replace_loop");
    emitter.instruction("ldr x1, [sp, #136]");                                  // current pos
    emitter.instruction("ldrb w9, [x1]");                                       // check for end
    emitter.instruction("cbz w9, __rt_preg_replace_done");                      // end of string

    emitter.instruction("mov x0, sp");                                          // regex_t
    emitter.instruction("mov x2, #1");                                          // nmatch
    emitter.instruction("add x3, sp, #32");                                     // regmatch_t at sp+32
    emitter.instruction("mov x4, #0");                                          // eflags
    emitter.bl_c("regexec");                                         // execute
    emitter.instruction("cbnz x0, __rt_preg_replace_tail");                     // no more matches, copy rest

    // -- copy text before match (rm_so bytes) --
    emitter.instruction("ldr x9, [sp, #32]");                                   // rm_so (8-byte at sp+32)
    emitter.instruction("ldr x10, [sp, #136]");                                 // current C string pos
    emitter.instruction("ldr x11, [sp, #128]");                                 // output write pos
    emitter.instruction("mov x12, #0");                                         // copy index
    emitter.label("__rt_preg_replace_pre");
    emitter.instruction("cmp x12, x9");                                         // check if done
    emitter.instruction("b.ge __rt_preg_replace_repl");                         // done copying prefix
    emitter.instruction("ldrb w13, [x10, x12]");                                // load byte
    emitter.instruction("strb w13, [x11]");                                     // write byte
    emitter.instruction("add x11, x11, #1");                                    // advance output
    emitter.instruction("add x12, x12, #1");                                    // increment
    emitter.instruction("b __rt_preg_replace_pre");                             // continue

    // -- copy replacement string --
    emitter.label("__rt_preg_replace_repl");
    emitter.instruction("ldr x1, [sp, #64]");                                   // replacement ptr
    emitter.instruction("ldr x2, [sp, #72]");                                   // replacement len
    emitter.instruction("mov x12, #0");                                         // copy index
    emitter.label("__rt_preg_replace_repl_copy");
    emitter.instruction("cmp x12, x2");                                         // check if done
    emitter.instruction("b.ge __rt_preg_replace_advance");                      // done
    emitter.instruction("ldrb w13, [x1, x12]");                                 // load replacement byte
    emitter.instruction("strb w13, [x11]");                                     // write byte
    emitter.instruction("add x11, x11, #1");                                    // advance output
    emitter.instruction("add x12, x12, #1");                                    // increment
    emitter.instruction("b __rt_preg_replace_repl_copy");                       // continue

    // -- advance past match --
    emitter.label("__rt_preg_replace_advance");
    emitter.instruction("str x11, [sp, #128]");                                 // save output write pos
    emitter.instruction("ldr x9, [sp, #40]");                                   // rm_eo (8-byte at sp+32+8)
    emitter.instruction("ldr x10, [sp, #136]");                                 // current pos
    emitter.instruction("add x10, x10, x9");                                    // advance past match
    emitter.instruction("str x10, [sp, #136]");                                 // save new pos
    emitter.instruction("b __rt_preg_replace_loop");                            // continue

    // -- copy remaining text after last match --
    emitter.label("__rt_preg_replace_tail");
    emitter.instruction("ldr x10, [sp, #136]");                                 // current pos
    emitter.instruction("ldr x11, [sp, #128]");                                 // output write pos
    emitter.label("__rt_preg_replace_tail_loop");
    emitter.instruction("ldrb w9, [x10]");                                      // load byte
    emitter.instruction("cbz w9, __rt_preg_replace_done");                      // null terminator = done
    emitter.instruction("strb w9, [x11]");                                      // write byte
    emitter.instruction("add x10, x10, #1");                                    // advance source
    emitter.instruction("add x11, x11, #1");                                    // advance output
    emitter.instruction("b __rt_preg_replace_tail_loop");                       // continue

    emitter.label("__rt_preg_replace_done");
    emitter.instruction("str x11, [sp, #128]");                                 // save final write pos
    // -- free regex --
    emitter.instruction("mov x0, sp");                                          // regex_t
    emitter.bl_c("regfree");                                         // free

    // -- compute result --
    emitter.instruction("ldr x1, [sp, #120]");                                  // output start
    emitter.instruction("ldr x11, [sp, #128]");                                 // output end
    emitter.instruction("sub x2, x11, x1");                                     // result length

    // -- update concat_off --
    emitter.adrp("x9", "_concat_off");                           // load page of concat offset
    emitter.add_lo12("x9", "x9", "_concat_off");                     // resolve address
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    emitter.instruction("add x10, x10, x2");                                    // add result length
    emitter.instruction("str x10, [x9]");                                       // store updated offset
    emitter.instruction("b __rt_preg_replace_ret");                             // return

    // -- failure: return original subject --
    emitter.label("__rt_preg_replace_fail");
    emitter.instruction("ldr x1, [sp, #80]");                                   // original subject ptr
    emitter.instruction("ldr x2, [sp, #88]");                                   // original subject len

    emitter.label("__rt_preg_replace_ret");
    emitter.instruction("ldp x29, x30, [sp, #224]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #240");                                    // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
