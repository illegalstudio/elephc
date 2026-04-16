use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// htmlspecialchars: replace &, ", ', <, > with HTML entities.
/// Input: x1/x2=string. Output: x1/x2=result in concat_buf.
pub fn emit_htmlspecialchars(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_htmlspecialchars_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: htmlspecialchars ---");
    emitter.label_global("__rt_htmlspecialchars");

    // -- set up concat_buf destination --
    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("mov x11, x2");                                         // remaining byte count

    emitter.label("__rt_htmlsc_loop");
    emitter.instruction("cbz x11, __rt_htmlsc_done");                           // no bytes left -> done
    emitter.instruction("ldrb w12, [x1], #1");                                  // load source byte, advance
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining

    // -- check & (38) -> &amp; --
    emitter.instruction("cmp w12, #38");                                        // is it '&'?
    emitter.instruction("b.eq __rt_htmlsc_amp");                                // yes -> write &amp;

    // -- check " (34) -> &quot; --
    emitter.instruction("cmp w12, #34");                                        // is it '"'?
    emitter.instruction("b.eq __rt_htmlsc_quot");                               // yes -> write &quot;

    // -- check ' (39) -> &#039; --
    emitter.instruction("cmp w12, #39");                                        // is it '\''?
    emitter.instruction("b.eq __rt_htmlsc_apos");                               // yes -> write &#039;

    // -- check < (60) -> &lt; --
    emitter.instruction("cmp w12, #60");                                        // is it '<'?
    emitter.instruction("b.eq __rt_htmlsc_lt");                                 // yes -> write &lt;

    // -- check > (62) -> &gt; --
    emitter.instruction("cmp w12, #62");                                        // is it '>'?
    emitter.instruction("b.eq __rt_htmlsc_gt");                                 // yes -> write &gt;

    // -- store unmodified byte --
    emitter.instruction("strb w12, [x9], #1");                                  // store byte as-is
    emitter.instruction("b __rt_htmlsc_loop");                                  // next byte

    // -- &amp; (5 bytes: &, a, m, p, ;) --
    emitter.label("__rt_htmlsc_amp");
    emitter.instruction("mov w13, #38");                                        // '&'
    emitter.instruction("strb w13, [x9], #1");                                  // write '&'
    emitter.instruction("mov w13, #97");                                        // 'a'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'a'
    emitter.instruction("mov w13, #109");                                       // 'm'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'm'
    emitter.instruction("mov w13, #112");                                       // 'p'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'p'
    emitter.instruction("mov w13, #59");                                        // ';'
    emitter.instruction("strb w13, [x9], #1");                                  // write ';'
    emitter.instruction("b __rt_htmlsc_loop");                                  // next byte

    // -- &quot; (6 bytes: &, q, u, o, t, ;) --
    emitter.label("__rt_htmlsc_quot");
    emitter.instruction("mov w13, #38");                                        // '&'
    emitter.instruction("strb w13, [x9], #1");                                  // write '&'
    emitter.instruction("mov w13, #113");                                       // 'q'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'q'
    emitter.instruction("mov w13, #117");                                       // 'u'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'u'
    emitter.instruction("mov w13, #111");                                       // 'o'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'o'
    emitter.instruction("mov w13, #116");                                       // 't'
    emitter.instruction("strb w13, [x9], #1");                                  // write 't'
    emitter.instruction("mov w13, #59");                                        // ';'
    emitter.instruction("strb w13, [x9], #1");                                  // write ';'
    emitter.instruction("b __rt_htmlsc_loop");                                  // next byte

    // -- &#039; (6 bytes: &, #, 0, 3, 9, ;) --
    emitter.label("__rt_htmlsc_apos");
    emitter.instruction("mov w13, #38");                                        // '&'
    emitter.instruction("strb w13, [x9], #1");                                  // write '&'
    emitter.instruction("mov w13, #35");                                        // '#'
    emitter.instruction("strb w13, [x9], #1");                                  // write '#'
    emitter.instruction("mov w13, #48");                                        // '0'
    emitter.instruction("strb w13, [x9], #1");                                  // write '0'
    emitter.instruction("mov w13, #51");                                        // '3'
    emitter.instruction("strb w13, [x9], #1");                                  // write '3'
    emitter.instruction("mov w13, #57");                                        // '9'
    emitter.instruction("strb w13, [x9], #1");                                  // write '9'
    emitter.instruction("mov w13, #59");                                        // ';'
    emitter.instruction("strb w13, [x9], #1");                                  // write ';'
    emitter.instruction("b __rt_htmlsc_loop");                                  // next byte

    // -- &lt; (4 bytes: &, l, t, ;) --
    emitter.label("__rt_htmlsc_lt");
    emitter.instruction("mov w13, #38");                                        // '&'
    emitter.instruction("strb w13, [x9], #1");                                  // write '&'
    emitter.instruction("mov w13, #108");                                       // 'l'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'l'
    emitter.instruction("mov w13, #116");                                       // 't'
    emitter.instruction("strb w13, [x9], #1");                                  // write 't'
    emitter.instruction("mov w13, #59");                                        // ';'
    emitter.instruction("strb w13, [x9], #1");                                  // write ';'
    emitter.instruction("b __rt_htmlsc_loop");                                  // next byte

    // -- &gt; (4 bytes: &, g, t, ;) --
    emitter.label("__rt_htmlsc_gt");
    emitter.instruction("mov w13, #38");                                        // '&'
    emitter.instruction("strb w13, [x9], #1");                                  // write '&'
    emitter.instruction("mov w13, #103");                                       // 'g'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'g'
    emitter.instruction("mov w13, #116");                                       // 't'
    emitter.instruction("strb w13, [x9], #1");                                  // write 't'
    emitter.instruction("mov w13, #59");                                        // ';'
    emitter.instruction("strb w13, [x9], #1");                                  // write ';'
    emitter.instruction("b __rt_htmlsc_loop");                                  // next byte

    emitter.label("__rt_htmlsc_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, x2");                                      // advance by result length
    emitter.instruction("str x8, [x6]");                                        // store updated offset
    emitter.instruction("ret");                                                 // return
}

fn emit_htmlspecialchars_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: htmlspecialchars ---");
    emitter.label_global("__rt_htmlspecialchars");

    // -- set up concat_buf destination --
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the current concat-buffer write offset before expanding HTML-sensitive characters
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("lea r11, [r10 + r9]");                                 // compute the concat-buffer destination pointer where the escaped HTML string begins
    emitter.instruction("mov r8, r11");                                         // preserve the concat-backed result start pointer for the returned string value after the loop mutates the destination cursor
    emitter.instruction("mov rcx, rdx");                                        // seed the remaining source length counter from the borrowed input string length
    emitter.instruction("mov rsi, rax");                                        // preserve the borrowed source string cursor in a dedicated register before the loop mutates caller-saved registers

    emitter.label("__rt_htmlsc_loop_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // stop once every source byte has been classified and copied into concat storage
    emitter.instruction("jz __rt_htmlsc_done_linux_x86_64");                    // finish once the full borrowed source string has been escaped
    emitter.instruction("mov dl, BYTE PTR [rsi]");                              // load one source byte before deciding whether it maps to a named HTML entity
    emitter.instruction("add rsi, 1");                                          // advance the borrowed source string cursor after consuming one byte
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining source length after consuming one byte
    emitter.instruction("cmp dl, 38");                                          // is the current byte an ampersand that must expand to `&amp;`?
    emitter.instruction("je __rt_htmlsc_amp_linux_x86_64");                     // write the ampersand entity expansion when the current byte is '&'
    emitter.instruction("cmp dl, 34");                                          // is the current byte a double quote that must expand to `&quot;`?
    emitter.instruction("je __rt_htmlsc_quot_linux_x86_64");                    // write the double-quote entity expansion when the current byte is '\"'
    emitter.instruction("cmp dl, 39");                                          // is the current byte a single quote that must expand to `&#039;`?
    emitter.instruction("je __rt_htmlsc_apos_linux_x86_64");                    // write the single-quote entity expansion when the current byte is '\\''
    emitter.instruction("cmp dl, 60");                                          // is the current byte a less-than sign that must expand to `&lt;`?
    emitter.instruction("je __rt_htmlsc_lt_linux_x86_64");                      // write the less-than entity expansion when the current byte is '<'
    emitter.instruction("cmp dl, 62");                                          // is the current byte a greater-than sign that must expand to `&gt;`?
    emitter.instruction("je __rt_htmlsc_gt_linux_x86_64");                      // write the greater-than entity expansion when the current byte is '>'
    emitter.instruction("mov BYTE PTR [r11], dl");                              // store source bytes that do not need HTML escaping directly into concat storage
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after copying an unescaped source byte
    emitter.instruction("jmp __rt_htmlsc_loop_linux_x86_64");                   // continue escaping the remaining source bytes until the input string is exhausted

    emitter.label("__rt_htmlsc_amp_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11], 38");                              // write '&' as the first byte of the `&amp;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the first byte of `&amp;`
    emitter.instruction("mov BYTE PTR [r11], 97");                              // write 'a' as the second byte of the `&amp;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the second byte of `&amp;`
    emitter.instruction("mov BYTE PTR [r11], 109");                             // write 'm' as the third byte of the `&amp;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the third byte of `&amp;`
    emitter.instruction("mov BYTE PTR [r11], 112");                             // write 'p' as the fourth byte of the `&amp;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the fourth byte of `&amp;`
    emitter.instruction("mov BYTE PTR [r11], 59");                              // write ';' as the terminating byte of the `&amp;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the final byte of `&amp;`
    emitter.instruction("jmp __rt_htmlsc_loop_linux_x86_64");                   // continue escaping the remaining source bytes after expanding one ampersand

    emitter.label("__rt_htmlsc_quot_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11], 38");                              // write '&' as the first byte of the `&quot;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the first byte of `&quot;`
    emitter.instruction("mov BYTE PTR [r11], 113");                             // write 'q' as the second byte of the `&quot;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the second byte of `&quot;`
    emitter.instruction("mov BYTE PTR [r11], 117");                             // write 'u' as the third byte of the `&quot;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the third byte of `&quot;`
    emitter.instruction("mov BYTE PTR [r11], 111");                             // write 'o' as the fourth byte of the `&quot;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the fourth byte of `&quot;`
    emitter.instruction("mov BYTE PTR [r11], 116");                             // write 't' as the fifth byte of the `&quot;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the fifth byte of `&quot;`
    emitter.instruction("mov BYTE PTR [r11], 59");                              // write ';' as the terminating byte of the `&quot;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the final byte of `&quot;`
    emitter.instruction("jmp __rt_htmlsc_loop_linux_x86_64");                   // continue escaping the remaining source bytes after expanding one double quote

    emitter.label("__rt_htmlsc_apos_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11], 38");                              // write '&' as the first byte of the `&#039;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the first byte of `&#039;`
    emitter.instruction("mov BYTE PTR [r11], 35");                              // write '#' as the second byte of the `&#039;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the second byte of `&#039;`
    emitter.instruction("mov BYTE PTR [r11], 48");                              // write '0' as the third byte of the `&#039;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the third byte of `&#039;`
    emitter.instruction("mov BYTE PTR [r11], 51");                              // write '3' as the fourth byte of the `&#039;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the fourth byte of `&#039;`
    emitter.instruction("mov BYTE PTR [r11], 57");                              // write '9' as the fifth byte of the `&#039;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the fifth byte of `&#039;`
    emitter.instruction("mov BYTE PTR [r11], 59");                              // write ';' as the terminating byte of the `&#039;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the final byte of `&#039;`
    emitter.instruction("jmp __rt_htmlsc_loop_linux_x86_64");                   // continue escaping the remaining source bytes after expanding one single quote

    emitter.label("__rt_htmlsc_lt_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11], 38");                              // write '&' as the first byte of the `&lt;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the first byte of `&lt;`
    emitter.instruction("mov BYTE PTR [r11], 108");                             // write 'l' as the second byte of the `&lt;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the second byte of `&lt;`
    emitter.instruction("mov BYTE PTR [r11], 116");                             // write 't' as the third byte of the `&lt;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the third byte of `&lt;`
    emitter.instruction("mov BYTE PTR [r11], 59");                              // write ';' as the terminating byte of the `&lt;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the final byte of `&lt;`
    emitter.instruction("jmp __rt_htmlsc_loop_linux_x86_64");                   // continue escaping the remaining source bytes after expanding one less-than sign

    emitter.label("__rt_htmlsc_gt_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11], 38");                              // write '&' as the first byte of the `&gt;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the first byte of `&gt;`
    emitter.instruction("mov BYTE PTR [r11], 103");                             // write 'g' as the second byte of the `&gt;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the second byte of `&gt;`
    emitter.instruction("mov BYTE PTR [r11], 116");                             // write 't' as the third byte of the `&gt;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the third byte of `&gt;`
    emitter.instruction("mov BYTE PTR [r11], 59");                              // write ';' as the terminating byte of the `&gt;` entity expansion
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the final byte of `&gt;`
    emitter.instruction("jmp __rt_htmlsc_loop_linux_x86_64");                   // continue escaping the remaining source bytes after expanding one greater-than sign

    emitter.label("__rt_htmlsc_done_linux_x86_64");
    emitter.instruction("mov rax, r8");                                         // return the concat-backed result start pointer after escaping the full input string
    emitter.instruction("mov rdx, r11");                                        // copy the final concat-buffer destination cursor before computing the escaped string length
    emitter.instruction("sub rdx, r8");                                         // compute the escaped string length as dest_end - dest_start for the returned x86_64 string value
    emitter.instruction("mov rcx, QWORD PTR [rip + _concat_off]");              // reload the concat-buffer write offset before publishing the bytes that htmlspecialchars() appended
    emitter.instruction("add rcx, rdx");                                        // advance the concat-buffer write offset by the produced escaped-string length
    emitter.instruction("mov QWORD PTR [rip + _concat_off], rcx");              // persist the updated concat-buffer write offset after finishing the HTML-escape expansion
    emitter.instruction("ret");                                                 // return the concat-backed escaped string in the standard x86_64 string result registers
}
