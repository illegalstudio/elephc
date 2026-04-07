use crate::codegen::emit::Emitter;

/// __rt_preg_match_all: count all non-overlapping matches of regex in subject.
/// Input:  x1=pattern ptr, x2=pattern len, x3=subject ptr, x4=subject len
/// Output: x0=match count
///
/// Stack layout (176 bytes):
///   sp+0..31:   regex_t (32 bytes)
///   sp+32..47:  regmatch_t (16 bytes)
///   sp+48..55:  pattern ptr
///   sp+56..63:  pattern len
///   sp+64..71:  subject ptr
///   sp+72..79:  subject len
///   sp+80..87:  flags
///   sp+88..95:  pattern C string
///   sp+96..103: subject C string
///   sp+104..111: match count
///   sp+112..119: current position in C string
///   sp+128..143: padding
///   sp+144..159: saved x29, x30
pub(crate) fn emit_preg_match_all(emitter: &mut Emitter) {
    let regex_t_size = emitter.platform.regex_t_size();
    let regmatch_off = regex_t_size;
    let pattern_ptr_off = regmatch_off + emitter.platform.regmatch_t_size();
    let pattern_len_off = pattern_ptr_off + 8;
    let subject_ptr_off = pattern_len_off + 8;
    let subject_len_off = subject_ptr_off + 8;
    let flags_off = subject_len_off + 8;
    let pattern_cstr_off = flags_off + 8;
    let subject_cstr_off = pattern_cstr_off + 8;
    let match_count_off = subject_cstr_off + 8;
    let current_pos_off = match_count_off + 8;
    let stack_size = (current_pos_off + 48 + 15) & !15;
    let save_off = stack_size - 16;

    emitter.blank();
    emitter.comment("--- runtime: preg_match_all ---");
    emitter.label_global("__rt_preg_match_all");

    // -- set up stack frame --
    emitter.instruction(&format!("sub sp, sp, #{}", stack_size));               // allocate preg_match_all stack frame
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", save_off));         // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", save_off));                // set new frame pointer

    // -- save inputs --
    emitter.instruction(&format!("str x1, [sp, #{}]", pattern_ptr_off));        // save pattern ptr
    emitter.instruction(&format!("str x2, [sp, #{}]", pattern_len_off));        // save pattern len
    emitter.instruction(&format!("str x3, [sp, #{}]", subject_ptr_off));        // save subject ptr
    emitter.instruction(&format!("str x4, [sp, #{}]", subject_len_off));        // save subject len

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
    emitter.instruction("b.eq __rt_preg_match_all_nc");                         // skip
    emitter.instruction("orr x2, x2, #2");                                      // REG_ICASE
    emitter.label("__rt_preg_match_all_nc");
    emitter.bl_c("regcomp");                                         // compile
    emitter.instruction("cbnz x0, __rt_preg_match_all_fail");                   // fail

    // -- null-terminate subject --
    emitter.instruction(&format!("ldr x1, [sp, #{}]", subject_ptr_off));        // subject ptr
    emitter.instruction(&format!("ldr x2, [sp, #{}]", subject_len_off));        // subject len
    emitter.instruction("bl __rt_cstr2");                                       // → x0=subject C string
    emitter.instruction(&format!("str x0, [sp, #{}]", subject_cstr_off));       // save subject C string

    // -- count matches loop --
    emitter.instruction(&format!("str xzr, [sp, #{}]", match_count_off));       // match count = 0
    emitter.instruction(&format!("ldr x9, [sp, #{}]", subject_cstr_off));       // current position = start
    emitter.instruction(&format!("str x9, [sp, #{}]", current_pos_off));        // save current pos

    emitter.label("__rt_preg_match_all_loop");
    emitter.instruction(&format!("ldr x1, [sp, #{}]", current_pos_off));        // current subject position
    emitter.instruction("ldrb w9, [x1]");                                       // load byte at current pos
    emitter.instruction("cbz w9, __rt_preg_match_all_done");                    // null terminator = done
    emitter.instruction("mov x0, sp");                                          // regex_t
    emitter.instruction("mov x2, #1");                                          // nmatch = 1
    emitter.instruction(&format!("add x3, sp, #{}", regmatch_off));             // regmatch_t buffer
    emitter.instruction("mov x4, #0");                                          // eflags
    emitter.bl_c("regexec");                                         // execute
    emitter.instruction("cbnz x0, __rt_preg_match_all_done");                   // no more matches

    // -- found a match, increment count --
    emitter.instruction(&format!("ldr x9, [sp, #{}]", match_count_off));        // load count
    emitter.instruction("add x9, x9, #1");                                      // increment
    emitter.instruction(&format!("str x9, [sp, #{}]", match_count_off));        // save count

    // -- advance past this match (rm_eo is 8 bytes at sp+40) --
    emitter.instruction(&format!("ldr x10, [sp, #{}]", current_pos_off));       // current pos
    emitter.instruction(&emitter.platform.regoff_load_instr("x11", "sp", regmatch_off + emitter.platform.regmatch_rm_eo_offset())); // load rm_eo with the native regoff_t width
    emitter.instruction("cmp x11, #0");                                         // check for zero-length match
    emitter.instruction("b.gt __rt_preg_match_all_adv");                        // non-zero advance
    emitter.instruction("mov x11, #1");                                         // advance by at least 1
    emitter.label("__rt_preg_match_all_adv");
    emitter.instruction("add x10, x10, x11");                                   // advance position
    emitter.instruction(&format!("str x10, [sp, #{}]", current_pos_off));       // save new position
    emitter.instruction("b __rt_preg_match_all_loop");                          // continue

    emitter.label("__rt_preg_match_all_done");
    emitter.instruction("mov x0, sp");                                          // regex_t
    emitter.bl_c("regfree");                                         // free
    emitter.instruction(&format!("ldr x0, [sp, #{}]", match_count_off));        // return count
    emitter.instruction("b __rt_preg_match_all_ret");                           // return

    emitter.label("__rt_preg_match_all_fail");
    emitter.instruction("mov x0, #0");                                          // return 0 on compile failure

    emitter.label("__rt_preg_match_all_ret");
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_off));         // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", stack_size));               // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
