use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// ucwords: uppercase first letter of each word (after whitespace).
/// Input: x1=ptr, x2=len. Output: x1=new_ptr, x2=len.
pub fn emit_ucwords(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ucwords_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ucwords ---");
    emitter.label_global("__rt_ucwords");
    emitter.instruction("sub sp, sp, #16");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // set frame pointer
    emitter.instruction("bl __rt_strcopy");                                     // copy string to mutable concat_buf
    emitter.instruction("cbz x2, __rt_ucwords_done");                           // empty string → nothing to do
    emitter.instruction("mov x9, x1");                                          // cursor pointer
    emitter.instruction("mov x10, x2");                                         // remaining length
    emitter.instruction("mov x11, #1");                                         // word_start flag (1 = next char starts a word)

    emitter.label("__rt_ucwords_loop");
    emitter.instruction("cbz x10, __rt_ucwords_done");                          // no bytes left → done
    emitter.instruction("ldrb w12, [x9]");                                      // load current byte
    // -- check if current char is whitespace --
    emitter.instruction("cmp w12, #32");                                        // space?
    emitter.instruction("b.eq __rt_ucwords_ws");                                // yes → mark next as word start
    emitter.instruction("cmp w12, #9");                                         // tab?
    emitter.instruction("b.eq __rt_ucwords_ws");                                // yes → mark next as word start
    emitter.instruction("cmp w12, #10");                                        // newline?
    emitter.instruction("b.eq __rt_ucwords_ws");                                // yes → mark next as word start
    // -- not whitespace: uppercase if word_start --
    emitter.instruction("cbz x11, __rt_ucwords_next");                          // not word start → skip uppercasing
    emitter.instruction("cmp w12, #97");                                        // check if char >= 'a'
    emitter.instruction("b.lt __rt_ucwords_clear");                             // not lowercase → just clear flag
    emitter.instruction("cmp w12, #122");                                       // check if char <= 'z'
    emitter.instruction("b.gt __rt_ucwords_clear");                             // not lowercase → just clear flag
    emitter.instruction("sub w12, w12, #32");                                   // convert a-z to A-Z
    emitter.instruction("strb w12, [x9]");                                      // store uppercased byte
    emitter.label("__rt_ucwords_clear");
    emitter.instruction("mov x11, #0");                                         // clear word_start flag
    emitter.instruction("b __rt_ucwords_next");                                 // advance to next char

    emitter.label("__rt_ucwords_ws");
    emitter.instruction("mov x11, #1");                                         // set word_start flag for next char

    emitter.label("__rt_ucwords_next");
    emitter.instruction("add x9, x9, #1");                                      // advance cursor
    emitter.instruction("sub x10, x10, #1");                                    // decrement remaining
    emitter.instruction("b __rt_ucwords_loop");                                 // process next byte

    emitter.label("__rt_ucwords_done");
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x1/x2 from strcopy
}

fn emit_ucwords_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ucwords ---");
    emitter.label_global("__rt_ucwords");
    emitter.instruction("call __rt_strcopy");                                   // copy the source string into concat storage so ucwords() can mutate bytes in place without touching borrowed input
    emitter.instruction("test rdx, rdx");                                       // skip the word-start scan when ucwords() receives an empty string
    emitter.instruction("jz __rt_ucwords_done_linux_x86_64");                   // return immediately when there are no bytes to uppercase
    emitter.instruction("mov r8, rax");                                         // seed the mutable string cursor with the concat-backed copy returned by __rt_strcopy
    emitter.instruction("mov rcx, rdx");                                        // seed the remaining-length counter from the copied string length returned by __rt_strcopy
    emitter.instruction("mov r9, 1");                                           // start in word-start mode so the first non-whitespace byte can be uppercased when appropriate

    emitter.label("__rt_ucwords_loop_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // stop once every byte of the concat-backed copy has been classified
    emitter.instruction("jz __rt_ucwords_done_linux_x86_64");                   // finish once the full copied string has been processed
    emitter.instruction("movzx r10d, BYTE PTR [r8]");                           // load the current byte from the mutable concat-backed copy before classifying whitespace and ASCII case
    emitter.instruction("cmp r10b, 32");                                        // is the current byte a space that marks the start of the next word?
    emitter.instruction("je __rt_ucwords_ws_linux_x86_64");                     // mark the next byte as a word start after a space separator
    emitter.instruction("cmp r10b, 9");                                         // is the current byte a tab that marks the start of the next word?
    emitter.instruction("je __rt_ucwords_ws_linux_x86_64");                     // mark the next byte as a word start after a tab separator
    emitter.instruction("cmp r10b, 10");                                        // is the current byte a newline that marks the start of the next word?
    emitter.instruction("je __rt_ucwords_ws_linux_x86_64");                     // mark the next byte as a word start after a newline separator
    emitter.instruction("test r9, r9");                                         // should ucwords() try to uppercase the current non-whitespace byte?
    emitter.instruction("jz __rt_ucwords_next_linux_x86_64");                   // skip the ASCII-case conversion when the current byte is inside an existing word
    emitter.instruction("cmp r10b, 97");                                        // compare the current byte against 'a' to detect lowercase ASCII letters
    emitter.instruction("jb __rt_ucwords_clear_linux_x86_64");                  // clear word-start mode without mutating bytes below 'a'
    emitter.instruction("cmp r10b, 122");                                       // compare the current byte against 'z' to bound the lowercase ASCII range
    emitter.instruction("ja __rt_ucwords_clear_linux_x86_64");                  // clear word-start mode without mutating bytes above 'z'
    emitter.instruction("sub r10b, 32");                                        // convert the first lowercase ASCII letter of the word to uppercase
    emitter.instruction("mov BYTE PTR [r8], r10b");                             // store the uppercased first letter back into the mutable concat-backed copy

    emitter.label("__rt_ucwords_clear_linux_x86_64");
    emitter.instruction("mov r9, 0");                                           // clear word-start mode after the first byte of the current word has been handled
    emitter.instruction("jmp __rt_ucwords_next_linux_x86_64");                  // advance to the next byte after handling the current word-start candidate

    emitter.label("__rt_ucwords_ws_linux_x86_64");
    emitter.instruction("mov r9, 1");                                           // mark the next non-whitespace byte as the start of a new word after a separator

    emitter.label("__rt_ucwords_next_linux_x86_64");
    emitter.instruction("add r8, 1");                                           // advance the mutable string cursor after classifying the current byte
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining byte count after processing one byte from the copied string
    emitter.instruction("jmp __rt_ucwords_loop_linux_x86_64");                  // continue processing bytes until the full copied string has been classified

    emitter.label("__rt_ucwords_done_linux_x86_64");
    emitter.instruction("ret");                                                 // return the mutated concat-backed copy in the standard x86_64 string result registers
}
