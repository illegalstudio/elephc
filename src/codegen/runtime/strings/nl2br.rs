use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// nl2br: insert "<br />\n" before each newline.
/// Input: x1/x2=string. Output: x1/x2=result.
pub fn emit_nl2br(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_nl2br_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: nl2br ---");
    emitter.label_global("__rt_nl2br");

    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("mov x11, x2");                                         // remaining count

    emitter.label("__rt_nl2br_loop");
    emitter.instruction("cbz x11, __rt_nl2br_done");                            // no bytes left → done
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte, advance source
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining
    emitter.instruction("cmp w12, #10");                                        // is it '\n'?
    emitter.instruction("b.ne __rt_nl2br_store");                               // no → store as-is
    // -- insert "<br />" before the newline --
    emitter.instruction("mov w13, #60");                                        // '<'
    emitter.instruction("strb w13, [x9], #1");                                  // write '<'
    emitter.instruction("mov w13, #98");                                        // 'b'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'b'
    emitter.instruction("mov w13, #114");                                       // 'r'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'r'
    emitter.instruction("mov w13, #32");                                        // ' '
    emitter.instruction("strb w13, [x9], #1");                                  // write ' '
    emitter.instruction("mov w13, #47");                                        // '/'
    emitter.instruction("strb w13, [x9], #1");                                  // write '/'
    emitter.instruction("mov w13, #62");                                        // '>'
    emitter.instruction("strb w13, [x9], #1");                                  // write '>'
    emitter.label("__rt_nl2br_store");
    emitter.instruction("strb w12, [x9], #1");                                  // write original byte (including '\n')
    emitter.instruction("b __rt_nl2br_loop");                                   // next byte

    emitter.label("__rt_nl2br_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, x2");                                      // advance
    emitter.instruction("str x8, [x6]");                                        // store updated offset
    emitter.instruction("ret");                                                 // return
}

fn emit_nl2br_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: nl2br ---");
    emitter.label_global("__rt_nl2br");

    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the current concat-buffer write offset before expanding newline bytes into HTML break tags
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("lea r11, [r10 + r9]");                                 // compute the concat-buffer destination pointer where the nl2br() result begins
    emitter.instruction("mov r8, r11");                                         // preserve the concat-backed result start pointer for the returned string value after the loop mutates the destination cursor
    emitter.instruction("mov rcx, rdx");                                        // seed the remaining source length counter from the borrowed input string length
    emitter.instruction("mov rsi, rax");                                        // preserve the borrowed source string cursor in a dedicated register before the loop mutates caller-saved registers

    emitter.label("__rt_nl2br_loop_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // stop once every source byte has been classified and copied into concat storage
    emitter.instruction("jz __rt_nl2br_done_linux_x86_64");                     // finish once the borrowed source string has been fully consumed
    emitter.instruction("mov dl, BYTE PTR [rsi]");                              // load one source byte before deciding whether nl2br() must inject a break tag
    emitter.instruction("add rsi, 1");                                          // advance the borrowed source string cursor after consuming one byte
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining source length after consuming one byte
    emitter.instruction("cmp dl, 10");                                          // is the current byte a newline that should gain a preceding HTML break tag?
    emitter.instruction("jne __rt_nl2br_store_linux_x86_64");                   // copy non-newline bytes straight through without injecting extra HTML markup
    emitter.instruction("mov BYTE PTR [r11], 60");                              // write '<' as the first byte of the injected `<br />` break tag
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the '<' of the break tag
    emitter.instruction("mov BYTE PTR [r11], 98");                              // write 'b' as the second byte of the injected `<br />` break tag
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the 'b' of the break tag
    emitter.instruction("mov BYTE PTR [r11], 114");                             // write 'r' as the third byte of the injected `<br />` break tag
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the 'r' of the break tag
    emitter.instruction("mov BYTE PTR [r11], 32");                              // write the space byte of the injected `<br />` break tag
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the space of the break tag
    emitter.instruction("mov BYTE PTR [r11], 47");                              // write '/' as the fifth byte of the injected `<br />` break tag
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the slash of the break tag
    emitter.instruction("mov BYTE PTR [r11], 62");                              // write '>' as the final byte of the injected `<br />` break tag
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after emitting the closing angle bracket of the break tag

    emitter.label("__rt_nl2br_store_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11], dl");                              // store the original source byte, including newline bytes after any injected break tag prefix
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after storing the original source byte
    emitter.instruction("jmp __rt_nl2br_loop_linux_x86_64");                    // continue scanning the remaining source bytes until the input string is exhausted

    emitter.label("__rt_nl2br_done_linux_x86_64");
    emitter.instruction("mov rax, r8");                                         // return the concat-backed result start pointer after nl2br() finishes expanding the input string
    emitter.instruction("mov rdx, r11");                                        // copy the final concat-buffer destination cursor before computing the produced string length
    emitter.instruction("sub rdx, r8");                                         // compute the produced string length as dest_end - dest_start for the returned x86_64 string value
    emitter.instruction("mov rcx, QWORD PTR [rip + _concat_off]");              // reload the concat-buffer write offset before publishing the bytes that nl2br() appended
    emitter.instruction("add rcx, rdx");                                        // advance the concat-buffer write offset by the produced string length that nl2br() just materialized
    emitter.instruction("mov QWORD PTR [rip + _concat_off], rcx");              // persist the updated concat-buffer write offset after finishing the nl2br() expansion
    emitter.instruction("ret");                                                 // return the concat-backed nl2br() result in the standard x86_64 string result registers
}
