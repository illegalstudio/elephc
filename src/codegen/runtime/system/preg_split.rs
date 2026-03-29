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
    emitter.blank();
    emitter.comment("--- runtime: preg_split ---");
    emitter.label("__rt_preg_split");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #224");                                    // allocate 224 bytes
    emitter.instruction("stp x29, x30, [sp, #208]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #208");                                   // set new frame pointer

    // -- save inputs --
    emitter.instruction("str x1, [sp, #48]");                                   // pattern ptr
    emitter.instruction("str x2, [sp, #56]");                                   // pattern len
    emitter.instruction("str x3, [sp, #64]");                                   // subject ptr (elephc)
    emitter.instruction("str x4, [sp, #72]");                                   // subject len

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
    emitter.instruction("b.eq __rt_preg_split_nc");                             // skip
    emitter.instruction("orr x2, x2, #2");                                      // REG_ICASE
    emitter.label("__rt_preg_split_nc");
    emitter.instruction("bl _regcomp");                                         // compile
    emitter.instruction("cbnz x0, __rt_preg_split_fail");                       // fail

    // -- create new string array --
    emitter.instruction("mov x0, #8");                                          // initial capacity
    emitter.instruction("mov x1, #16");                                         // element size = 16 (ptr + len for strings)
    emitter.instruction("bl __rt_array_new");                                   // create array → x0
    emitter.instruction("str x0, [sp, #96]");                                   // save array ptr

    // -- null-terminate subject --
    emitter.instruction("ldr x1, [sp, #64]");                                   // subject ptr
    emitter.instruction("ldr x2, [sp, #72]");                                   // subject len
    emitter.instruction("bl __rt_cstr2");                                       // → x0=subject C string
    emitter.instruction("str x0, [sp, #104]");                                  // save subject C string

    // -- initialize positions --
    emitter.instruction("ldr x9, [sp, #104]");                                  // C string start
    emitter.instruction("str x9, [sp, #112]");                                  // current C string pos
    emitter.instruction("ldr x9, [sp, #64]");                                   // elephc ptr start
    emitter.instruction("str x9, [sp, #120]");                                  // current elephc ptr

    // -- split loop --
    emitter.label("__rt_preg_split_loop");
    emitter.instruction("ldr x1, [sp, #112]");                                  // current C string pos
    emitter.instruction("ldrb w9, [x1]");                                       // check for end
    emitter.instruction("cbz w9, __rt_preg_split_last");                        // end of string, add final segment

    emitter.instruction("mov x0, sp");                                          // regex_t
    emitter.instruction("mov x2, #1");                                          // nmatch
    emitter.instruction("add x3, sp, #32");                                     // regmatch_t at sp+32
    emitter.instruction("mov x4, #0");                                          // eflags
    emitter.instruction("bl _regexec");                                         // execute
    emitter.instruction("cbnz x0, __rt_preg_split_last");                       // no more matches

    // -- add segment before match to array --
    emitter.instruction("ldr x9, [sp, #32]");                                   // rm_so (8-byte at sp+32)
    emitter.instruction("ldr x0, [sp, #96]");                                   // array ptr
    emitter.instruction("ldr x1, [sp, #120]");                                  // current elephc ptr
    emitter.instruction("mov x2, x9");                                          // segment length = rm_so
    emitter.instruction("bl __rt_array_push_str");                              // push string to array
    emitter.instruction("str x0, [sp, #96]");                                   // save (possibly reallocated) array ptr

    // -- advance past match --
    emitter.instruction("ldr x9, [sp, #40]");                                   // rm_eo (8-byte at sp+32+8)
    emitter.instruction("ldr x10, [sp, #112]");                                 // current C string pos
    emitter.instruction("add x10, x10, x9");                                    // advance C string pos
    emitter.instruction("str x10, [sp, #112]");                                 // save
    emitter.instruction("ldr x10, [sp, #120]");                                 // current elephc ptr
    emitter.instruction("add x10, x10, x9");                                    // advance elephc ptr
    emitter.instruction("str x10, [sp, #120]");                                 // save
    emitter.instruction("b __rt_preg_split_loop");                              // continue

    // -- add last segment --
    emitter.label("__rt_preg_split_last");
    emitter.instruction("ldr x10, [sp, #120]");                                 // current elephc ptr
    emitter.instruction("ldr x11, [sp, #64]");                                  // original subject ptr
    emitter.instruction("ldr x12, [sp, #72]");                                  // original subject len
    emitter.instruction("add x11, x11, x12");                                   // end of subject
    emitter.instruction("sub x2, x11, x10");                                    // remaining length

    emitter.instruction("ldr x0, [sp, #96]");                                   // array ptr
    emitter.instruction("mov x1, x10");                                         // segment ptr
    emitter.instruction("bl __rt_array_push_str");                              // push last segment
    emitter.instruction("str x0, [sp, #96]");                                   // save array ptr

    // -- free regex and return --
    emitter.instruction("mov x0, sp");                                          // regex_t
    emitter.instruction("bl _regfree");                                         // free
    emitter.instruction("ldr x0, [sp, #96]");                                   // return array ptr
    emitter.instruction("b __rt_preg_split_ret");                               // return

    // -- failure: return empty array --
    emitter.label("__rt_preg_split_fail");
    emitter.instruction("mov x0, #4");                                          // small capacity
    emitter.instruction("mov x1, #16");                                         // element size = 16 for string array
    emitter.instruction("bl __rt_array_new");                                   // create empty array

    emitter.label("__rt_preg_split_ret");
    emitter.instruction("ldp x29, x30, [sp, #208]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #224");                                    // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
