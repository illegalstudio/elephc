use crate::codegen::emit::Emitter;

/// __rt_preg_match: check if a POSIX regex matches a subject string.
/// Input:  x1=pattern ptr, x2=pattern len, x3=subject ptr, x4=subject len
/// Output: x0=1 if match found, 0 if not
///
/// Stack layout (160 bytes):
///   sp+0..31:   regex_t (32 bytes)
///   sp+32..47:  regmatch_t (16 bytes: rm_so at +32, rm_eo at +40)
///   sp+48..55:  pattern ptr
///   sp+56..63:  pattern len
///   sp+64..71:  subject ptr
///   sp+72..79:  subject len
///   sp+80..87:  flags
///   sp+88..95:  pattern C string
///   sp+96..103: subject C string
///   sp+104..111: regexec result
///   sp+112..127: padding
///   sp+128..143: saved x29, x30
pub(crate) fn emit_preg_match(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: preg_match ---");
    emitter.label_global("__rt_preg_match");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #160");                                    // allocate 160 bytes
    emitter.instruction("stp x29, x30, [sp, #144]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #144");                                   // set new frame pointer

    // -- save inputs --
    emitter.instruction("str x1, [sp, #48]");                                   // save pattern ptr
    emitter.instruction("str x2, [sp, #56]");                                   // save pattern len
    emitter.instruction("str x3, [sp, #64]");                                   // save subject ptr
    emitter.instruction("str x4, [sp, #72]");                                   // save subject len

    // -- strip delimiters from pattern --
    emitter.instruction("bl __rt_preg_strip");                                  // → x1=stripped, x2=len, x3=flags
    emitter.instruction("str x3, [sp, #80]");                                   // save flags

    // -- convert pattern from PCRE to POSIX and null-terminate --
    emitter.instruction("bl __rt_pcre_to_posix");                               // → x0=C string with PCRE shorthands converted
    emitter.instruction("str x0, [sp, #88]");                                   // save pattern C string

    // -- compile regex: regcomp(&regex_t, pattern, flags) --
    emitter.instruction("mov x0, sp");                                          // x0 = regex_t at sp+0
    emitter.instruction("ldr x1, [sp, #88]");                                   // x1 = pattern C string
    emitter.instruction("mov x2, #1");                                          // REG_EXTENDED
    emitter.instruction("ldr x9, [sp, #80]");                                   // load flags
    emitter.instruction("tst x9, #1");                                          // test icase
    emitter.instruction("b.eq __rt_preg_match_nocase");                         // skip if no icase
    emitter.instruction("orr x2, x2, #2");                                      // add REG_ICASE
    emitter.label("__rt_preg_match_nocase");
    emitter.instruction("bl _regcomp");                                         // compile regex
    emitter.instruction("cbnz x0, __rt_preg_match_no");                         // compile failed → no match

    // -- null-terminate subject --
    emitter.instruction("ldr x1, [sp, #64]");                                   // load subject ptr
    emitter.instruction("ldr x2, [sp, #72]");                                   // load subject len
    emitter.instruction("bl __rt_cstr2");                                       // → x0=subject C string
    emitter.instruction("str x0, [sp, #96]");                                   // save subject C string

    // -- execute regex: regexec(&regex_t, subject, nmatch, &regmatch_t, eflags) --
    emitter.instruction("mov x0, sp");                                          // x0 = regex_t
    emitter.instruction("ldr x1, [sp, #96]");                                   // x1 = subject C string
    emitter.instruction("mov x2, #1");                                          // nmatch = 1
    emitter.instruction("add x3, sp, #32");                                     // x3 = regmatch_t buffer at sp+32
    emitter.instruction("mov x4, #0");                                          // eflags = 0
    emitter.instruction("bl _regexec");                                         // regexec → x0=0 if match
    emitter.instruction("str x0, [sp, #104]");                                  // save regexec result

    // -- free compiled regex --
    emitter.instruction("mov x0, sp");                                          // x0 = regex_t
    emitter.instruction("bl _regfree");                                         // free compiled regex

    // -- return result --
    emitter.instruction("ldr x0, [sp, #104]");                                  // reload regexec result
    emitter.instruction("cbnz x0, __rt_preg_match_no");                         // non-zero = no match
    emitter.instruction("mov x0, #1");                                          // matched → return 1
    emitter.instruction("b __rt_preg_match_ret");                               // return

    emitter.label("__rt_preg_match_no");
    emitter.instruction("mov x0, #0");                                          // no match → return 0

    emitter.label("__rt_preg_match_ret");
    emitter.instruction("ldp x29, x30, [sp, #144]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #160");                                    // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
