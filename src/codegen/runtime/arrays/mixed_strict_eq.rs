use crate::codegen::emit::Emitter;

/// mixed_strict_eq: compare two boxed mixed values by runtime tag and payload.
/// Input:  x0 = left mixed pointer, x1 = right mixed pointer
/// Output: x0 = 1 if strictly equal, else 0
pub fn emit_mixed_strict_eq(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_strict_eq ---");
    emitter.label_global("__rt_mixed_strict_eq");

    // -- save both mixed operands across helper calls --
    emitter.instruction("sub sp, sp, #64");                                     // allocate stack space for both operands, payloads, and saved frame state
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper stack frame
    emitter.instruction("stp x0, x1, [sp, #0]");                                // save the incoming left/right mixed pointers

    // -- unbox the left payload --
    emitter.instruction("bl __rt_mixed_unbox");                                 // left mixed pointer -> x0=tag, x1=value_lo, x2=value_hi
    emitter.instruction("str x0, [sp, #16]");                                   // save the left runtime tag
    emitter.instruction("stp x1, x2, [sp, #24]");                               // save the left payload words

    // -- unbox the right payload --
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the right mixed pointer into the helper argument register
    emitter.instruction("bl __rt_mixed_unbox");                                 // right mixed pointer -> x0=tag, x1=value_lo, x2=value_hi
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the saved left runtime tag
    emitter.instruction("cmp x9, x0");                                          // strict equality first requires matching runtime tags
    emitter.instruction("b.ne __rt_mixed_strict_eq_false");                     // different payload tags are never strictly equal

    // -- dispatch on the shared concrete runtime tag --
    emitter.instruction("cmp x0, #1");                                          // do both payloads hold strings?
    emitter.instruction("b.eq __rt_mixed_strict_eq_string");                    // strings need byte-by-byte comparison
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the left payload low word
    emitter.instruction("cmp x10, x1");                                         // compare low payload words for scalar/pointer tags
    emitter.instruction("b.ne __rt_mixed_strict_eq_false");                     // mismatched payload low words are not equal
    emitter.instruction("ldr x11, [sp, #32]");                                  // reload the left payload high word
    emitter.instruction("cmp x11, x2");                                         // compare high payload words for string/null padding
    emitter.instruction("b.ne __rt_mixed_strict_eq_false");                     // mismatched payload high words are not equal
    emitter.instruction("mov x0, #1");                                          // matching tag + payload words means strict equality
    emitter.instruction("b __rt_mixed_strict_eq_done");                         // return true after the scalar/pointer comparison

    // -- strings compare by bytes, not by pointer identity --
    emitter.label("__rt_mixed_strict_eq_string");
    emitter.instruction("mov x3, x1");                                          // move right string pointer into the third string-equality argument slot
    emitter.instruction("mov x4, x2");                                          // move right string length into the fourth string-equality argument slot
    emitter.instruction("ldp x1, x2, [sp, #24]");                               // reload the left string pointer/length into the first two argument slots
    emitter.instruction("bl __rt_str_eq");                                      // compare the two string payloads byte-for-byte
    emitter.instruction("b __rt_mixed_strict_eq_done");                         // return the string comparison result

    emitter.label("__rt_mixed_strict_eq_false");
    emitter.instruction("mov x0, #0");                                          // report that the mixed payloads are not strictly equal

    emitter.label("__rt_mixed_strict_eq_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper stack frame
    emitter.instruction("ret");                                                 // return the strict-equality boolean in x0
}
