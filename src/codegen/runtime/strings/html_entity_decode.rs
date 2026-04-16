use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// html_entity_decode: decode &amp;, &lt;, &gt;, &quot;, &#039; back to chars.
/// Input: x1/x2=string. Output: x1/x2=result in concat_buf.
pub fn emit_html_entity_decode(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_html_entity_decode_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: html_entity_decode ---");
    emitter.label_global("__rt_html_entity_decode");

    // -- set up concat_buf destination --
    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("mov x11, x2");                                         // remaining byte count

    emitter.label("__rt_hed_loop");
    emitter.instruction("cbz x11, __rt_hed_done");                              // no bytes left → done
    emitter.instruction("ldrb w12, [x1]");                                      // peek at current byte
    emitter.instruction("cmp w12, #38");                                        // is it '&'?
    emitter.instruction("b.ne __rt_hed_copy");                                  // no → copy as-is

    // -- try &lt; (4 chars: &lt;) --
    emitter.instruction("cmp x11, #4");                                         // need at least 4
    emitter.instruction("b.lt __rt_hed_copy");                                  // not enough
    emitter.instruction("ldrb w13, [x1, #1]");                                  // 2nd char
    emitter.instruction("cmp w13, #108");                                       // 'l'?
    emitter.instruction("b.ne __rt_hed_not_lt");                                // no
    emitter.instruction("ldrb w13, [x1, #2]");                                  // 3rd char
    emitter.instruction("cmp w13, #116");                                       // 't'?
    emitter.instruction("b.ne __rt_hed_copy");                                  // no
    emitter.instruction("ldrb w13, [x1, #3]");                                  // 4th char
    emitter.instruction("cmp w13, #59");                                        // ';'?
    emitter.instruction("b.ne __rt_hed_copy");                                  // no
    emitter.instruction("mov w12, #60");                                        // matched &lt; → '<'
    emitter.instruction("strb w12, [x9], #1");                                  // write '<'
    emitter.instruction("add x1, x1, #4");                                      // skip 4 source bytes
    emitter.instruction("sub x11, x11, #4");                                    // decrement remaining
    emitter.instruction("b __rt_hed_loop");                                     // next

    // -- try &gt; (4 chars: &gt;) --
    emitter.label("__rt_hed_not_lt");
    emitter.instruction("cmp w13, #103");                                       // 'g'?
    emitter.instruction("b.ne __rt_hed_not_gt");                                // no
    emitter.instruction("ldrb w13, [x1, #2]");                                  // 3rd char
    emitter.instruction("cmp w13, #116");                                       // 't'?
    emitter.instruction("b.ne __rt_hed_copy");                                  // no
    emitter.instruction("ldrb w13, [x1, #3]");                                  // 4th char
    emitter.instruction("cmp w13, #59");                                        // ';'?
    emitter.instruction("b.ne __rt_hed_copy");                                  // no
    emitter.instruction("mov w12, #62");                                        // matched &gt; → '>'
    emitter.instruction("strb w12, [x9], #1");                                  // write '>'
    emitter.instruction("add x1, x1, #4");                                      // skip 4
    emitter.instruction("sub x11, x11, #4");                                    // decrement
    emitter.instruction("b __rt_hed_loop");                                     // next

    // -- try &amp; (5 chars) --
    emitter.label("__rt_hed_not_gt");
    emitter.instruction("cmp x11, #5");                                         // need at least 5
    emitter.instruction("b.lt __rt_hed_try_long");                              // not enough for &amp;
    emitter.instruction("cmp w13, #97");                                        // 'a'?
    emitter.instruction("b.ne __rt_hed_try_long");                              // no
    emitter.instruction("ldrb w13, [x1, #2]");                                  // 3rd char
    emitter.instruction("cmp w13, #109");                                       // 'm'?
    emitter.instruction("b.ne __rt_hed_try_long");                              // no
    emitter.instruction("ldrb w13, [x1, #3]");                                  // 4th char
    emitter.instruction("cmp w13, #112");                                       // 'p'?
    emitter.instruction("b.ne __rt_hed_try_long");                              // no
    emitter.instruction("ldrb w13, [x1, #4]");                                  // 5th char
    emitter.instruction("cmp w13, #59");                                        // ';'?
    emitter.instruction("b.ne __rt_hed_try_long");                              // no
    emitter.instruction("mov w12, #38");                                        // matched &amp; → '&'
    emitter.instruction("strb w12, [x9], #1");                                  // write '&'
    emitter.instruction("add x1, x1, #5");                                      // skip 5
    emitter.instruction("sub x11, x11, #5");                                    // decrement
    emitter.instruction("b __rt_hed_loop");                                     // next

    // -- try &quot; or &#039; (6 chars) --
    emitter.label("__rt_hed_try_long");
    emitter.instruction("cmp x11, #6");                                         // need at least 6
    emitter.instruction("b.lt __rt_hed_copy");                                  // not enough
    emitter.instruction("ldrb w13, [x1, #1]");                                  // reload 2nd char
    emitter.instruction("cmp w13, #113");                                       // 'q'? (&quot;)
    emitter.instruction("b.ne __rt_hed_try_apos");                              // no

    // -- try &quot; --
    emitter.instruction("ldrb w13, [x1, #2]");                                  // 3rd
    emitter.instruction("cmp w13, #117");                                       // 'u'?
    emitter.instruction("b.ne __rt_hed_copy");                                  // no
    emitter.instruction("ldrb w13, [x1, #3]");                                  // 4th
    emitter.instruction("cmp w13, #111");                                       // 'o'?
    emitter.instruction("b.ne __rt_hed_copy");                                  // no
    emitter.instruction("ldrb w13, [x1, #4]");                                  // 5th
    emitter.instruction("cmp w13, #116");                                       // 't'?
    emitter.instruction("b.ne __rt_hed_copy");                                  // no
    emitter.instruction("ldrb w13, [x1, #5]");                                  // 6th
    emitter.instruction("cmp w13, #59");                                        // ';'?
    emitter.instruction("b.ne __rt_hed_copy");                                  // no
    emitter.instruction("mov w12, #34");                                        // matched &quot; → '"'
    emitter.instruction("strb w12, [x9], #1");                                  // write '"'
    emitter.instruction("add x1, x1, #6");                                      // skip 6
    emitter.instruction("sub x11, x11, #6");                                    // decrement
    emitter.instruction("b __rt_hed_loop");                                     // next

    // -- try &#039; --
    emitter.label("__rt_hed_try_apos");
    emitter.instruction("cmp w13, #35");                                        // '#'?
    emitter.instruction("b.ne __rt_hed_copy");                                  // no
    emitter.instruction("ldrb w13, [x1, #2]");                                  // 3rd
    emitter.instruction("cmp w13, #48");                                        // '0'?
    emitter.instruction("b.ne __rt_hed_copy");                                  // no
    emitter.instruction("ldrb w13, [x1, #3]");                                  // 4th
    emitter.instruction("cmp w13, #51");                                        // '3'?
    emitter.instruction("b.ne __rt_hed_copy");                                  // no
    emitter.instruction("ldrb w13, [x1, #4]");                                  // 5th
    emitter.instruction("cmp w13, #57");                                        // '9'?
    emitter.instruction("b.ne __rt_hed_copy");                                  // no
    emitter.instruction("ldrb w13, [x1, #5]");                                  // 6th
    emitter.instruction("cmp w13, #59");                                        // ';'?
    emitter.instruction("b.ne __rt_hed_copy");                                  // no
    emitter.instruction("mov w12, #39");                                        // matched &#039; → '\''
    emitter.instruction("strb w12, [x9], #1");                                  // write '\''
    emitter.instruction("add x1, x1, #6");                                      // skip 6
    emitter.instruction("sub x11, x11, #6");                                    // decrement
    emitter.instruction("b __rt_hed_loop");                                     // next

    // -- copy single byte as-is --
    emitter.label("__rt_hed_copy");
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte, advance source
    emitter.instruction("strb w12, [x9], #1");                                  // store to output
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining
    emitter.instruction("b __rt_hed_loop");                                     // next byte

    // -- finalize --
    emitter.label("__rt_hed_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, x2");                                      // advance
    emitter.instruction("str x8, [x6]");                                        // store updated offset
    emitter.instruction("ret");                                                 // return
}

fn emit_html_entity_decode_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: html_entity_decode ---");
    emitter.label_global("__rt_html_entity_decode");

    // -- set up concat_buf destination --
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the current concat-buffer write offset before decoding HTML entities back into plain bytes
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("lea r11, [r10 + r9]");                                 // compute the concat-buffer destination pointer where the decoded string begins
    emitter.instruction("mov r8, r11");                                         // preserve the concat-backed result start pointer for the returned string value after the loop mutates the destination cursor
    emitter.instruction("mov rcx, rdx");                                        // seed the remaining source length counter from the borrowed entity-encoded input string length
    emitter.instruction("mov rsi, rax");                                        // preserve the borrowed source string cursor in a dedicated register before the loop mutates caller-saved registers

    emitter.label("__rt_hed_loop_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // stop once every source byte has been classified and copied or decoded into concat storage
    emitter.instruction("jz __rt_hed_done_linux_x86_64");                       // finish once the full borrowed entity-encoded input has been consumed
    emitter.instruction("mov dl, BYTE PTR [rsi]");                              // peek at the current source byte before deciding whether an HTML entity starts here
    emitter.instruction("cmp dl, 38");                                          // is the current source byte an ampersand that may start a supported HTML entity?
    emitter.instruction("jne __rt_hed_copy_linux_x86_64");                      // copy bytes that do not begin with '&' straight through without attempting entity decoding

    // -- try &lt; (4 chars: &lt;) --
    emitter.instruction("cmp rcx, 4");                                          // is there enough source length left to match the four-byte `&lt;` entity?
    emitter.instruction("jb __rt_hed_copy_linux_x86_64");                       // copy the leading '&' literally when there are not enough bytes left for any supported entity
    emitter.instruction("movzx r10d, BYTE PTR [rsi + 1]");                      // load the second byte so the decoder can discriminate between `&lt;`, `&gt;`, and longer entities
    emitter.instruction("cmp r10b, 108");                                       // does the entity candidate start with `&l`, which would indicate `&lt;`?
    emitter.instruction("jne __rt_hed_not_lt_linux_x86_64");                    // fall through to the other entity probes when the candidate is not `&lt;`
    emitter.instruction("movzx r10d, BYTE PTR [rsi + 2]");                      // load the third byte to finish validating the `&lt;` entity candidate
    emitter.instruction("cmp r10b, 116");                                       // does the entity candidate contain `t` as the third byte of `&lt;`?
    emitter.instruction("jne __rt_hed_copy_linux_x86_64");                      // copy the leading '&' literally when the candidate diverges from `&lt;`
    emitter.instruction("movzx r10d, BYTE PTR [rsi + 3]");                      // load the fourth byte to validate the terminating semicolon of `&lt;`
    emitter.instruction("cmp r10b, 59");                                        // does the entity candidate terminate with ';' exactly like `&lt;`?
    emitter.instruction("jne __rt_hed_copy_linux_x86_64");                      // copy the leading '&' literally when the `&lt;` candidate is malformed
    emitter.instruction("mov BYTE PTR [r11], 60");                              // write '<' when the current source span exactly matches the supported `&lt;` entity
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after decoding one supported `&lt;` entity
    emitter.instruction("add rsi, 4");                                          // skip the four source bytes consumed by the decoded `&lt;` entity
    emitter.instruction("sub rcx, 4");                                          // shrink the remaining source length after consuming one decoded `&lt;` entity
    emitter.instruction("jmp __rt_hed_loop_linux_x86_64");                      // continue decoding the remainder of the source string after one successful `&lt;` expansion

    // -- try &gt; (4 chars: &gt;) --
    emitter.label("__rt_hed_not_lt_linux_x86_64");
    emitter.instruction("cmp r10b, 103");                                       // does the entity candidate start with `&g`, which would indicate `&gt;`?
    emitter.instruction("jne __rt_hed_not_gt_linux_x86_64");                    // fall through to the other entity probes when the candidate is not `&gt;`
    emitter.instruction("movzx r10d, BYTE PTR [rsi + 2]");                      // load the third byte to finish validating the `&gt;` entity candidate
    emitter.instruction("cmp r10b, 116");                                       // does the entity candidate contain `t` as the third byte of `&gt;`?
    emitter.instruction("jne __rt_hed_copy_linux_x86_64");                      // copy the leading '&' literally when the candidate diverges from `&gt;`
    emitter.instruction("movzx r10d, BYTE PTR [rsi + 3]");                      // load the fourth byte to validate the terminating semicolon of `&gt;`
    emitter.instruction("cmp r10b, 59");                                        // does the entity candidate terminate with ';' exactly like `&gt;`?
    emitter.instruction("jne __rt_hed_copy_linux_x86_64");                      // copy the leading '&' literally when the `&gt;` candidate is malformed
    emitter.instruction("mov BYTE PTR [r11], 62");                              // write '>' when the current source span exactly matches the supported `&gt;` entity
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after decoding one supported `&gt;` entity
    emitter.instruction("add rsi, 4");                                          // skip the four source bytes consumed by the decoded `&gt;` entity
    emitter.instruction("sub rcx, 4");                                          // shrink the remaining source length after consuming one decoded `&gt;` entity
    emitter.instruction("jmp __rt_hed_loop_linux_x86_64");                      // continue decoding the remainder of the source string after one successful `&gt;` expansion

    // -- try &amp; (5 chars) --
    emitter.label("__rt_hed_not_gt_linux_x86_64");
    emitter.instruction("cmp rcx, 5");                                          // is there enough source length left to match the five-byte `&amp;` entity?
    emitter.instruction("jb __rt_hed_try_long_linux_x86_64");                   // skip directly to the longer probes when there are not enough bytes left for `&amp;`
    emitter.instruction("cmp r10b, 97");                                        // does the entity candidate start with `&a`, which would indicate `&amp;`?
    emitter.instruction("jne __rt_hed_try_long_linux_x86_64");                  // skip the `&amp;` probe when the candidate does not begin with `&a`
    emitter.instruction("movzx r10d, BYTE PTR [rsi + 2]");                      // load the third byte to continue validating the `&amp;` entity candidate
    emitter.instruction("cmp r10b, 109");                                       // does the entity candidate contain `m` as the third byte of `&amp;`?
    emitter.instruction("jne __rt_hed_try_long_linux_x86_64");                  // fall through to the longer probes when the `&amp;` candidate diverges here
    emitter.instruction("movzx r10d, BYTE PTR [rsi + 3]");                      // load the fourth byte to continue validating the `&amp;` entity candidate
    emitter.instruction("cmp r10b, 112");                                       // does the entity candidate contain `p` as the fourth byte of `&amp;`?
    emitter.instruction("jne __rt_hed_try_long_linux_x86_64");                  // fall through to the longer probes when the `&amp;` candidate diverges here
    emitter.instruction("movzx r10d, BYTE PTR [rsi + 4]");                      // load the fifth byte to validate the terminating semicolon of `&amp;`
    emitter.instruction("cmp r10b, 59");                                        // does the entity candidate terminate with ';' exactly like `&amp;`?
    emitter.instruction("jne __rt_hed_try_long_linux_x86_64");                  // fall through to the longer probes when the `&amp;` candidate is malformed
    emitter.instruction("mov BYTE PTR [r11], 38");                              // write '&' when the current source span exactly matches the supported `&amp;` entity
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after decoding one supported `&amp;` entity
    emitter.instruction("add rsi, 5");                                          // skip the five source bytes consumed by the decoded `&amp;` entity
    emitter.instruction("sub rcx, 5");                                          // shrink the remaining source length after consuming one decoded `&amp;` entity
    emitter.instruction("jmp __rt_hed_loop_linux_x86_64");                      // continue decoding the remainder of the source string after one successful `&amp;` expansion

    // -- try &quot; or &#039; (6 chars) --
    emitter.label("__rt_hed_try_long_linux_x86_64");
    emitter.instruction("cmp rcx, 6");                                          // is there enough source length left to match the six-byte `&quot;` or `&#039;` entities?
    emitter.instruction("jb __rt_hed_copy_linux_x86_64");                       // copy the leading '&' literally when there are not enough bytes left for the long entity probes
    emitter.instruction("movzx r10d, BYTE PTR [rsi + 1]");                      // reload the second byte so the decoder can discriminate between `&quot;` and `&#039;`
    emitter.instruction("cmp r10b, 113");                                       // does the entity candidate start with `&q`, which would indicate `&quot;`?
    emitter.instruction("jne __rt_hed_try_apos_linux_x86_64");                  // probe the numeric single-quote entity when the candidate does not begin with `&q`
    emitter.instruction("movzx r10d, BYTE PTR [rsi + 2]");                      // load the third byte to continue validating the `&quot;` entity candidate
    emitter.instruction("cmp r10b, 117");                                       // does the entity candidate contain `u` as the third byte of `&quot;`?
    emitter.instruction("jne __rt_hed_copy_linux_x86_64");                      // copy the leading '&' literally when the `&quot;` candidate diverges here
    emitter.instruction("movzx r10d, BYTE PTR [rsi + 3]");                      // load the fourth byte to continue validating the `&quot;` entity candidate
    emitter.instruction("cmp r10b, 111");                                       // does the entity candidate contain `o` as the fourth byte of `&quot;`?
    emitter.instruction("jne __rt_hed_copy_linux_x86_64");                      // copy the leading '&' literally when the `&quot;` candidate diverges here
    emitter.instruction("movzx r10d, BYTE PTR [rsi + 4]");                      // load the fifth byte to continue validating the `&quot;` entity candidate
    emitter.instruction("cmp r10b, 116");                                       // does the entity candidate contain `t` as the fifth byte of `&quot;`?
    emitter.instruction("jne __rt_hed_copy_linux_x86_64");                      // copy the leading '&' literally when the `&quot;` candidate diverges here
    emitter.instruction("movzx r10d, BYTE PTR [rsi + 5]");                      // load the sixth byte to validate the terminating semicolon of `&quot;`
    emitter.instruction("cmp r10b, 59");                                        // does the entity candidate terminate with ';' exactly like `&quot;`?
    emitter.instruction("jne __rt_hed_copy_linux_x86_64");                      // copy the leading '&' literally when the `&quot;` candidate is malformed
    emitter.instruction("mov BYTE PTR [r11], 34");                              // write '\"' when the current source span exactly matches the supported `&quot;` entity
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after decoding one supported `&quot;` entity
    emitter.instruction("add rsi, 6");                                          // skip the six source bytes consumed by the decoded `&quot;` entity
    emitter.instruction("sub rcx, 6");                                          // shrink the remaining source length after consuming one decoded `&quot;` entity
    emitter.instruction("jmp __rt_hed_loop_linux_x86_64");                      // continue decoding the remainder of the source string after one successful `&quot;` expansion

    emitter.label("__rt_hed_try_apos_linux_x86_64");
    emitter.instruction("cmp r10b, 35");                                        // does the entity candidate start with `&#`, which would indicate the numeric single-quote entity?
    emitter.instruction("jne __rt_hed_copy_linux_x86_64");                      // copy the leading '&' literally when the candidate is neither `&quot;` nor `&#039;`
    emitter.instruction("movzx r10d, BYTE PTR [rsi + 2]");                      // load the third byte to continue validating the `&#039;` entity candidate
    emitter.instruction("cmp r10b, 48");                                        // does the entity candidate contain `0` as the third byte of `&#039;`?
    emitter.instruction("jne __rt_hed_copy_linux_x86_64");                      // copy the leading '&' literally when the numeric entity diverges here
    emitter.instruction("movzx r10d, BYTE PTR [rsi + 3]");                      // load the fourth byte to continue validating the `&#039;` entity candidate
    emitter.instruction("cmp r10b, 51");                                        // does the entity candidate contain `3` as the fourth byte of `&#039;`?
    emitter.instruction("jne __rt_hed_copy_linux_x86_64");                      // copy the leading '&' literally when the numeric entity diverges here
    emitter.instruction("movzx r10d, BYTE PTR [rsi + 4]");                      // load the fifth byte to continue validating the `&#039;` entity candidate
    emitter.instruction("cmp r10b, 57");                                        // does the entity candidate contain `9` as the fifth byte of `&#039;`?
    emitter.instruction("jne __rt_hed_copy_linux_x86_64");                      // copy the leading '&' literally when the numeric entity diverges here
    emitter.instruction("movzx r10d, BYTE PTR [rsi + 5]");                      // load the sixth byte to validate the terminating semicolon of `&#039;`
    emitter.instruction("cmp r10b, 59");                                        // does the entity candidate terminate with ';' exactly like `&#039;`?
    emitter.instruction("jne __rt_hed_copy_linux_x86_64");                      // copy the leading '&' literally when the numeric entity is malformed
    emitter.instruction("mov BYTE PTR [r11], 39");                              // write '\\'' when the current source span exactly matches the supported `&#039;` entity
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after decoding one supported `&#039;` entity
    emitter.instruction("add rsi, 6");                                          // skip the six source bytes consumed by the decoded `&#039;` entity
    emitter.instruction("sub rcx, 6");                                          // shrink the remaining source length after consuming one decoded `&#039;` entity
    emitter.instruction("jmp __rt_hed_loop_linux_x86_64");                      // continue decoding the remainder of the source string after one successful `&#039;` expansion

    // -- copy single byte as-is --
    emitter.label("__rt_hed_copy_linux_x86_64");
    emitter.instruction("mov dl, BYTE PTR [rsi]");                              // load the current source byte literally when no supported HTML entity begins at this position
    emitter.instruction("mov BYTE PTR [r11], dl");                              // copy the undecoded source byte straight through into concat storage
    emitter.instruction("add rsi, 1");                                          // advance the borrowed source string cursor after copying one undecoded byte
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after storing one undecoded byte
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining source length after copying one undecoded byte
    emitter.instruction("jmp __rt_hed_loop_linux_x86_64");                      // continue decoding the remainder of the source string after one literal-byte copy

    // -- finalize --
    emitter.label("__rt_hed_done_linux_x86_64");
    emitter.instruction("mov rax, r8");                                         // return the concat-backed result start pointer after decoding the full input string
    emitter.instruction("mov rdx, r11");                                        // copy the final concat-buffer destination cursor before computing the decoded string length
    emitter.instruction("sub rdx, r8");                                         // compute the decoded string length as dest_end - dest_start for the returned x86_64 string value
    emitter.instruction("mov rcx, QWORD PTR [rip + _concat_off]");              // reload the concat-buffer write offset before publishing the bytes that html_entity_decode() appended
    emitter.instruction("add rcx, rdx");                                        // advance the concat-buffer write offset by the produced decoded-string length
    emitter.instruction("mov QWORD PTR [rip + _concat_off], rcx");              // persist the updated concat-buffer write offset after finishing the HTML-entity decode pass
    emitter.instruction("ret");                                                 // return the concat-backed decoded string in the standard x86_64 string result registers
}
