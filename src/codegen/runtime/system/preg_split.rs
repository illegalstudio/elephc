use crate::codegen::emit::Emitter;

/// __rt_preg_split: split a string by regex pattern.
/// Input:  x1=pattern ptr, x2=pattern len, x3=subject ptr, x4=subject len
/// Output: x0=array pointer (string array)
///
/// Stack layout (224 bytes):
///   sp+0..31:    regex_t (32 bytes)
///   sp+32..47:   regmatch_t (16 bytes)
///   sp+48..55:   pattern ptr
///   sp+56..63:   pattern len
///   sp+64..71:   subject ptr (elephc)
///   sp+72..79:   subject len
///   sp+80..87:   flags
///   sp+88..95:   pattern C string
///   sp+96..103:  array ptr
///   sp+104..111: subject C string
///   sp+112..119: current C string pos
///   sp+120..127: current elephc ptr
///   sp+128..191: padding
///   sp+192..207: saved x29, x30
pub(crate) fn emit_preg_split(emitter: &mut Emitter) {
    let regex_t_size = emitter.platform.regex_t_size();
    let regmatch_off = regex_t_size;
    let pattern_ptr_off = regmatch_off + emitter.platform.regmatch_t_size();
    let pattern_len_off = pattern_ptr_off + 8;
    let subject_ptr_off = pattern_len_off + 8;
    let subject_len_off = subject_ptr_off + 8;
    let flags_off = subject_len_off + 8;
    let pattern_cstr_off = flags_off + 8;
    let array_ptr_off = pattern_cstr_off + 8;
    let subject_cstr_off = array_ptr_off + 8;
    let current_cstr_off = subject_cstr_off + 8;
    let current_elephc_off = current_cstr_off + 8;
    let stack_size = (current_elephc_off + 96 + 15) & !15;
    let save_off = stack_size - 16;

    emitter.blank();
    emitter.comment("--- runtime: preg_split ---");
    emitter.label_global("__rt_preg_split");

    // -- set up stack frame --
    emitter.instruction(&format!("sub sp, sp, #{}", stack_size));               // allocate preg_split stack frame
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", save_off));         // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", save_off));                // set new frame pointer

    // -- save inputs --
    emitter.instruction(&format!("str x1, [sp, #{}]", pattern_ptr_off));        // pattern ptr
    emitter.instruction(&format!("str x2, [sp, #{}]", pattern_len_off));        // pattern len
    emitter.instruction(&format!("str x3, [sp, #{}]", subject_ptr_off));        // subject ptr (elephc)
    emitter.instruction(&format!("str x4, [sp, #{}]", subject_len_off));        // subject len

    // -- strip delimiters --
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
    emitter.instruction("b.eq __rt_preg_split_nc");                             // skip
    emitter.instruction("orr x2, x2, #2");                                      // REG_ICASE
    emitter.label("__rt_preg_split_nc");
    emitter.bl_c("regcomp");                                         // compile
    emitter.instruction("cbnz x0, __rt_preg_split_fail");                       // fail

    // -- create new string array --
    emitter.instruction("mov x0, #8");                                          // initial capacity
    emitter.instruction("mov x1, #16");                                         // element size = 16 (ptr + len for strings)
    emitter.instruction("bl __rt_array_new");                                   // create array → x0
    emitter.instruction(&format!("str x0, [sp, #{}]", array_ptr_off));          // save array ptr

    // -- null-terminate subject --
    emitter.instruction(&format!("ldr x1, [sp, #{}]", subject_ptr_off));        // subject ptr
    emitter.instruction(&format!("ldr x2, [sp, #{}]", subject_len_off));        // subject len
    emitter.instruction("bl __rt_cstr2");                                       // → x0=subject C string
    emitter.instruction(&format!("str x0, [sp, #{}]", subject_cstr_off));       // save subject C string

    // -- initialize positions --
    emitter.instruction(&format!("ldr x9, [sp, #{}]", subject_cstr_off));       // C string start
    emitter.instruction(&format!("str x9, [sp, #{}]", current_cstr_off));       // current C string pos
    emitter.instruction(&format!("ldr x9, [sp, #{}]", subject_ptr_off));        // elephc ptr start
    emitter.instruction(&format!("str x9, [sp, #{}]", current_elephc_off));     // current elephc ptr

    // -- split loop --
    emitter.label("__rt_preg_split_loop");
    emitter.instruction(&format!("ldr x1, [sp, #{}]", current_cstr_off));       // current C string pos
    emitter.instruction("ldrb w9, [x1]");                                       // check for end
    emitter.instruction("cbz w9, __rt_preg_split_last");                        // end of string, add final segment

    emitter.instruction("mov x0, sp");                                          // regex_t
    emitter.instruction("mov x2, #1");                                          // nmatch
    emitter.instruction(&format!("add x3, sp, #{}", regmatch_off));             // regmatch_t buffer
    emitter.instruction("mov x4, #0");                                          // eflags
    emitter.bl_c("regexec");                                         // execute
    emitter.instruction("cbnz x0, __rt_preg_split_last");                       // no more matches

    // -- add segment before match to array --
    emitter.instruction(&emitter.platform.regoff_load_instr("x9", "sp", regmatch_off)); // load rm_so with the native regoff_t width
    emitter.instruction(&format!("ldr x0, [sp, #{}]", array_ptr_off));          // array ptr
    emitter.instruction(&format!("ldr x1, [sp, #{}]", current_elephc_off));     // current elephc ptr
    emitter.instruction("mov x2, x9");                                          // segment length = rm_so
    emitter.instruction("bl __rt_array_push_str");                              // push string to array
    emitter.instruction(&format!("str x0, [sp, #{}]", array_ptr_off));          // save (possibly reallocated) array ptr

    // -- advance past match --
    emitter.instruction(&emitter.platform.regoff_load_instr("x9", "sp", regmatch_off + emitter.platform.regmatch_rm_eo_offset())); // load rm_eo with the native regoff_t width
    emitter.instruction(&format!("ldr x10, [sp, #{}]", current_cstr_off));      // current C string pos
    emitter.instruction("add x10, x10, x9");                                    // advance C string pos
    emitter.instruction(&format!("str x10, [sp, #{}]", current_cstr_off));      // save
    emitter.instruction(&format!("ldr x10, [sp, #{}]", current_elephc_off));    // current elephc ptr
    emitter.instruction("add x10, x10, x9");                                    // advance elephc ptr
    emitter.instruction(&format!("str x10, [sp, #{}]", current_elephc_off));    // save
    emitter.instruction("b __rt_preg_split_loop");                              // continue

    // -- add last segment --
    emitter.label("__rt_preg_split_last");
    emitter.instruction(&format!("ldr x10, [sp, #{}]", current_elephc_off));    // current elephc ptr
    emitter.instruction(&format!("ldr x11, [sp, #{}]", subject_ptr_off));       // original subject ptr
    emitter.instruction(&format!("ldr x12, [sp, #{}]", subject_len_off));       // original subject len
    emitter.instruction("add x11, x11, x12");                                   // end of subject
    emitter.instruction("sub x2, x11, x10");                                    // remaining length

    emitter.instruction(&format!("ldr x0, [sp, #{}]", array_ptr_off));          // array ptr
    emitter.instruction("mov x1, x10");                                         // segment ptr
    emitter.instruction("bl __rt_array_push_str");                              // push last segment
    emitter.instruction(&format!("str x0, [sp, #{}]", array_ptr_off));          // save array ptr

    // -- free regex and return --
    emitter.instruction("mov x0, sp");                                          // regex_t
    emitter.bl_c("regfree");                                         // free
    emitter.instruction(&format!("ldr x0, [sp, #{}]", array_ptr_off));          // return array ptr
    emitter.instruction("b __rt_preg_split_ret");                               // return

    // -- failure: return empty array --
    emitter.label("__rt_preg_split_fail");
    emitter.instruction("mov x0, #4");                                          // small capacity
    emitter.instruction("mov x1, #16");                                         // element size = 16 for string array
    emitter.instruction("bl __rt_array_new");                                   // create empty array

    emitter.label("__rt_preg_split_ret");
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_off));         // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", stack_size));               // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
