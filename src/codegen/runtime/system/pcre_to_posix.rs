//! Purpose:
//! Emits the `__rt_pcre_to_posix`, `__rt_p2p_loop` runtime helper assembly for pcre to posix.
//! Keeps PHP builtin semantics, libc/syscall boundaries, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::system`.
//!
//! Key details:
//! - The translator is an emitted parser/formatter state machine that rewrites only the supported PCRE subset.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// __rt_pcre_to_posix: copy regex pattern to _cstr_buf, converting PCRE shorthands
/// and common Unicode property escapes to POSIX equivalents.
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
    emitter.instruction("cmp w14, #112");                                       // check for 'p' (0x70)
    emitter.instruction("b.eq __rt_p2p_prop");                                  // \p{...} → POSIX character class
    emitter.instruction("cmp w14, #80");                                        // check for 'P' (0x50)
    emitter.instruction("b.eq __rt_p2p_nprop");                                 // \P{...} → negated POSIX character class

    // -- not a PCRE shorthand, copy backslash as-is --
    emitter.label("__rt_p2p_copy");
    emitter.instruction("strb w12, [x9], #1");                                  // store byte to buffer, advance
    emitter.instruction("add x1, x1, #1");                                      // advance source ptr
    emitter.instruction("b __rt_p2p_loop");                                     // continue scanning

    // -- \s → [[:space:]] (11 bytes) --
    emitter.label("__rt_p2p_space");
    emitter.instruction("add x13, x1, #2");                                     // resume after the two-byte shorthand escape
    emitter.adrp("x15", "_pcre_space");                                         // load page of replacement string
    emitter.add_lo12("x15", "x15", "_pcre_space");                              // resolve address
    emitter.instruction("mov x16, #11");                                        // replacement length = 11
    emitter.instruction("b __rt_p2p_replace");                                  // go to copy routine

    // -- \d → [[:digit:]] (11 bytes) --
    emitter.label("__rt_p2p_digit");
    emitter.instruction("add x13, x1, #2");                                     // resume after the two-byte shorthand escape
    emitter.adrp("x15", "_pcre_digit");                                         // load page of replacement string
    emitter.add_lo12("x15", "x15", "_pcre_digit");                              // resolve address
    emitter.instruction("mov x16, #11");                                        // replacement length = 11
    emitter.instruction("b __rt_p2p_replace");                                  // go to copy routine

    // -- \w → [[:alnum:]_] (12 bytes) --
    emitter.label("__rt_p2p_word");
    emitter.instruction("add x13, x1, #2");                                     // resume after the two-byte shorthand escape
    emitter.adrp("x15", "_pcre_word");                                          // load page of replacement string
    emitter.add_lo12("x15", "x15", "_pcre_word");                               // resolve address
    emitter.instruction("mov x16, #12");                                        // replacement length = 12
    emitter.instruction("b __rt_p2p_replace");                                  // go to copy routine

    // -- \S → [^[:space:]] (12 bytes) --
    emitter.label("__rt_p2p_nspace");
    emitter.instruction("add x13, x1, #2");                                     // resume after the two-byte shorthand escape
    emitter.adrp("x15", "_pcre_nspace");                                        // load page of replacement string
    emitter.add_lo12("x15", "x15", "_pcre_nspace");                             // resolve address
    emitter.instruction("mov x16, #12");                                        // replacement length = 12
    emitter.instruction("b __rt_p2p_replace");                                  // go to copy routine

    // -- \D → [^[:digit:]] (12 bytes) --
    emitter.label("__rt_p2p_ndigit");
    emitter.instruction("add x13, x1, #2");                                     // resume after the two-byte shorthand escape
    emitter.adrp("x15", "_pcre_ndigit");                                        // load page of replacement string
    emitter.add_lo12("x15", "x15", "_pcre_ndigit");                             // resolve address
    emitter.instruction("mov x16, #12");                                        // replacement length = 12
    emitter.instruction("b __rt_p2p_replace");                                  // go to copy routine

    // -- \W → [^[:alnum:]_] (13 bytes) --
    emitter.label("__rt_p2p_nword");
    emitter.instruction("add x13, x1, #2");                                     // resume after the two-byte shorthand escape
    emitter.adrp("x15", "_pcre_nword");                                         // load page of replacement string
    emitter.add_lo12("x15", "x15", "_pcre_nword");                              // resolve address
    emitter.instruction("mov x16, #13");                                        // replacement length = 13
    emitter.instruction("b __rt_p2p_replace");                                  // copy the negated word class into the output

    // -- \p{...}/\P{...} Unicode property escapes --
    emitter.label("__rt_p2p_prop");
    emitter.instruction("mov x14, #0");                                         // mark the property escape as positive
    emitter.instruction("b __rt_p2p_prop_parse");                               // parse the braced property name
    emitter.label("__rt_p2p_nprop");
    emitter.instruction("mov x14, #1");                                         // mark the property escape as negated
    emitter.label("__rt_p2p_prop_parse");
    emitter.instruction("add x15, x1, #2");                                     // point at the expected opening property brace
    emitter.instruction("cmp x15, x11");                                        // ensure the opening brace position is in bounds
    emitter.instruction("b.ge __rt_p2p_prop_literal");                          // malformed escape: keep the backslash literal
    emitter.instruction("ldrb w12, [x15]");                                     // load the byte after the property marker
    emitter.instruction("cmp w12, #123");                                       // check for '{'
    emitter.instruction("b.ne __rt_p2p_prop_literal");                          // unsupported property spelling: keep the backslash literal
    emitter.instruction("add x16, x1, #3");                                     // property name starts after the opening brace
    emitter.instruction("mov x17, x16");                                        // start scanning for the closing property brace
    emitter.label("__rt_p2p_prop_scan");
    emitter.instruction("cmp x17, x11");                                        // stop if the property escape runs off the pattern end
    emitter.instruction("b.ge __rt_p2p_prop_literal");                          // malformed escape: keep the backslash literal
    emitter.instruction("ldrb w12, [x17]");                                     // load the current property-name byte
    emitter.instruction("cmp w12, #125");                                       // check for '}'
    emitter.instruction("b.eq __rt_p2p_prop_found");                            // found the closing property brace
    emitter.instruction("add x17, x17, #1");                                    // advance to the next property-name byte
    emitter.instruction("b __rt_p2p_prop_scan");                                // continue scanning the property escape
    emitter.label("__rt_p2p_prop_found");
    emitter.instruction("sub x0, x17, x16");                                    // compute property-name length
    emitter.instruction("cbz x0, __rt_p2p_prop_literal");                       // empty property names are unsupported literals
    emitter.instruction("add x13, x17, #1");                                    // resume after the closing property brace
    emitter.instruction("ldrb w12, [x16]");                                     // load the first property-name byte
    emitter.instruction("cmp w12, #76");                                        // check for 'L' properties
    emitter.instruction("b.eq __rt_p2p_prop_letter");                           // map letter properties to POSIX alpha/lower/upper
    emitter.instruction("cmp w12, #108");                                       // check for 'l' properties
    emitter.instruction("b.eq __rt_p2p_prop_letter");                           // map lowercase spelling to POSIX letter classes
    emitter.instruction("cmp w12, #78");                                        // check for 'N' properties
    emitter.instruction("b.eq __rt_p2p_prop_number");                           // map number properties to POSIX digit
    emitter.instruction("cmp w12, #110");                                       // check for 'n' properties
    emitter.instruction("b.eq __rt_p2p_prop_number");                           // map lowercase spelling to POSIX digit
    emitter.instruction("cmp w12, #90");                                        // check for 'Z' separator properties
    emitter.instruction("b.eq __rt_p2p_prop_space");                            // map separator properties to POSIX space
    emitter.instruction("cmp w12, #122");                                       // check for 'z' separator properties
    emitter.instruction("b.eq __rt_p2p_prop_space");                            // map lowercase spelling to POSIX space
    emitter.instruction("cmp w12, #80");                                        // check for 'P' punctuation properties
    emitter.instruction("b.eq __rt_p2p_prop_punct");                            // map punctuation properties to POSIX punct
    emitter.instruction("cmp w12, #112");                                       // check for 'p' punctuation properties
    emitter.instruction("b.eq __rt_p2p_prop_punct");                            // map lowercase spelling to POSIX punct
    emitter.instruction("b __rt_p2p_prop_literal");                             // unsupported property names remain literal

    emitter.label("__rt_p2p_prop_letter");
    emitter.instruction("cmp x0, #2");                                          // only two-byte L* aliases can refine alpha
    emitter.instruction("b.ne __rt_p2p_prop_alpha");                            // longer names use the broad alpha class
    emitter.instruction("ldrb w12, [x16, #1]");                                 // load the second property alias byte
    emitter.instruction("cmp w12, #117");                                       // check for 'u' in Lu
    emitter.instruction("b.eq __rt_p2p_prop_upper");                            // Lu maps to upper
    emitter.instruction("cmp w12, #85");                                        // check for 'U' in LU
    emitter.instruction("b.eq __rt_p2p_prop_upper");                            // uppercase spelling maps to upper
    emitter.instruction("cmp w12, #108");                                       // check for 'l' in Ll
    emitter.instruction("b.eq __rt_p2p_prop_lower");                            // Ll maps to lower
    emitter.instruction("cmp w12, #76");                                        // check for 'L' in LL
    emitter.instruction("b.eq __rt_p2p_prop_lower");                            // uppercase spelling maps to lower
    emitter.instruction("b __rt_p2p_prop_alpha");                               // other L* aliases use broad alpha
    emitter.label("__rt_p2p_prop_alpha");
    emitter.instruction("cbnz x14, __rt_p2p_prop_nalpha");                      // choose negated alpha for \P{L}
    emitter.adrp("x15", "_pcre_alpha");                                         // load page of alpha replacement string
    emitter.add_lo12("x15", "x15", "_pcre_alpha");                              // resolve alpha replacement address
    emitter.instruction("mov x16, #30");                                        // replacement length = 30
    emitter.instruction("b __rt_p2p_replace");                                  // copy the broad letter shim into the output
    emitter.label("__rt_p2p_prop_nalpha");
    emitter.adrp("x15", "_pcre_nalpha");                                        // load page of negated alpha replacement string
    emitter.add_lo12("x15", "x15", "_pcre_nalpha");                             // resolve negated alpha replacement address
    emitter.instruction("mov x16, #29");                                        // replacement length = 29
    emitter.instruction("b __rt_p2p_replace");                                  // copy the negated letter shim into the output
    emitter.label("__rt_p2p_prop_lower");
    emitter.instruction("cbnz x14, __rt_p2p_prop_nlower");                      // choose negated lower for \P{Ll}
    emitter.adrp("x15", "_pcre_lower");                                         // load page of lower replacement string
    emitter.add_lo12("x15", "x15", "_pcre_lower");                              // resolve lower replacement address
    emitter.instruction("mov x16, #11");                                        // replacement length = 11
    emitter.instruction("b __rt_p2p_replace");                                  // copy the lower class into the output
    emitter.label("__rt_p2p_prop_nlower");
    emitter.adrp("x15", "_pcre_nlower");                                        // load page of negated lower replacement string
    emitter.add_lo12("x15", "x15", "_pcre_nlower");                             // resolve negated lower replacement address
    emitter.instruction("mov x16, #12");                                        // replacement length = 12
    emitter.instruction("b __rt_p2p_replace");                                  // copy the negated lower class into the output
    emitter.label("__rt_p2p_prop_upper");
    emitter.instruction("cbnz x14, __rt_p2p_prop_nupper");                      // choose negated upper for \P{Lu}
    emitter.adrp("x15", "_pcre_upper");                                         // load page of upper replacement string
    emitter.add_lo12("x15", "x15", "_pcre_upper");                              // resolve upper replacement address
    emitter.instruction("mov x16, #11");                                        // replacement length = 11
    emitter.instruction("b __rt_p2p_replace");                                  // copy the upper class into the output
    emitter.label("__rt_p2p_prop_nupper");
    emitter.adrp("x15", "_pcre_nupper");                                        // load page of negated upper replacement string
    emitter.add_lo12("x15", "x15", "_pcre_nupper");                             // resolve negated upper replacement address
    emitter.instruction("mov x16, #12");                                        // replacement length = 12
    emitter.instruction("b __rt_p2p_replace");                                  // copy the negated upper class into the output
    emitter.label("__rt_p2p_prop_number");
    emitter.instruction("cbnz x14, __rt_p2p_prop_ndigit");                      // choose negated digit for \P{N}
    emitter.adrp("x15", "_pcre_digit");                                         // load page of digit replacement string
    emitter.add_lo12("x15", "x15", "_pcre_digit");                              // resolve digit replacement address
    emitter.instruction("mov x16, #11");                                        // replacement length = 11
    emitter.instruction("b __rt_p2p_replace");                                  // copy the digit class into the output
    emitter.label("__rt_p2p_prop_ndigit");
    emitter.adrp("x15", "_pcre_ndigit");                                        // load page of negated digit replacement string
    emitter.add_lo12("x15", "x15", "_pcre_ndigit");                             // resolve negated digit replacement address
    emitter.instruction("mov x16, #12");                                        // replacement length = 12
    emitter.instruction("b __rt_p2p_replace");                                  // copy the negated digit class into the output
    emitter.label("__rt_p2p_prop_space");
    emitter.instruction("cbnz x14, __rt_p2p_prop_nspace");                      // choose negated space for \P{Z}
    emitter.adrp("x15", "_pcre_space");                                         // load page of space replacement string
    emitter.add_lo12("x15", "x15", "_pcre_space");                              // resolve space replacement address
    emitter.instruction("mov x16, #11");                                        // replacement length = 11
    emitter.instruction("b __rt_p2p_replace");                                  // copy the space class into the output
    emitter.label("__rt_p2p_prop_nspace");
    emitter.adrp("x15", "_pcre_nspace");                                        // load page of negated space replacement string
    emitter.add_lo12("x15", "x15", "_pcre_nspace");                             // resolve negated space replacement address
    emitter.instruction("mov x16, #12");                                        // replacement length = 12
    emitter.instruction("b __rt_p2p_replace");                                  // copy the negated space class into the output
    emitter.label("__rt_p2p_prop_punct");
    emitter.instruction("cbnz x14, __rt_p2p_prop_npunct");                      // choose negated punctuation for \P{P}
    emitter.adrp("x15", "_pcre_punct");                                         // load page of punctuation replacement string
    emitter.add_lo12("x15", "x15", "_pcre_punct");                              // resolve punctuation replacement address
    emitter.instruction("mov x16, #11");                                        // replacement length = 11
    emitter.instruction("b __rt_p2p_replace");                                  // copy the punctuation class into the output
    emitter.label("__rt_p2p_prop_npunct");
    emitter.adrp("x15", "_pcre_npunct");                                        // load page of negated punctuation replacement string
    emitter.add_lo12("x15", "x15", "_pcre_npunct");                             // resolve negated punctuation replacement address
    emitter.instruction("mov x16, #12");                                        // replacement length = 12
    emitter.instruction("b __rt_p2p_replace");                                  // copy the negated punctuation class into the output
    emitter.label("__rt_p2p_prop_literal");
    emitter.instruction("ldrb w12, [x1]");                                      // reload the original backslash byte for literal copying
    emitter.instruction("b __rt_p2p_copy");                                     // leave unsupported property escapes unchanged

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
    emitter.instruction("mov x1, x13");                                         // resume source scanning after the translated escape
    emitter.instruction("b __rt_p2p_loop");                                     // continue scanning

    // -- null-terminate and return --
    emitter.label("__rt_p2p_done");
    emitter.instruction("strb wzr, [x9]");                                      // write null terminator
    emitter.instruction("mov x0, x10");                                         // return pointer to converted string
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux variant of the `__rt_pcre_to_posix` state machine.
/// Mirrors the ARM64 behavior but uses x86_64 SysV ABI registers:
/// Input:  rax=pattern ptr, rdx=pattern len
/// Output: rax=pointer to null-terminated string in _cstr_buf
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
    emitter.instruction("cmp edx, 112");                                        // check for the lowercase Unicode property escape marker '\\p'
    emitter.instruction("je __rt_p2p_prop_linux_x86_64");                       // translate supported '\\p{...}' properties into POSIX classes
    emitter.instruction("cmp edx, 80");                                         // check for the uppercase negated Unicode property escape marker '\\P'
    emitter.instruction("je __rt_p2p_nprop_linux_x86_64");                      // translate supported '\\P{...}' properties into negated POSIX classes

    emitter.label("__rt_p2p_copy_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r8], cl");                               // copy the current literal PCRE byte into the converted POSIX pattern buffer
    emitter.instruction("add r8, 1");                                           // advance the converted-pattern write cursor after storing one literal byte
    emitter.instruction("add rax, 1");                                          // advance the source pattern cursor to the next input byte
    emitter.instruction("jmp __rt_p2p_loop_linux_x86_64");                      // continue scanning the remaining PCRE pattern payload

    emitter.label("__rt_p2p_space_linux_x86_64");
    emitter.instruction("lea rdi, [rax + 2]");                                  // resume after the two-byte shorthand escape
    abi::emit_symbol_address(emitter, "rsi", "_pcre_space");
    emitter.instruction("mov ecx, 11");                                         // materialize the replacement length for the [[:space:]] POSIX character class
    emitter.instruction("jmp __rt_p2p_replace_linux_x86_64");                   // copy the translated POSIX replacement into the converted pattern buffer

    emitter.label("__rt_p2p_digit_linux_x86_64");
    emitter.instruction("lea rdi, [rax + 2]");                                  // resume after the two-byte shorthand escape
    abi::emit_symbol_address(emitter, "rsi", "_pcre_digit");
    emitter.instruction("mov ecx, 11");                                         // materialize the replacement length for the [[:digit:]] POSIX character class
    emitter.instruction("jmp __rt_p2p_replace_linux_x86_64");                   // copy the translated POSIX replacement into the converted pattern buffer

    emitter.label("__rt_p2p_word_linux_x86_64");
    emitter.instruction("lea rdi, [rax + 2]");                                  // resume after the two-byte shorthand escape
    abi::emit_symbol_address(emitter, "rsi", "_pcre_word");
    emitter.instruction("mov ecx, 12");                                         // materialize the replacement length for the [[:alnum:]_] POSIX character class
    emitter.instruction("jmp __rt_p2p_replace_linux_x86_64");                   // copy the translated POSIX replacement into the converted pattern buffer

    emitter.label("__rt_p2p_nspace_linux_x86_64");
    emitter.instruction("lea rdi, [rax + 2]");                                  // resume after the two-byte shorthand escape
    abi::emit_symbol_address(emitter, "rsi", "_pcre_nspace");
    emitter.instruction("mov ecx, 12");                                         // materialize the replacement length for the [^[:space:]] POSIX character class
    emitter.instruction("jmp __rt_p2p_replace_linux_x86_64");                   // copy the translated POSIX replacement into the converted pattern buffer

    emitter.label("__rt_p2p_ndigit_linux_x86_64");
    emitter.instruction("lea rdi, [rax + 2]");                                  // resume after the two-byte shorthand escape
    abi::emit_symbol_address(emitter, "rsi", "_pcre_ndigit");
    emitter.instruction("mov ecx, 12");                                         // materialize the replacement length for the [^[:digit:]] POSIX character class
    emitter.instruction("jmp __rt_p2p_replace_linux_x86_64");                   // copy the translated POSIX replacement into the converted pattern buffer

    emitter.label("__rt_p2p_nword_linux_x86_64");
    emitter.instruction("lea rdi, [rax + 2]");                                  // resume after the two-byte shorthand escape
    abi::emit_symbol_address(emitter, "rsi", "_pcre_nword");
    emitter.instruction("mov ecx, 13");                                         // materialize the replacement length for the [^[:alnum:]_] POSIX character class
    emitter.instruction("jmp __rt_p2p_replace_linux_x86_64");                   // copy the translated POSIX replacement into the converted pattern buffer

    emitter.label("__rt_p2p_prop_linux_x86_64");
    emitter.instruction("xor r11d, r11d");                                      // mark the Unicode property escape as positive
    emitter.instruction("jmp __rt_p2p_prop_parse_linux_x86_64");                // parse the braced property name
    emitter.label("__rt_p2p_nprop_linux_x86_64");
    emitter.instruction("mov r11d, 1");                                         // mark the Unicode property escape as negated
    emitter.label("__rt_p2p_prop_parse_linux_x86_64");
    emitter.instruction("lea rdi, [rax + 2]");                                  // point at the expected opening property brace
    emitter.instruction("cmp rdi, r10");                                        // ensure the opening brace position is in bounds
    emitter.instruction("jge __rt_p2p_prop_literal_linux_x86_64");              // malformed escape: keep the backslash literal
    emitter.instruction("movzx ecx, BYTE PTR [rdi]");                           // load the byte after the property marker
    emitter.instruction("cmp ecx, 123");                                        // check for '{'
    emitter.instruction("jne __rt_p2p_prop_literal_linux_x86_64");              // unsupported property spelling: keep the backslash literal
    emitter.instruction("lea rsi, [rax + 3]");                                  // property name starts after the opening brace
    emitter.instruction("mov rdx, rsi");                                        // start scanning for the closing property brace
    emitter.label("__rt_p2p_prop_scan_linux_x86_64");
    emitter.instruction("cmp rdx, r10");                                        // stop if the property escape runs off the pattern end
    emitter.instruction("jge __rt_p2p_prop_literal_linux_x86_64");              // malformed escape: keep the backslash literal
    emitter.instruction("movzx ecx, BYTE PTR [rdx]");                           // load the current property-name byte
    emitter.instruction("cmp ecx, 125");                                        // check for '}'
    emitter.instruction("je __rt_p2p_prop_found_linux_x86_64");                 // found the closing property brace
    emitter.instruction("add rdx, 1");                                          // advance to the next property-name byte
    emitter.instruction("jmp __rt_p2p_prop_scan_linux_x86_64");                 // continue scanning the property escape
    emitter.label("__rt_p2p_prop_found_linux_x86_64");
    emitter.instruction("mov rcx, rdx");                                        // copy closing-brace pointer to compute property length
    emitter.instruction("sub rcx, rsi");                                        // compute property-name length
    emitter.instruction("jz __rt_p2p_prop_literal_linux_x86_64");               // empty property names are unsupported literals
    emitter.instruction("lea rdi, [rdx + 1]");                                  // resume after the closing property brace
    emitter.instruction("movzx edx, BYTE PTR [rsi]");                           // load the first property-name byte
    emitter.instruction("cmp edx, 76");                                         // check for 'L' properties
    emitter.instruction("je __rt_p2p_prop_letter_linux_x86_64");                // map letter properties to POSIX alpha/lower/upper
    emitter.instruction("cmp edx, 108");                                        // check for 'l' properties
    emitter.instruction("je __rt_p2p_prop_letter_linux_x86_64");                // map lowercase spelling to POSIX letter classes
    emitter.instruction("cmp edx, 78");                                         // check for 'N' properties
    emitter.instruction("je __rt_p2p_prop_number_linux_x86_64");                // map number properties to POSIX digit
    emitter.instruction("cmp edx, 110");                                        // check for 'n' properties
    emitter.instruction("je __rt_p2p_prop_number_linux_x86_64");                // map lowercase spelling to POSIX digit
    emitter.instruction("cmp edx, 90");                                         // check for 'Z' separator properties
    emitter.instruction("je __rt_p2p_prop_space_linux_x86_64");                 // map separator properties to POSIX space
    emitter.instruction("cmp edx, 122");                                        // check for 'z' separator properties
    emitter.instruction("je __rt_p2p_prop_space_linux_x86_64");                 // map lowercase spelling to POSIX space
    emitter.instruction("cmp edx, 80");                                         // check for 'P' punctuation properties
    emitter.instruction("je __rt_p2p_prop_punct_linux_x86_64");                 // map punctuation properties to POSIX punct
    emitter.instruction("cmp edx, 112");                                        // check for 'p' punctuation properties
    emitter.instruction("je __rt_p2p_prop_punct_linux_x86_64");                 // map lowercase spelling to POSIX punct
    emitter.instruction("jmp __rt_p2p_prop_literal_linux_x86_64");              // unsupported property names remain literal

    emitter.label("__rt_p2p_prop_letter_linux_x86_64");
    emitter.instruction("cmp rcx, 2");                                          // only two-byte L* aliases can refine alpha
    emitter.instruction("jne __rt_p2p_prop_alpha_linux_x86_64");                // longer names use the broad alpha class
    emitter.instruction("movzx edx, BYTE PTR [rsi + 1]");                       // load the second property alias byte
    emitter.instruction("cmp edx, 117");                                        // check for 'u' in Lu
    emitter.instruction("je __rt_p2p_prop_upper_linux_x86_64");                 // Lu maps to upper
    emitter.instruction("cmp edx, 85");                                         // check for 'U' in LU
    emitter.instruction("je __rt_p2p_prop_upper_linux_x86_64");                 // uppercase spelling maps to upper
    emitter.instruction("cmp edx, 108");                                        // check for 'l' in Ll
    emitter.instruction("je __rt_p2p_prop_lower_linux_x86_64");                 // Ll maps to lower
    emitter.instruction("cmp edx, 76");                                         // check for 'L' in LL
    emitter.instruction("je __rt_p2p_prop_lower_linux_x86_64");                 // uppercase spelling maps to lower
    emitter.instruction("jmp __rt_p2p_prop_alpha_linux_x86_64");                // other L* aliases use broad alpha
    emitter.label("__rt_p2p_prop_alpha_linux_x86_64");
    emitter.instruction("test r11d, r11d");                                     // choose positive or negated alpha class
    emitter.instruction("jnz __rt_p2p_prop_nalpha_linux_x86_64");               // negated \P{L} uses the broad non-letter shim
    abi::emit_symbol_address(emitter, "rsi", "_pcre_alpha");
    emitter.instruction("mov ecx, 30");                                         // materialize the replacement length for the broad letter shim
    emitter.instruction("jmp __rt_p2p_replace_linux_x86_64");                   // copy the translated POSIX replacement into the converted pattern buffer
    emitter.label("__rt_p2p_prop_nalpha_linux_x86_64");
    abi::emit_symbol_address(emitter, "rsi", "_pcre_nalpha");
    emitter.instruction("mov ecx, 29");                                         // materialize the replacement length for the negated letter shim
    emitter.instruction("jmp __rt_p2p_replace_linux_x86_64");                   // copy the translated POSIX replacement into the converted pattern buffer
    emitter.label("__rt_p2p_prop_lower_linux_x86_64");
    emitter.instruction("test r11d, r11d");                                     // choose positive or negated lower class
    emitter.instruction("jnz __rt_p2p_prop_nlower_linux_x86_64");               // negated \P{Ll} uses [^[:lower:]]
    abi::emit_symbol_address(emitter, "rsi", "_pcre_lower");
    emitter.instruction("mov ecx, 11");                                         // materialize the replacement length for the [[:lower:]] POSIX character class
    emitter.instruction("jmp __rt_p2p_replace_linux_x86_64");                   // copy the translated POSIX replacement into the converted pattern buffer
    emitter.label("__rt_p2p_prop_nlower_linux_x86_64");
    abi::emit_symbol_address(emitter, "rsi", "_pcre_nlower");
    emitter.instruction("mov ecx, 12");                                         // materialize the replacement length for the [^[:lower:]] POSIX character class
    emitter.instruction("jmp __rt_p2p_replace_linux_x86_64");                   // copy the translated POSIX replacement into the converted pattern buffer
    emitter.label("__rt_p2p_prop_upper_linux_x86_64");
    emitter.instruction("test r11d, r11d");                                     // choose positive or negated upper class
    emitter.instruction("jnz __rt_p2p_prop_nupper_linux_x86_64");               // negated \P{Lu} uses [^[:upper:]]
    abi::emit_symbol_address(emitter, "rsi", "_pcre_upper");
    emitter.instruction("mov ecx, 11");                                         // materialize the replacement length for the [[:upper:]] POSIX character class
    emitter.instruction("jmp __rt_p2p_replace_linux_x86_64");                   // copy the translated POSIX replacement into the converted pattern buffer
    emitter.label("__rt_p2p_prop_nupper_linux_x86_64");
    abi::emit_symbol_address(emitter, "rsi", "_pcre_nupper");
    emitter.instruction("mov ecx, 12");                                         // materialize the replacement length for the [^[:upper:]] POSIX character class
    emitter.instruction("jmp __rt_p2p_replace_linux_x86_64");                   // copy the translated POSIX replacement into the converted pattern buffer

    emitter.label("__rt_p2p_prop_number_linux_x86_64");
    emitter.instruction("test r11d, r11d");                                     // choose positive or negated digit class
    emitter.instruction("jnz __rt_p2p_prop_ndigit_linux_x86_64");               // negated \P{N} uses [^[:digit:]]
    abi::emit_symbol_address(emitter, "rsi", "_pcre_digit");
    emitter.instruction("mov ecx, 11");                                         // materialize the replacement length for the [[:digit:]] POSIX character class
    emitter.instruction("jmp __rt_p2p_replace_linux_x86_64");                   // copy the translated POSIX replacement into the converted pattern buffer
    emitter.label("__rt_p2p_prop_ndigit_linux_x86_64");
    abi::emit_symbol_address(emitter, "rsi", "_pcre_ndigit");
    emitter.instruction("mov ecx, 12");                                         // materialize the replacement length for the [^[:digit:]] POSIX character class
    emitter.instruction("jmp __rt_p2p_replace_linux_x86_64");                   // copy the translated POSIX replacement into the converted pattern buffer

    emitter.label("__rt_p2p_prop_space_linux_x86_64");
    emitter.instruction("test r11d, r11d");                                     // choose positive or negated space class
    emitter.instruction("jnz __rt_p2p_prop_nspace_linux_x86_64");               // negated \P{Z} uses [^[:space:]]
    abi::emit_symbol_address(emitter, "rsi", "_pcre_space");
    emitter.instruction("mov ecx, 11");                                         // materialize the replacement length for the [[:space:]] POSIX character class
    emitter.instruction("jmp __rt_p2p_replace_linux_x86_64");                   // copy the translated POSIX replacement into the converted pattern buffer
    emitter.label("__rt_p2p_prop_nspace_linux_x86_64");
    abi::emit_symbol_address(emitter, "rsi", "_pcre_nspace");
    emitter.instruction("mov ecx, 12");                                         // materialize the replacement length for the [^[:space:]] POSIX character class
    emitter.instruction("jmp __rt_p2p_replace_linux_x86_64");                   // copy the translated POSIX replacement into the converted pattern buffer

    emitter.label("__rt_p2p_prop_punct_linux_x86_64");
    emitter.instruction("test r11d, r11d");                                     // choose positive or negated punctuation class
    emitter.instruction("jnz __rt_p2p_prop_npunct_linux_x86_64");               // negated \P{P} uses [^[:punct:]]
    abi::emit_symbol_address(emitter, "rsi", "_pcre_punct");
    emitter.instruction("mov ecx, 11");                                         // materialize the replacement length for the [[:punct:]] POSIX character class
    emitter.instruction("jmp __rt_p2p_replace_linux_x86_64");                   // copy the translated POSIX replacement into the converted pattern buffer
    emitter.label("__rt_p2p_prop_npunct_linux_x86_64");
    abi::emit_symbol_address(emitter, "rsi", "_pcre_npunct");
    emitter.instruction("mov ecx, 12");                                         // materialize the replacement length for the [^[:punct:]] POSIX character class
    emitter.instruction("jmp __rt_p2p_replace_linux_x86_64");                   // copy the translated POSIX replacement into the converted pattern buffer

    emitter.label("__rt_p2p_prop_literal_linux_x86_64");
    emitter.instruction("movzx ecx, BYTE PTR [rax]");                           // reload the original backslash byte for literal copying
    emitter.instruction("jmp __rt_p2p_copy_linux_x86_64");                      // leave unsupported property escapes unchanged

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
    emitter.instruction("mov rax, rdi");                                        // resume source scanning after the translated escape
    emitter.instruction("jmp __rt_p2p_loop_linux_x86_64");                      // continue scanning the remaining PCRE pattern bytes after the translated escape

    emitter.label("__rt_p2p_done_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r8], 0");                                // append the trailing C null terminator after the converted POSIX pattern bytes
    emitter.instruction("mov rax, r9");                                         // return the start of the converted POSIX pattern buffer in the x86_64 integer result register
    emitter.instruction("ret");                                                 // return the converted POSIX-compatible regex pattern to the caller
}
