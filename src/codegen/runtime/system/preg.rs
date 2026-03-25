use crate::codegen::emit::Emitter;

/// __rt_preg_strip: strip regex delimiters (e.g., "/pattern/i" → "pattern").
/// Input:  x1=pattern ptr, x2=pattern len
/// Output: x1=stripped pattern ptr, x2=stripped len, x3=flags (bit 0=icase)
fn emit_strip_delimiters(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: preg_strip_delimiters ---");
    emitter.label("__rt_preg_strip");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set new frame pointer

    emitter.instruction("str x1, [sp, #0]");                                    // save pattern ptr
    emitter.instruction("str x2, [sp, #8]");                                    // save pattern len
    emitter.instruction("mov x3, #0");                                          // flags = 0
    emitter.instruction("str x3, [sp, #16]");                                   // save flags

    // -- check if pattern starts with '/' --
    emitter.instruction("ldrb w9, [x1]");                                       // load first byte
    emitter.instruction("cmp w9, #47");                                         // compare with '/'
    emitter.instruction("b.ne __rt_preg_strip_done");                           // not delimited, return as-is

    // -- find closing delimiter by scanning from the end --
    emitter.instruction("sub x10, x2, #1");                                     // start from last char
    emitter.label("__rt_preg_strip_scan");
    emitter.instruction("cmp x10, #1");                                         // must have at least 1 char between delimiters
    emitter.instruction("b.lt __rt_preg_strip_done");                           // no closing delimiter found
    emitter.instruction("ldrb w9, [x1, x10]");                                  // load byte at position
    emitter.instruction("cmp w9, #47");                                         // check for closing '/'
    emitter.instruction("b.eq __rt_preg_strip_found");                          // found it

    // -- check for 'i' flag --
    emitter.instruction("cmp w9, #105");                                        // check for 'i'
    emitter.instruction("b.ne __rt_preg_strip_skip_flag");                      // not 'i'
    emitter.instruction("ldr x3, [sp, #16]");                                   // load flags
    emitter.instruction("orr x3, x3, #1");                                      // set icase flag
    emitter.instruction("str x3, [sp, #16]");                                   // save flags
    emitter.label("__rt_preg_strip_skip_flag");
    emitter.instruction("sub x10, x10, #1");                                    // move backward
    emitter.instruction("b __rt_preg_strip_scan");                              // continue scanning

    // -- found closing delimiter at x10 --
    emitter.label("__rt_preg_strip_found");
    emitter.instruction("add x1, x1, #1");                                      // skip opening delimiter
    emitter.instruction("sub x2, x10, #1");                                     // length = closing_pos - 1

    emitter.label("__rt_preg_strip_done");
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload flags

    // -- tear down and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return
}

/// __rt_pcre_to_posix: copy regex pattern to _cstr_buf, converting PCRE shorthands
/// to POSIX equivalents (\s→[[:space:]], \d→[[:digit:]], \w→[[:alnum:]_], and uppercase negations).
/// Input:  x1=pattern ptr, x2=pattern len
/// Output: x0=pointer to null-terminated string in _cstr_buf
fn emit_pcre_to_posix(emitter: &mut Emitter) {
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
pub fn emit_preg_match(emitter: &mut Emitter) {
    emit_strip_delimiters(emitter);
    emit_pcre_to_posix(emitter);

    emitter.blank();
    emitter.comment("--- runtime: preg_match ---");
    emitter.label("__rt_preg_match");

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
pub fn emit_preg_match_all(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: preg_match_all ---");
    emitter.label("__rt_preg_match_all");

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
pub fn emit_preg_replace(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: preg_replace ---");
    emitter.label("__rt_preg_replace");

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
    emitter.instruction("bl _regcomp");                                         // compile
    emitter.instruction("cbnz x0, __rt_preg_replace_fail");                     // fail → return original

    // -- null-terminate subject --
    emitter.instruction("ldr x1, [sp, #80]");                                   // subject ptr
    emitter.instruction("ldr x2, [sp, #88]");                                   // subject len
    emitter.instruction("bl __rt_cstr2");                                       // → x0=subject C string
    emitter.instruction("str x0, [sp, #112]");                                  // save subject C string

    // -- set up output buffer in concat_buf --
    emitter.instruction("adrp x9, _concat_off@PAGE");                           // load page of concat offset
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                     // resolve address
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    emitter.instruction("adrp x11, _concat_buf@PAGE");                          // load page of concat buffer
    emitter.instruction("add x11, x11, _concat_buf@PAGEOFF");                   // resolve address
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
    emitter.instruction("bl _regexec");                                         // execute
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
    emitter.instruction("bl _regfree");                                         // free

    // -- compute result --
    emitter.instruction("ldr x1, [sp, #120]");                                  // output start
    emitter.instruction("ldr x11, [sp, #128]");                                 // output end
    emitter.instruction("sub x2, x11, x1");                                     // result length

    // -- update concat_off --
    emitter.instruction("adrp x9, _concat_off@PAGE");                           // load page of concat offset
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                     // resolve address
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
pub fn emit_preg_split(emitter: &mut Emitter) {
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
