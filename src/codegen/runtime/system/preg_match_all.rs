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
    emitter.blank();
    emitter.comment("--- runtime: preg_match_all ---");
    emitter.label_global("__rt_preg_match_all");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #176");                                    // allocate 176 bytes
    emitter.instruction("stp x29, x30, [sp, #160]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #160");                                   // set new frame pointer

    // -- save inputs --
    emitter.instruction("str x1, [sp, #48]");                                   // save pattern ptr
    emitter.instruction("str x2, [sp, #56]");                                   // save pattern len
    emitter.instruction("str x3, [sp, #64]");                                   // save subject ptr
    emitter.instruction("str x4, [sp, #72]");                                   // save subject len

    // -- strip delimiters --
    emitter.instruction("bl __rt_preg_strip");                                  // → x1, x2, x3=flags
    emitter.instruction("str x3, [sp, #80]");                                   // save flags

    // -- convert pattern from PCRE to POSIX and null-terminate --
    emitter.instruction("bl __rt_pcre_to_posix");                               // → x0=C string with PCRE shorthands converted
    emitter.instruction("str x0, [sp, #88]");                                   // save pattern C string

    // -- compile regex --
    emitter.instruction("mov x0, sp");                                          // regex_t at sp
    emitter.instruction("ldr x1, [sp, #88]");                                   // pattern
    emitter.instruction("mov x2, #1");                                          // REG_EXTENDED
    emitter.instruction("ldr x9, [sp, #80]");                                   // flags
    emitter.instruction("tst x9, #1");                                          // test icase
    emitter.instruction("b.eq __rt_preg_match_all_nc");                         // skip
    emitter.instruction("orr x2, x2, #2");                                      // REG_ICASE
    emitter.label("__rt_preg_match_all_nc");
    emitter.instruction("bl _regcomp");                                         // compile
    emitter.instruction("cbnz x0, __rt_preg_match_all_fail");                   // fail

    // -- null-terminate subject --
    emitter.instruction("ldr x1, [sp, #64]");                                   // subject ptr
    emitter.instruction("ldr x2, [sp, #72]");                                   // subject len
    emitter.instruction("bl __rt_cstr2");                                       // → x0=subject C string
    emitter.instruction("str x0, [sp, #96]");                                   // save subject C string

    // -- count matches loop --
    emitter.instruction("str xzr, [sp, #104]");                                 // match count = 0
    emitter.instruction("ldr x9, [sp, #96]");                                   // current position = start
    emitter.instruction("str x9, [sp, #112]");                                  // save current pos

    emitter.label("__rt_preg_match_all_loop");
    emitter.instruction("ldr x1, [sp, #112]");                                  // current subject position
    emitter.instruction("ldrb w9, [x1]");                                       // load byte at current pos
    emitter.instruction("cbz w9, __rt_preg_match_all_done");                    // null terminator = done
    emitter.instruction("mov x0, sp");                                          // regex_t
    emitter.instruction("mov x2, #1");                                          // nmatch = 1
    emitter.instruction("add x3, sp, #32");                                     // regmatch_t at sp+32
    emitter.instruction("mov x4, #0");                                          // eflags
    emitter.instruction("bl _regexec");                                         // execute
    emitter.instruction("cbnz x0, __rt_preg_match_all_done");                   // no more matches

    // -- found a match, increment count --
    emitter.instruction("ldr x9, [sp, #104]");                                  // load count
    emitter.instruction("add x9, x9, #1");                                      // increment
    emitter.instruction("str x9, [sp, #104]");                                  // save count

    // -- advance past this match (rm_eo is 8 bytes at sp+40) --
    emitter.instruction("ldr x10, [sp, #112]");                                 // current pos
    emitter.instruction("ldr x11, [sp, #40]");                                  // rm_eo (8-byte regoff_t at sp+32+8)
    emitter.instruction("cmp x11, #0");                                         // check for zero-length match
    emitter.instruction("b.gt __rt_preg_match_all_adv");                        // non-zero advance
    emitter.instruction("mov x11, #1");                                         // advance by at least 1
    emitter.label("__rt_preg_match_all_adv");
    emitter.instruction("add x10, x10, x11");                                   // advance position
    emitter.instruction("str x10, [sp, #112]");                                 // save new position
    emitter.instruction("b __rt_preg_match_all_loop");                          // continue

    emitter.label("__rt_preg_match_all_done");
    emitter.instruction("mov x0, sp");                                          // regex_t
    emitter.instruction("bl _regfree");                                         // free
    emitter.instruction("ldr x0, [sp, #104]");                                  // return count
    emitter.instruction("b __rt_preg_match_all_ret");                           // return

    emitter.label("__rt_preg_match_all_fail");
    emitter.instruction("mov x0, #0");                                          // return 0 on compile failure

    emitter.label("__rt_preg_match_all_ret");
    emitter.instruction("ldp x29, x30, [sp, #160]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #176");                                    // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
