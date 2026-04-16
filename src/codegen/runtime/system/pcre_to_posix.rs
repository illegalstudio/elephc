use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// __rt_pcre_to_posix: copy regex pattern to _cstr_buf, converting PCRE shorthands
/// to POSIX equivalents (\s→[[:space:]], \d→[[:digit:]], \w→[[:alnum:]_], and uppercase negations).
/// Input:  x1=pattern ptr, x2=pattern len
/// Output: x0=pointer to null-terminated string in _cstr_buf
pub(crate) fn emit_pcre_to_posix(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_pcre_to_posix_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: pcre_to_posix ---");
    emitter.label_global("__rt_pcre_to_posix");

    // -- load destination buffer address --
    emitter.adrp("x9", "_cstr_buf");                                            // load page address of cstr scratch buffer
    emitter.add_lo12("x9", "x9", "_cstr_buf");                                  // resolve exact address of cstr buffer
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
    emitter.adrp("x15", "_pcre_space");                                         // load page of replacement string
    emitter.add_lo12("x15", "x15", "_pcre_space");                              // resolve address
    emitter.instruction("mov x16, #11");                                        // replacement length = 11
    emitter.instruction("b __rt_p2p_replace");                                  // go to copy routine

    // -- \d → [[:digit:]] (11 bytes) --
    emitter.label("__rt_p2p_digit");
    emitter.adrp("x15", "_pcre_digit");                                         // load page of replacement string
    emitter.add_lo12("x15", "x15", "_pcre_digit");                              // resolve address
    emitter.instruction("mov x16, #11");                                        // replacement length = 11
    emitter.instruction("b __rt_p2p_replace");                                  // go to copy routine

    // -- \w → [[:alnum:]_] (12 bytes) --
    emitter.label("__rt_p2p_word");
    emitter.adrp("x15", "_pcre_word");                                          // load page of replacement string
    emitter.add_lo12("x15", "x15", "_pcre_word");                               // resolve address
    emitter.instruction("mov x16, #12");                                        // replacement length = 12
    emitter.instruction("b __rt_p2p_replace");                                  // go to copy routine

    // -- \S → [^[:space:]] (12 bytes) --
    emitter.label("__rt_p2p_nspace");
    emitter.adrp("x15", "_pcre_nspace");                                        // load page of replacement string
    emitter.add_lo12("x15", "x15", "_pcre_nspace");                             // resolve address
    emitter.instruction("mov x16, #12");                                        // replacement length = 12
    emitter.instruction("b __rt_p2p_replace");                                  // go to copy routine

    // -- \D → [^[:digit:]] (12 bytes) --
    emitter.label("__rt_p2p_ndigit");
    emitter.adrp("x15", "_pcre_ndigit");                                        // load page of replacement string
    emitter.add_lo12("x15", "x15", "_pcre_ndigit");                             // resolve address
    emitter.instruction("mov x16, #12");                                        // replacement length = 12
    emitter.instruction("b __rt_p2p_replace");                                  // go to copy routine

    // -- \W → [^[:alnum:]_] (13 bytes) --
    emitter.label("__rt_p2p_nword");
    emitter.adrp("x15", "_pcre_nword");                                         // load page of replacement string
    emitter.add_lo12("x15", "x15", "_pcre_nword");                              // resolve address
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

fn emit_pcre_to_posix_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: pcre_to_posix ---");
    emitter.label_global("__rt_pcre_to_posix");

    abi::emit_symbol_address(emitter, "r8", "_cstr_buf");
    emitter.instruction("mov r9, r8");                                          // preserve the start of the converted POSIX pattern buffer for the helper return value
    emitter.instruction("lea r10, [rax + rdx]");                                // precompute the end pointer of the source pattern so the scan loop can use pointer comparisons

    emitter.label("__rt_p2p_loop_linux_x86_64");
    emitter.instruction("cmp rax, r10");                                        // stop scanning once the source cursor reaches the end of the PCRE pattern payload
    emitter.instruction("jge __rt_p2p_done_linux_x86_64");                      // finish by null-terminating the converted POSIX pattern buffer
    emitter.instruction("movzx ecx, BYTE PTR [rax]");                           // load the current source byte from the PCRE pattern payload
    emitter.instruction("cmp ecx, 92");                                         // detect backslashes that may start a PCRE shorthand escape sequence
    emitter.instruction("jne __rt_p2p_copy_linux_x86_64");                      // copy ordinary pattern bytes through unchanged when no escape translation is needed
    emitter.instruction("lea r11, [rax + 1]");                                  // compute the address of the escaped character after the current backslash
    emitter.instruction("cmp r11, r10");                                        // ensure the escaped character is still inside the source pattern payload
    emitter.instruction("jge __rt_p2p_copy_linux_x86_64");                      // copy a trailing backslash literally when there is no following shorthand byte
    emitter.instruction("movzx edx, BYTE PTR [r11]");                           // load the escaped character following the current PCRE backslash
    emitter.instruction("cmp edx, 115");                                        // check for the lowercase space shorthand '\\s'
    emitter.instruction("je __rt_p2p_space_linux_x86_64");                      // replace '\\s' with the POSIX [[:space:]] character class
    emitter.instruction("cmp edx, 100");                                        // check for the lowercase digit shorthand '\\d'
    emitter.instruction("je __rt_p2p_digit_linux_x86_64");                      // replace '\\d' with the POSIX [[:digit:]] character class
    emitter.instruction("cmp edx, 119");                                        // check for the lowercase word shorthand '\\w'
    emitter.instruction("je __rt_p2p_word_linux_x86_64");                       // replace '\\w' with the POSIX [[:alnum:]_] character class
    emitter.instruction("cmp edx, 83");                                         // check for the uppercase negated space shorthand '\\S'
    emitter.instruction("je __rt_p2p_nspace_linux_x86_64");                     // replace '\\S' with the POSIX [^[:space:]] character class
    emitter.instruction("cmp edx, 68");                                         // check for the uppercase negated digit shorthand '\\D'
    emitter.instruction("je __rt_p2p_ndigit_linux_x86_64");                     // replace '\\D' with the POSIX [^[:digit:]] character class
    emitter.instruction("cmp edx, 87");                                         // check for the uppercase negated word shorthand '\\W'
    emitter.instruction("je __rt_p2p_nword_linux_x86_64");                      // replace '\\W' with the POSIX [^[:alnum:]_] character class

    emitter.label("__rt_p2p_copy_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r8], cl");                               // copy the current literal PCRE byte into the converted POSIX pattern buffer
    emitter.instruction("add r8, 1");                                           // advance the converted-pattern write cursor after storing one literal byte
    emitter.instruction("add rax, 1");                                          // advance the source pattern cursor to the next input byte
    emitter.instruction("jmp __rt_p2p_loop_linux_x86_64");                      // continue scanning the remaining PCRE pattern payload

    emitter.label("__rt_p2p_space_linux_x86_64");
    abi::emit_symbol_address(emitter, "rsi", "_pcre_space");
    emitter.instruction("mov ecx, 11");                                         // materialize the replacement length for the [[:space:]] POSIX character class
    emitter.instruction("jmp __rt_p2p_replace_linux_x86_64");                   // copy the translated POSIX replacement into the converted pattern buffer

    emitter.label("__rt_p2p_digit_linux_x86_64");
    abi::emit_symbol_address(emitter, "rsi", "_pcre_digit");
    emitter.instruction("mov ecx, 11");                                         // materialize the replacement length for the [[:digit:]] POSIX character class
    emitter.instruction("jmp __rt_p2p_replace_linux_x86_64");                   // copy the translated POSIX replacement into the converted pattern buffer

    emitter.label("__rt_p2p_word_linux_x86_64");
    abi::emit_symbol_address(emitter, "rsi", "_pcre_word");
    emitter.instruction("mov ecx, 12");                                         // materialize the replacement length for the [[:alnum:]_] POSIX character class
    emitter.instruction("jmp __rt_p2p_replace_linux_x86_64");                   // copy the translated POSIX replacement into the converted pattern buffer

    emitter.label("__rt_p2p_nspace_linux_x86_64");
    abi::emit_symbol_address(emitter, "rsi", "_pcre_nspace");
    emitter.instruction("mov ecx, 12");                                         // materialize the replacement length for the [^[:space:]] POSIX character class
    emitter.instruction("jmp __rt_p2p_replace_linux_x86_64");                   // copy the translated POSIX replacement into the converted pattern buffer

    emitter.label("__rt_p2p_ndigit_linux_x86_64");
    abi::emit_symbol_address(emitter, "rsi", "_pcre_ndigit");
    emitter.instruction("mov ecx, 12");                                         // materialize the replacement length for the [^[:digit:]] POSIX character class
    emitter.instruction("jmp __rt_p2p_replace_linux_x86_64");                   // copy the translated POSIX replacement into the converted pattern buffer

    emitter.label("__rt_p2p_nword_linux_x86_64");
    abi::emit_symbol_address(emitter, "rsi", "_pcre_nword");
    emitter.instruction("mov ecx, 13");                                         // materialize the replacement length for the [^[:alnum:]_] POSIX character class

    emitter.label("__rt_p2p_replace_linux_x86_64");
    emitter.instruction("xor edx, edx");                                        // start copying the POSIX replacement payload from offset zero

    emitter.label("__rt_p2p_replace_loop_linux_x86_64");
    emitter.instruction("cmp rdx, rcx");                                        // stop copying once the full translated POSIX class literal has been emitted
    emitter.instruction("jge __rt_p2p_replace_done_linux_x86_64");              // resume scanning the source PCRE pattern after copying the replacement bytes
    emitter.instruction("mov r11b, BYTE PTR [rsi + rdx]");                      // load one translated POSIX replacement byte from the static helper literal
    emitter.instruction("mov BYTE PTR [r8], r11b");                             // append the translated POSIX replacement byte into the destination scratch buffer
    emitter.instruction("add r8, 1");                                           // advance the converted-pattern write cursor after emitting one replacement byte
    emitter.instruction("add rdx, 1");                                          // advance the replacement literal index to the next byte
    emitter.instruction("jmp __rt_p2p_replace_loop_linux_x86_64");              // continue copying the translated POSIX replacement literal

    emitter.label("__rt_p2p_replace_done_linux_x86_64");
    emitter.instruction("add rax, 2");                                          // consume both the backslash and shorthand byte that were translated into the POSIX literal
    emitter.instruction("jmp __rt_p2p_loop_linux_x86_64");                      // continue scanning the remaining PCRE pattern bytes after the translated escape

    emitter.label("__rt_p2p_done_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r8], 0");                                // append the trailing C null terminator after the converted POSIX pattern bytes
    emitter.instruction("mov rax, r9");                                         // return the start of the converted POSIX pattern buffer in the x86_64 integer result register
    emitter.instruction("ret");                                                 // return the converted POSIX-compatible regex pattern to the caller
}
