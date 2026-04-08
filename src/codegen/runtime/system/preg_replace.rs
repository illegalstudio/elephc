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
    let regex_t_size = emitter.platform.regex_t_size();
    let regmatch_off = regex_t_size;
    let pattern_ptr_off = regmatch_off + emitter.platform.regmatch_t_size();
    let pattern_len_off = pattern_ptr_off + 8;
    let replacement_ptr_off = pattern_len_off + 8;
    let replacement_len_off = replacement_ptr_off + 8;
    let subject_ptr_off = replacement_len_off + 8;
    let subject_len_off = subject_ptr_off + 8;
    let flags_off = subject_len_off + 8;
    let pattern_cstr_off = flags_off + 8;
    let subject_cstr_off = pattern_cstr_off + 8;
    let output_start_off = subject_cstr_off + 8;
    let output_write_off = output_start_off + 8;
    let current_pos_off = output_write_off + 8;
    let stack_size = (current_pos_off + 96 + 15) & !15;
    let save_off = stack_size - 16;

    emitter.blank();
    emitter.comment("--- runtime: preg_replace ---");
    emitter.label_global("__rt_preg_replace");

    // -- set up stack frame --
    emitter.instruction(&format!("sub sp, sp, #{}", stack_size));               // allocate preg_replace stack frame
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", save_off));         // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", save_off));                // set new frame pointer

    // -- save all inputs --
    emitter.instruction(&format!("str x1, [sp, #{}]", pattern_ptr_off));        // pattern ptr
    emitter.instruction(&format!("str x2, [sp, #{}]", pattern_len_off));        // pattern len
    emitter.instruction(&format!("str x3, [sp, #{}]", replacement_ptr_off));    // replacement ptr
    emitter.instruction(&format!("str x4, [sp, #{}]", replacement_len_off));    // replacement len
    emitter.instruction(&format!("str x5, [sp, #{}]", subject_ptr_off));        // subject ptr
    emitter.instruction(&format!("str x6, [sp, #{}]", subject_len_off));        // subject len

    // -- strip delimiters from pattern --
    emitter.instruction("bl __rt_preg_strip");                                  // → x1, x2, x3=flags
    emitter.instruction(&format!("str x3, [sp, #{}]", flags_off));              // save flags

    // -- convert pattern from PCRE to POSIX and null-terminate --
    emitter.instruction("bl __rt_pcre_to_posix");                               // → x0=C string with PCRE shorthands converted
    emitter.instruction(&format!("str x0, [sp, #{}]", pattern_cstr_off));       // save pattern C string

    // -- compile regex --
    emitter.instruction("mov x0, sp");                                          // regex_t at sp
    emitter.instruction(&format!("ldr x1, [sp, #{}]", pattern_cstr_off));       // pattern
    emitter.instruction("mov x2, #1");                                          // REG_EXTENDED
    emitter.instruction(&format!("ldr x9, [sp, #{}]", flags_off));              // flags
    emitter.instruction("tst x9, #1");                                          // test icase
    emitter.instruction("b.eq __rt_preg_replace_nc");                           // skip
    emitter.instruction("orr x2, x2, #2");                                      // REG_ICASE
    emitter.label("__rt_preg_replace_nc");
    emitter.bl_c("regcomp");                                         // compile
    emitter.instruction("cbnz x0, __rt_preg_replace_fail");                     // fail → return original

    // -- null-terminate subject --
    emitter.instruction(&format!("ldr x1, [sp, #{}]", subject_ptr_off));        // subject ptr
    emitter.instruction(&format!("ldr x2, [sp, #{}]", subject_len_off));        // subject len
    emitter.instruction("bl __rt_cstr2");                                       // → x0=subject C string
    emitter.instruction(&format!("str x0, [sp, #{}]", subject_cstr_off));       // save subject C string

    // -- set up output buffer in concat_buf --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x11, x11, x10");                                   // output position
    emitter.instruction(&format!("str x11, [sp, #{}]", output_start_off));      // save output start
    emitter.instruction(&format!("str x11, [sp, #{}]", output_write_off));      // save output write pos

    // -- initialize current position --
    emitter.instruction(&format!("ldr x9, [sp, #{}]", subject_cstr_off));       // subject C string start
    emitter.instruction(&format!("str x9, [sp, #{}]", current_pos_off));        // save current pos

    // -- replacement loop --
    emitter.label("__rt_preg_replace_loop");
    emitter.instruction(&format!("ldr x1, [sp, #{}]", current_pos_off));        // current pos
    emitter.instruction("ldrb w9, [x1]");                                       // check for end
    emitter.instruction("cbz w9, __rt_preg_replace_done");                      // end of string

    emitter.instruction("mov x0, sp");                                          // regex_t
    emitter.instruction("mov x2, #1");                                          // nmatch
    emitter.instruction(&format!("add x3, sp, #{}", regmatch_off));             // regmatch_t buffer
    emitter.instruction("mov x4, #0");                                          // eflags
    emitter.bl_c("regexec");                                         // execute
    emitter.instruction("cbnz x0, __rt_preg_replace_tail");                     // no more matches, copy rest

    // -- copy text before match (rm_so bytes) --
    emitter.instruction(&emitter.platform.regoff_load_instr("x9", "sp", regmatch_off)); // load rm_so with the native regoff_t width
    emitter.instruction(&format!("ldr x10, [sp, #{}]", current_pos_off));       // current C string pos
    emitter.instruction(&format!("ldr x11, [sp, #{}]", output_write_off));      // output write pos
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
    emitter.instruction(&format!("ldr x1, [sp, #{}]", replacement_ptr_off));    // replacement ptr
    emitter.instruction(&format!("ldr x2, [sp, #{}]", replacement_len_off));    // replacement len
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
    emitter.instruction(&format!("str x11, [sp, #{}]", output_write_off));      // save output write pos
    emitter.instruction(&emitter.platform.regoff_load_instr("x9", "sp", regmatch_off + emitter.platform.regmatch_rm_eo_offset())); // load rm_eo with the native regoff_t width
    emitter.instruction(&format!("ldr x10, [sp, #{}]", current_pos_off));       // current pos
    emitter.instruction("add x10, x10, x9");                                    // advance past match
    emitter.instruction(&format!("str x10, [sp, #{}]", current_pos_off));       // save new pos
    emitter.instruction("b __rt_preg_replace_loop");                            // continue

    // -- copy remaining text after last match --
    emitter.label("__rt_preg_replace_tail");
    emitter.instruction(&format!("ldr x10, [sp, #{}]", current_pos_off));       // current pos
    emitter.instruction(&format!("ldr x11, [sp, #{}]", output_write_off));      // output write pos
    emitter.label("__rt_preg_replace_tail_loop");
    emitter.instruction("ldrb w9, [x10]");                                      // load byte
    emitter.instruction("cbz w9, __rt_preg_replace_done");                      // null terminator = done
    emitter.instruction("strb w9, [x11]");                                      // write byte
    emitter.instruction("add x10, x10, #1");                                    // advance source
    emitter.instruction("add x11, x11, #1");                                    // advance output
    emitter.instruction("b __rt_preg_replace_tail_loop");                       // continue

    emitter.label("__rt_preg_replace_done");
    emitter.instruction(&format!("str x11, [sp, #{}]", output_write_off));      // save final write pos
    // -- free regex --
    emitter.instruction("mov x0, sp");                                          // regex_t
    emitter.bl_c("regfree");                                         // free

    // -- compute result --
    emitter.instruction(&format!("ldr x1, [sp, #{}]", output_start_off));       // output start
    emitter.instruction(&format!("ldr x11, [sp, #{}]", output_write_off));      // output end
    emitter.instruction("sub x2, x11, x1");                                     // result length

    // -- update concat_off --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    emitter.instruction("add x10, x10, x2");                                    // add result length
    emitter.instruction("str x10, [x9]");                                       // store updated offset
    emitter.instruction("b __rt_preg_replace_ret");                             // return

    // -- failure: return original subject --
    emitter.label("__rt_preg_replace_fail");
    emitter.instruction(&format!("ldr x1, [sp, #{}]", subject_ptr_off));        // original subject ptr
    emitter.instruction(&format!("ldr x2, [sp, #{}]", subject_len_off));        // original subject len

    emitter.label("__rt_preg_replace_ret");
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_off));         // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", stack_size));               // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
