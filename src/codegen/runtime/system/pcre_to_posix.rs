use crate::codegen::emit::Emitter;

/// __rt_pcre_to_posix: copy regex pattern to _cstr_buf, converting PCRE shorthands
/// to POSIX equivalents (\s→[[:space:]], \d→[[:digit:]], \w→[[:alnum:]_], and uppercase negations).
/// Input:  x1=pattern ptr, x2=pattern len
/// Output: x0=pointer to null-terminated string in _cstr_buf
pub(crate) fn emit_pcre_to_posix(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: pcre_to_posix ---");
    emitter.label("__rt_pcre_to_posix");

    // -- load destination buffer address --
    emitter.instruction("adrp x9, _cstr_buf@PAGE");                             // load page address of cstr scratch buffer
    emitter.instruction("add x9, x9, _cstr_buf@PAGEOFF");                       // resolve exact address of cstr buffer
    emitter.instruction("mov x10, x9");                                         // save buffer start for return value
    emitter.instruction("add x11, x1, x2");                                     // x11 = end of source (ptr + len)

    // -- main scan loop --
    emitter.label("__rt_p2p_loop");
    emitter.instruction("cmp x1, x11");                                         // check if source exhausted
    emitter.instruction("b.ge __rt_p2p_done");                                  // done scanning
    emitter.instruction("ldrb w12, [x1]");                                      // load current byte
    emitter.instruction("cmp w12, #92");                                        // check for backslash (0x5C)
    emitter.instruction("b.ne __rt_p2p_copy");                                  // not backslash, copy as-is

    // -- backslash found: check if next char is a PCRE shorthand --
    emitter.instruction("add x13, x1, #1");                                     // peek at next byte position
    emitter.instruction("cmp x13, x11");                                        // check bounds
    emitter.instruction("b.ge __rt_p2p_copy");                                  // at end, copy backslash as-is
    emitter.instruction("ldrb w14, [x13]");                                     // load next byte after backslash

    // -- check lowercase shorthands --
    emitter.instruction("cmp w14, #115");                                       // check for 's' (0x73)
    emitter.instruction("b.eq __rt_p2p_space");                                 // \s → [[:space:]]
    emitter.instruction("cmp w14, #100");                                       // check for 'd' (0x64)
    emitter.instruction("b.eq __rt_p2p_digit");                                 // \d → [[:digit:]]
    emitter.instruction("cmp w14, #119");                                       // check for 'w' (0x77)
    emitter.instruction("b.eq __rt_p2p_word");                                  // \w → [[:alnum:]_]

    // -- check uppercase shorthands (negated) --
    emitter.instruction("cmp w14, #83");                                        // check for 'S' (0x53)
    emitter.instruction("b.eq __rt_p2p_nspace");                                // \S → [^[:space:]]
    emitter.instruction("cmp w14, #68");                                        // check for 'D' (0x44)
    emitter.instruction("b.eq __rt_p2p_ndigit");                                // \D → [^[:digit:]]
    emitter.instruction("cmp w14, #87");                                        // check for 'W' (0x57)
    emitter.instruction("b.eq __rt_p2p_nword");                                 // \W → [^[:alnum:]_]

    // -- not a PCRE shorthand, copy backslash as-is --
    emitter.label("__rt_p2p_copy");
    emitter.instruction("strb w12, [x9], #1");                                  // store byte to buffer, advance
    emitter.instruction("add x1, x1, #1");                                      // advance source ptr
    emitter.instruction("b __rt_p2p_loop");                                     // continue scanning

    // -- \s → [[:space:]] (11 bytes) --
    emitter.label("__rt_p2p_space");
    emitter.instruction("adrp x15, _pcre_space@PAGE");                          // load page of replacement string
    emitter.instruction("add x15, x15, _pcre_space@PAGEOFF");                   // resolve address
    emitter.instruction("mov x16, #11");                                        // replacement length = 11
    emitter.instruction("b __rt_p2p_replace");                                  // go to copy routine

    // -- \d → [[:digit:]] (11 bytes) --
    emitter.label("__rt_p2p_digit");
    emitter.instruction("adrp x15, _pcre_digit@PAGE");                          // load page of replacement string
    emitter.instruction("add x15, x15, _pcre_digit@PAGEOFF");                   // resolve address
    emitter.instruction("mov x16, #11");                                        // replacement length = 11
    emitter.instruction("b __rt_p2p_replace");                                  // go to copy routine

    // -- \w → [[:alnum:]_] (12 bytes) --
    emitter.label("__rt_p2p_word");
    emitter.instruction("adrp x15, _pcre_word@PAGE");                           // load page of replacement string
    emitter.instruction("add x15, x15, _pcre_word@PAGEOFF");                    // resolve address
    emitter.instruction("mov x16, #12");                                        // replacement length = 12
    emitter.instruction("b __rt_p2p_replace");                                  // go to copy routine

    // -- \S → [^[:space:]] (12 bytes) --
    emitter.label("__rt_p2p_nspace");
    emitter.instruction("adrp x15, _pcre_nspace@PAGE");                         // load page of replacement string
    emitter.instruction("add x15, x15, _pcre_nspace@PAGEOFF");                  // resolve address
    emitter.instruction("mov x16, #12");                                        // replacement length = 12
    emitter.instruction("b __rt_p2p_replace");                                  // go to copy routine

    // -- \D → [^[:digit:]] (12 bytes) --
    emitter.label("__rt_p2p_ndigit");
    emitter.instruction("adrp x15, _pcre_ndigit@PAGE");                         // load page of replacement string
    emitter.instruction("add x15, x15, _pcre_ndigit@PAGEOFF");                  // resolve address
    emitter.instruction("mov x16, #12");                                        // replacement length = 12
    emitter.instruction("b __rt_p2p_replace");                                  // go to copy routine

    // -- \W → [^[:alnum:]_] (13 bytes) --
    emitter.label("__rt_p2p_nword");
    emitter.instruction("adrp x15, _pcre_nword@PAGE");                          // load page of replacement string
    emitter.instruction("add x15, x15, _pcre_nword@PAGEOFF");                   // resolve address
    emitter.instruction("mov x16, #13");                                        // replacement length = 13

    // -- copy replacement string to output buffer --
    emitter.label("__rt_p2p_replace");
    emitter.instruction("mov x17, #0");                                         // copy index = 0
    emitter.label("__rt_p2p_repl_loop");
    emitter.instruction("cmp x17, x16");                                        // check if all bytes copied
    emitter.instruction("b.ge __rt_p2p_repl_done");                             // done with replacement
    emitter.instruction("ldrb w12, [x15, x17]");                                // load replacement byte
    emitter.instruction("strb w12, [x9], #1");                                  // store to output, advance
    emitter.instruction("add x17, x17, #1");                                    // increment copy index
    emitter.instruction("b __rt_p2p_repl_loop");                                // continue copying

    emitter.label("__rt_p2p_repl_done");
    emitter.instruction("add x1, x1, #2");                                      // skip both backslash and shorthand char
    emitter.instruction("b __rt_p2p_loop");                                     // continue scanning

    // -- null-terminate and return --
    emitter.label("__rt_p2p_done");
    emitter.instruction("strb wzr, [x9]");                                      // write null terminator
    emitter.instruction("mov x0, x10");                                         // return pointer to converted string
    emitter.instruction("ret");                                                 // return to caller
}
