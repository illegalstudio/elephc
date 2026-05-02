use crate::codegen::{emit::Emitter, platform::Arch};

/// fnmatch: shell-glob match between a pattern and a filename.
/// Input:  x1/x2 = pattern, x3/x4 = filename
/// Output: x0 = 1 on match, 0 otherwise
///
/// Supported wildcards (PHP defaults, flags == 0):
/// - `*` matches any sequence of characters (including the empty string)
/// - `?` matches exactly one character
/// - `[abc]`, `[a-z]`, `[!abc]`, `[^abc]` matches a character class (with `!`/`^` negation and `-` ranges)
/// - `\\` escapes the following character so it is treated as a literal
///
/// Flags (FNM_PATHNAME, FNM_PERIOD, FNM_CASEFOLD, FNM_NOESCAPE) are not yet
/// supported and are treated as 0 by the type-checker layer.
pub fn emit_fnmatch(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_fnmatch_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment after preceding runtime literals
    emitter.comment("--- runtime: fnmatch ---");
    emitter.label_global("__rt_fnmatch");

    // Register usage:
    //   x1=pattern_ptr, x2=pattern_len, x3=filename_ptr, x4=filename_len
    //   x5 = pattern index (i)
    //   x6 = filename index (j)
    //   x7 = star pattern index (-1 means "no star yet")
    //   x8 = filename index that was current when the star was seen

    emitter.instruction("mov x5, #0");                                          // initialise pattern index i = 0
    emitter.instruction("mov x6, #0");                                          // initialise filename index j = 0
    emitter.instruction("mov x7, #-1");                                         // initialise star_i = -1 (no star recorded yet)
    emitter.instruction("mov x8, #0");                                          // initialise star_j = 0 (placeholder)

    // -- main loop --
    emitter.label("__rt_fnmatch_loop");
    emitter.instruction("cmp x6, x4");                                          // have we consumed the entire filename?
    emitter.instruction("b.ge __rt_fnmatch_filename_done");                     // yes: see whether the remaining pattern is "*..."

    // -- pattern still has characters? --
    emitter.instruction("cmp x5, x2");                                          // is there any pattern byte left?
    emitter.instruction("b.ge __rt_fnmatch_try_backtrack");                     // pattern exhausted but filename has bytes → backtrack or fail
    emitter.instruction("ldrb w9, [x1, x5]");                                   // load the current pattern byte

    // -- '*': record backtracking point and consume the wildcard --
    emitter.instruction("cmp w9, #0x2A");                                       // is the current pattern byte '*'?
    emitter.instruction("b.ne __rt_fnmatch_check_qmark");                       // not '*': continue with the next dispatch
    emitter.instruction("mov x7, x5");                                          // remember the '*' position for backtracking
    emitter.instruction("mov x8, x6");                                          // remember the filename position when '*' began
    emitter.instruction("add x5, x5, #1");                                      // advance past the '*'
    emitter.instruction("b __rt_fnmatch_loop");                                 // restart matching with the wildcard active

    // -- '?': consume one filename byte --
    emitter.label("__rt_fnmatch_check_qmark");
    emitter.instruction("cmp w9, #0x3F");                                       // is the current pattern byte '?'?
    emitter.instruction("b.ne __rt_fnmatch_check_class");                       // not '?': continue with the next dispatch
    emitter.instruction("add x5, x5, #1");                                      // advance past the '?'
    emitter.instruction("add x6, x6, #1");                                      // consume one filename byte
    emitter.instruction("b __rt_fnmatch_loop");                                 // continue matching

    // -- '[': character class --
    emitter.label("__rt_fnmatch_check_class");
    emitter.instruction("cmp w9, #0x5B");                                       // is the current pattern byte '['?
    emitter.instruction("b.ne __rt_fnmatch_check_escape");                      // not '[': continue with the next dispatch
    emitter.instruction("ldrb w11, [x3, x6]");                                  // load the filename byte we are testing against the class
    // walk the class body, computing a match flag in x12 (1 = match)
    emitter.instruction("add x10, x5, #1");                                     // x10 = index into the class body
    emitter.instruction("mov x12, #0");                                         // matched-so-far flag
    emitter.instruction("mov x13, #0");                                         // negate flag
    // optional leading '!' or '^' negation
    emitter.instruction("cmp x10, x2");                                         // is the class empty (just "[")?
    emitter.instruction("b.ge __rt_fnmatch_fail");                              // unterminated class → no match
    emitter.instruction("ldrb w14, [x1, x10]");                                 // peek at first class byte
    emitter.instruction("cmp w14, #0x21");                                      // is it '!'?
    emitter.instruction("b.eq __rt_fnmatch_class_neg");                         // yes → set negate
    emitter.instruction("cmp w14, #0x5E");                                      // is it '^'?
    emitter.instruction("b.ne __rt_fnmatch_class_loop");                        // no → start scanning class members
    emitter.label("__rt_fnmatch_class_neg");
    emitter.instruction("mov x13, #1");                                         // record that the class is negated
    emitter.instruction("add x10, x10, #1");                                    // skip the negation byte

    emitter.label("__rt_fnmatch_class_loop");
    emitter.instruction("cmp x10, x2");                                         // ran past the end of the pattern without seeing ']'
    emitter.instruction("b.ge __rt_fnmatch_fail");                              // unterminated class → no match
    emitter.instruction("ldrb w14, [x1, x10]");                                 // load class byte
    emitter.instruction("cmp w14, #0x5D");                                      // is it ']'?
    emitter.instruction("b.eq __rt_fnmatch_class_done");                        // class body finished

    // peek at the next byte to detect a range a-b
    emitter.instruction("add x15, x10, #1");                                    // index of the byte right after the current class member
    emitter.instruction("cmp x15, x2");                                         // bounds check before peeking
    emitter.instruction("b.ge __rt_fnmatch_class_single");                      // no following byte → treat as single literal
    emitter.instruction("ldrb w16, [x1, x15]");                                 // peek at the next class byte
    emitter.instruction("cmp w16, #0x2D");                                      // is it '-'?
    emitter.instruction("b.ne __rt_fnmatch_class_single");                      // not a range → treat as single literal
    // make sure '-' is not the trailing class character
    emitter.instruction("add x17, x10, #2");                                    // index of the range upper bound
    emitter.instruction("cmp x17, x2");                                         // is the upper bound in range?
    emitter.instruction("b.ge __rt_fnmatch_class_single");                      // dangling '-' just before end-of-pattern → literal
    emitter.instruction("ldrb w0, [x1, x17]");                                  // load the upper-bound byte (range end)
    emitter.instruction("cmp w0, #0x5D");                                       // is the upper bound ']'? then '-' was a literal
    emitter.instruction("b.eq __rt_fnmatch_class_single");                      // 'X-]' → '-' is literal, treat 'X' as single
    // we have a real range: w14..w0
    emitter.instruction("cmp w11, w14");                                        // filename byte >= range low?
    emitter.instruction("b.lt __rt_fnmatch_class_advance3");                    // below low bound: skip range
    emitter.instruction("cmp w11, w0");                                         // filename byte <= range high?
    emitter.instruction("b.gt __rt_fnmatch_class_advance3");                    // above high bound: skip range
    emitter.instruction("mov x12, #1");                                         // inside range → mark as matched
    emitter.label("__rt_fnmatch_class_advance3");
    emitter.instruction("add x10, x10, #3");                                    // skip the three-byte range
    emitter.instruction("b __rt_fnmatch_class_loop");                           // continue scanning the class

    emitter.label("__rt_fnmatch_class_single");
    emitter.instruction("cmp w14, w11");                                        // does the filename byte equal this single class member?
    emitter.instruction("b.ne __rt_fnmatch_class_advance1");                    // mismatch: keep scanning
    emitter.instruction("mov x12, #1");                                         // matched: record success
    emitter.label("__rt_fnmatch_class_advance1");
    emitter.instruction("add x10, x10, #1");                                    // step past the single-byte class member
    emitter.instruction("b __rt_fnmatch_class_loop");                           // continue scanning the class

    emitter.label("__rt_fnmatch_class_done");
    // x12 = 1 if any literal/range matched; x13 = 1 if negated
    // class result = x12 XOR x13
    emitter.instruction("eor x12, x12, x13");                                   // apply optional negation
    emitter.instruction("cbz x12, __rt_fnmatch_try_backtrack");                 // class did not match → backtrack
    emitter.instruction("add x5, x10, #1");                                     // jump past the closing ']'
    emitter.instruction("add x6, x6, #1");                                      // consume the matched filename byte
    emitter.instruction("b __rt_fnmatch_loop");                                 // continue matching

    // -- '\\': escape the next pattern byte --
    emitter.label("__rt_fnmatch_check_escape");
    emitter.instruction("cmp w9, #0x5C");                                       // is the current pattern byte '\\'?
    emitter.instruction("b.ne __rt_fnmatch_literal");                           // not an escape: fall through to literal compare
    emitter.instruction("add x10, x5, #1");                                     // index of the escaped pattern byte
    emitter.instruction("cmp x10, x2");                                         // bounds check the escape sequence
    emitter.instruction("b.ge __rt_fnmatch_fail");                              // dangling '\\' at end of pattern → no match
    emitter.instruction("ldrb w9, [x1, x10]");                                  // load the escaped byte (overwrites w9 with the literal)
    emitter.instruction("ldrb w11, [x3, x6]");                                  // load the filename byte to compare against
    emitter.instruction("cmp w9, w11");                                         // does the escaped byte match?
    emitter.instruction("b.ne __rt_fnmatch_try_backtrack");                     // mismatch: backtrack on '*' or fail
    emitter.instruction("add x5, x5, #2");                                      // advance past '\\X'
    emitter.instruction("add x6, x6, #1");                                      // consume the matched filename byte
    emitter.instruction("b __rt_fnmatch_loop");                                 // continue matching

    // -- literal byte comparison --
    emitter.label("__rt_fnmatch_literal");
    emitter.instruction("ldrb w11, [x3, x6]");                                  // load filename byte
    emitter.instruction("cmp w9, w11");                                         // does it match the literal pattern byte?
    emitter.instruction("b.ne __rt_fnmatch_try_backtrack");                     // mismatch: backtrack on '*' or fail
    emitter.instruction("add x5, x5, #1");                                      // advance the pattern index
    emitter.instruction("add x6, x6, #1");                                      // advance the filename index
    emitter.instruction("b __rt_fnmatch_loop");                                 // continue matching

    // -- backtracking: if a '*' is active, advance the filename and retry --
    emitter.label("__rt_fnmatch_try_backtrack");
    emitter.instruction("cmn x7, #1");                                          // is star_i == -1 ?
    emitter.instruction("b.eq __rt_fnmatch_fail");                              // no recorded star: definitive failure
    emitter.instruction("add x5, x7, #1");                                      // resume pattern just after the recorded '*'
    emitter.instruction("add x8, x8, #1");                                      // extend the '*' span by one filename byte
    emitter.instruction("mov x6, x8");                                          // restart filename index from the extended position
    emitter.instruction("cmp x6, x4");                                          // does the extended span still fit inside the filename?
    emitter.instruction("b.gt __rt_fnmatch_fail");                              // overshot the filename: definitive failure
    emitter.instruction("b __rt_fnmatch_loop");                                 // retry matching after extending the wildcard

    // -- the filename is exhausted: pattern must reduce to "*..." --
    emitter.label("__rt_fnmatch_filename_done");
    emitter.label("__rt_fnmatch_drain_stars");
    emitter.instruction("cmp x5, x2");                                          // pattern fully consumed too?
    emitter.instruction("b.ge __rt_fnmatch_match");                             // success: both ran out together
    emitter.instruction("ldrb w9, [x1, x5]");                                   // peek at the remaining pattern byte
    emitter.instruction("cmp w9, #0x2A");                                       // is it '*'?
    emitter.instruction("b.ne __rt_fnmatch_fail");                              // anything else after filename ends → no match
    emitter.instruction("add x5, x5, #1");                                      // skip the trailing '*'
    emitter.instruction("b __rt_fnmatch_drain_stars");                          // consume any further trailing '*'

    emitter.label("__rt_fnmatch_match");
    emitter.instruction("mov x0, #1");                                          // report a successful match
    emitter.instruction("ret");                                                 // return to caller

    emitter.label("__rt_fnmatch_fail");
    emitter.instruction("mov x0, #0");                                          // report no match
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_fnmatch_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fnmatch ---");
    emitter.label_global("__rt_fnmatch");

    // ABI: rax=pattern_ptr, rdx=pattern_len, rdi=filename_ptr, rsi=filename_len
    // Returns: rax = 1 (match) or 0 (no match)
    //
    // Register usage:
    //   r8  = pattern index (i)
    //   r9  = filename index (j)
    //   r10 = star_i (-1 if no star)
    //   r11 = star_j

    emitter.instruction("push rbx");                                            // preserve the callee-saved scratch register used for class-range indexing
    emitter.instruction("xor r8d, r8d");                                        // i = 0
    emitter.instruction("xor r9d, r9d");                                        // j = 0
    emitter.instruction("mov r10, -1");                                         // star_i = -1
    emitter.instruction("xor r11d, r11d");                                      // star_j = 0

    emitter.label("__rt_fnmatch_loop_x86");
    emitter.instruction("cmp r9, rsi");                                         // filename consumed?
    emitter.instruction("jge __rt_fnmatch_filename_done_x86");                  // yes: see whether the remaining pattern is "*..."
    emitter.instruction("cmp r8, rdx");                                         // pattern still has bytes?
    emitter.instruction("jge __rt_fnmatch_try_backtrack_x86");                  // pattern exhausted but filename has bytes → backtrack
    emitter.instruction("movzx ecx, BYTE PTR [rax + r8]");                      // load the current pattern byte

    // '*': record backtracking point and consume the wildcard
    emitter.instruction("cmp cl, 0x2A");                                        // pattern byte == '*' ?
    emitter.instruction("jne __rt_fnmatch_check_qmark_x86");                    // not '*': continue dispatch
    emitter.instruction("mov r10, r8");                                         // record the '*' pattern index
    emitter.instruction("mov r11, r9");                                         // record the filename index when '*' began
    emitter.instruction("add r8, 1");                                           // advance past the '*'
    emitter.instruction("jmp __rt_fnmatch_loop_x86");                           // continue matching

    emitter.label("__rt_fnmatch_check_qmark_x86");
    emitter.instruction("cmp cl, 0x3F");                                        // pattern byte == '?' ?
    emitter.instruction("jne __rt_fnmatch_check_class_x86");                    // not '?': continue dispatch
    emitter.instruction("add r8, 1");                                           // advance past '?'
    emitter.instruction("add r9, 1");                                           // consume one filename byte
    emitter.instruction("jmp __rt_fnmatch_loop_x86");                           // continue matching

    emitter.label("__rt_fnmatch_check_class_x86");
    emitter.instruction("cmp cl, 0x5B");                                        // pattern byte == '[' ?
    emitter.instruction("jne __rt_fnmatch_check_escape_x86");                   // not '[': continue dispatch
    emitter.instruction("movzx r12d, BYTE PTR [rdi + r9]");                     // load filename byte under test
    emitter.instruction("mov r13, r8");                                         // r13 = pattern cursor for class body
    emitter.instruction("add r13, 1");                                          // step past the '[' itself
    emitter.instruction("xor r14d, r14d");                                      // matched-so-far = 0
    emitter.instruction("xor r15d, r15d");                                      // negate flag = 0
    emitter.instruction("cmp r13, rdx");                                        // empty class?
    emitter.instruction("jge __rt_fnmatch_fail_x86");                           // unterminated → no match
    emitter.instruction("movzx ecx, BYTE PTR [rax + r13]");                     // first class byte
    emitter.instruction("cmp cl, 0x21");                                        // is it '!'?
    emitter.instruction("je __rt_fnmatch_class_neg_x86");                       // yes → set negate
    emitter.instruction("cmp cl, 0x5E");                                        // is it '^'?
    emitter.instruction("jne __rt_fnmatch_class_loop_x86");                     // no → start scanning members
    emitter.label("__rt_fnmatch_class_neg_x86");
    emitter.instruction("mov r15d, 1");                                         // record negation
    emitter.instruction("add r13, 1");                                          // skip the negation byte

    emitter.label("__rt_fnmatch_class_loop_x86");
    emitter.instruction("cmp r13, rdx");                                        // ran past pattern end?
    emitter.instruction("jge __rt_fnmatch_fail_x86");                           // unterminated class → no match
    emitter.instruction("movzx ecx, BYTE PTR [rax + r13]");                     // load class byte
    emitter.instruction("cmp cl, 0x5D");                                        // ']' ?
    emitter.instruction("je __rt_fnmatch_class_done_x86");                      // class body finished

    // peek for range a-b
    emitter.instruction("mov rbx, r13");                                        // candidate index of '-' byte
    emitter.instruction("add rbx, 1");                                          // r13 + 1
    emitter.instruction("cmp rbx, rdx");                                        // bounds check
    emitter.instruction("jge __rt_fnmatch_class_single_x86");                   // no following byte: single literal
    emitter.instruction("movzx r12d, BYTE PTR [rax + rbx]");                    // tentative '-'
    emitter.instruction("cmp r12b, 0x2D");                                      // is it '-'?
    emitter.instruction("jne __rt_fnmatch_class_single_x86");                   // not range: single literal
    emitter.instruction("mov rbx, r13");                                        // recompute upper-bound index
    emitter.instruction("add rbx, 2");                                          // r13 + 2
    emitter.instruction("cmp rbx, rdx");                                        // bounds check
    emitter.instruction("jge __rt_fnmatch_class_single_x86");                   // dangling '-' at end → literal
    emitter.instruction("movzx r12d, BYTE PTR [rax + rbx]");                    // upper-bound byte
    emitter.instruction("cmp r12b, 0x5D");                                      // 'X-]' ? then '-' is literal
    emitter.instruction("je __rt_fnmatch_class_single_x86");                    // treat as single literal
    // real range: cl..r12b inclusive
    emitter.instruction("movzx r12d, BYTE PTR [rdi + r9]");                     // reload filename byte (was clobbered)
    emitter.instruction("cmp r12b, cl");                                        // filename byte >= low ?
    emitter.instruction("jb __rt_fnmatch_class_advance3_x86");                  // below low bound: skip range
    emitter.instruction("mov rbx, r13");                                        // recompute upper-bound index
    emitter.instruction("add rbx, 2");                                          // r13 + 2
    emitter.instruction("movzx ecx, BYTE PTR [rax + rbx]");                     // upper bound
    emitter.instruction("cmp r12b, cl");                                        // filename byte <= high ?
    emitter.instruction("ja __rt_fnmatch_class_advance3_x86");                  // above high bound: skip range
    emitter.instruction("mov r14d, 1");                                         // inside range → matched
    emitter.label("__rt_fnmatch_class_advance3_x86");
    emitter.instruction("add r13, 3");                                          // skip three-byte range
    emitter.instruction("jmp __rt_fnmatch_class_loop_x86");                     // continue scanning

    emitter.label("__rt_fnmatch_class_single_x86");
    emitter.instruction("movzx ecx, BYTE PTR [rax + r13]");                     // reload class member byte
    emitter.instruction("movzx r12d, BYTE PTR [rdi + r9]");                     // reload filename byte
    emitter.instruction("cmp cl, r12b");                                        // class byte == filename byte?
    emitter.instruction("jne __rt_fnmatch_class_advance1_x86");                 // mismatch: continue
    emitter.instruction("mov r14d, 1");                                         // matched
    emitter.label("__rt_fnmatch_class_advance1_x86");
    emitter.instruction("add r13, 1");                                          // step past single-byte member
    emitter.instruction("jmp __rt_fnmatch_class_loop_x86");                     // continue scanning

    emitter.label("__rt_fnmatch_class_done_x86");
    emitter.instruction("xor r14d, r15d");                                      // class result = matched XOR negated
    emitter.instruction("test r14d, r14d");                                     // any match?
    emitter.instruction("jz __rt_fnmatch_try_backtrack_x86");                   // no: backtrack
    emitter.instruction("mov r8, r13");                                         // resume pattern at the ']'
    emitter.instruction("add r8, 1");                                           // step past the ']'
    emitter.instruction("add r9, 1");                                           // consume the matched filename byte
    emitter.instruction("jmp __rt_fnmatch_loop_x86");                           // continue matching

    emitter.label("__rt_fnmatch_check_escape_x86");
    emitter.instruction("cmp cl, 0x5C");                                        // pattern byte == '\\' ?
    emitter.instruction("jne __rt_fnmatch_literal_x86");                        // not escape: fall through to literal compare
    emitter.instruction("mov r12, r8");                                         // index of escaped byte
    emitter.instruction("add r12, 1");                                          // i + 1
    emitter.instruction("cmp r12, rdx");                                        // bounds check
    emitter.instruction("jge __rt_fnmatch_fail_x86");                           // dangling '\\' → fail
    emitter.instruction("movzx ecx, BYTE PTR [rax + r12]");                     // load escaped byte
    emitter.instruction("movzx r12d, BYTE PTR [rdi + r9]");                     // load filename byte
    emitter.instruction("cmp cl, r12b");                                        // escaped byte matches filename byte?
    emitter.instruction("jne __rt_fnmatch_try_backtrack_x86");                  // mismatch: backtrack
    emitter.instruction("add r8, 2");                                           // advance past '\\X'
    emitter.instruction("add r9, 1");                                           // consume the matched filename byte
    emitter.instruction("jmp __rt_fnmatch_loop_x86");                           // continue matching

    emitter.label("__rt_fnmatch_literal_x86");
    emitter.instruction("movzx r12d, BYTE PTR [rdi + r9]");                     // load filename byte
    emitter.instruction("cmp cl, r12b");                                        // pattern byte == filename byte?
    emitter.instruction("jne __rt_fnmatch_try_backtrack_x86");                  // mismatch: backtrack
    emitter.instruction("add r8, 1");                                           // advance pattern
    emitter.instruction("add r9, 1");                                           // advance filename
    emitter.instruction("jmp __rt_fnmatch_loop_x86");                           // continue matching

    emitter.label("__rt_fnmatch_try_backtrack_x86");
    emitter.instruction("cmp r10, -1");                                         // is star_i == -1 ?
    emitter.instruction("je __rt_fnmatch_fail_x86");                            // no recorded star: definitive failure
    emitter.instruction("mov r8, r10");                                         // resume pattern just after recorded '*'
    emitter.instruction("add r8, 1");                                           // i = star_i + 1
    emitter.instruction("add r11, 1");                                          // extend wildcard span by one byte
    emitter.instruction("mov r9, r11");                                         // restart filename index from extended position
    emitter.instruction("cmp r9, rsi");                                         // span fits inside filename?
    emitter.instruction("jg __rt_fnmatch_fail_x86");                            // overshot: failure
    emitter.instruction("jmp __rt_fnmatch_loop_x86");                           // retry matching

    emitter.label("__rt_fnmatch_filename_done_x86");
    emitter.label("__rt_fnmatch_drain_stars_x86");
    emitter.instruction("cmp r8, rdx");                                         // pattern fully consumed?
    emitter.instruction("jge __rt_fnmatch_match_x86");                          // success
    emitter.instruction("movzx ecx, BYTE PTR [rax + r8]");                      // peek at remaining pattern byte
    emitter.instruction("cmp cl, 0x2A");                                        // is it '*'?
    emitter.instruction("jne __rt_fnmatch_fail_x86");                           // anything else → no match
    emitter.instruction("add r8, 1");                                           // skip trailing '*'
    emitter.instruction("jmp __rt_fnmatch_drain_stars_x86");                    // consume further trailing '*'

    emitter.label("__rt_fnmatch_match_x86");
    emitter.instruction("mov rax, 1");                                          // success
    emitter.instruction("pop rbx");                                             // restore the caller's callee-saved scratch register before returning
    emitter.instruction("ret");                                                 // return

    emitter.label("__rt_fnmatch_fail_x86");
    emitter.instruction("xor eax, eax");                                        // failure
    emitter.instruction("pop rbx");                                             // restore the caller's callee-saved scratch register before returning
    emitter.instruction("ret");                                                 // return
}
