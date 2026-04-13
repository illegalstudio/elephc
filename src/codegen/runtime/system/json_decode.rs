use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// __rt_json_decode: decode a JSON string value.
/// Input:  x1=json string ptr, x2=json string len
/// Output: x1=decoded string ptr, x2=decoded string len
///
/// Supported JSON inputs:
///   - Quoted strings: "hello" → hello (with unescape)
///   - Numbers: 42 → "42" (returned as string representation)
///   - true/false/null → returned as literal string
///
/// This is a simplified implementation that handles the most common case:
/// stripping quotes and unescaping a JSON string value.
pub fn emit_json_decode(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_json_decode_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: json_decode ---");
    emitter.label_global("__rt_json_decode");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set new frame pointer

    // -- check if input starts with a double quote --
    emitter.instruction("cbz x2, __rt_json_decode_empty");                      // empty string → return empty
    emitter.instruction("ldrb w9, [x1]");                                       // load first byte
    emitter.instruction("cmp w9, #34");                                         // check for double quote
    emitter.instruction("b.ne __rt_json_decode_passthrough");                   // not a quoted string, return as-is

    // -- it's a JSON string: strip quotes and unescape --
    emitter.instruction("str x1, [sp, #0]");                                    // save source ptr
    emitter.instruction("str x2, [sp, #8]");                                    // save source len

    // -- get output position in concat_buf --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x11, x11, x10");                                   // output position
    emitter.instruction("str x11, [sp, #16]");                                  // save output start
    emitter.instruction("str x11, [sp, #24]");                                  // save output write pos

    // -- skip opening quote, process until closing quote --
    emitter.instruction("mov x12, #1");                                         // source index (skip opening quote)
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload source len
    emitter.instruction("sub x2, x2, #1");                                      // subtract 1 for closing quote

    emitter.label("__rt_json_decode_loop");
    emitter.instruction("cmp x12, x2");                                         // check if at closing quote
    emitter.instruction("b.ge __rt_json_decode_done");                          // done

    emitter.instruction("ldr x1, [sp, #0]");                                    // reload source ptr
    emitter.instruction("ldrb w9, [x1, x12]");                                  // load source byte

    // -- check for escape sequence --
    emitter.instruction("cmp w9, #92");                                         // check for backslash
    emitter.instruction("b.ne __rt_json_decode_literal");                       // not escape, copy literal

    // -- process escape sequence --
    emitter.instruction("add x12, x12, #1");                                    // skip backslash
    emitter.instruction("ldrb w9, [x1, x12]");                                  // load escaped char

    emitter.instruction("cmp w9, #110");                                        // check for 'n' (newline)
    emitter.instruction("b.ne __rt_json_decode_esc_not_n");                     // not newline
    emitter.instruction("mov w9, #10");                                         // replace with actual newline
    emitter.instruction("b __rt_json_decode_literal");                          // store it
    emitter.label("__rt_json_decode_esc_not_n");

    emitter.instruction("cmp w9, #116");                                        // check for 't' (tab)
    emitter.instruction("b.ne __rt_json_decode_esc_not_t");                     // not tab
    emitter.instruction("mov w9, #9");                                          // replace with actual tab
    emitter.instruction("b __rt_json_decode_literal");                          // store it
    emitter.label("__rt_json_decode_esc_not_t");

    emitter.instruction("cmp w9, #114");                                        // check for 'r' (carriage return)
    emitter.instruction("b.ne __rt_json_decode_literal");                       // not CR, use char as-is (handles \" and \\)
    emitter.instruction("mov w9, #13");                                         // replace with actual CR

    // -- write literal or unescaped character --
    emitter.label("__rt_json_decode_literal");
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload write pos
    emitter.instruction("strb w9, [x11]");                                      // write byte
    emitter.instruction("add x11, x11, #1");                                    // advance
    emitter.instruction("str x11, [sp, #24]");                                  // save write pos
    emitter.instruction("add x12, x12, #1");                                    // advance source index
    emitter.instruction("b __rt_json_decode_loop");                             // continue

    // -- finalize --
    emitter.label("__rt_json_decode_done");
    emitter.instruction("ldr x1, [sp, #16]");                                   // x1 = output start
    emitter.instruction("ldr x11, [sp, #24]");                                  // load write end
    emitter.instruction("sub x2, x11, x1");                                     // x2 = output length

    // -- update concat_off --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    emitter.instruction("add x10, x10, x2");                                    // add result length
    emitter.instruction("str x10, [x9]");                                       // store updated offset

    emitter.instruction("b __rt_json_decode_ret");                              // return

    // -- empty input --
    emitter.label("__rt_json_decode_empty");
    emitter.instruction("mov x1, #0");                                          // null ptr
    emitter.instruction("mov x2, #0");                                          // zero length
    emitter.instruction("b __rt_json_decode_ret");                              // return

    // -- passthrough (not a quoted string) --
    emitter.label("__rt_json_decode_passthrough");
    // x1 and x2 already contain the input — return as-is

    // -- tear down and return --
    emitter.label("__rt_json_decode_ret");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_json_decode_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_decode ---");
    emitter.label_global("__rt_json_decode");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving JSON-decode scratch space
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the source slice and concat-buffer cursors
    emitter.instruction("sub rsp, 48");                                         // reserve local slots for source ptr/len, output pointers, write cursor, and source index
    emitter.instruction("test rdx, rdx");                                       // does the incoming JSON slice have any bytes to decode?
    emitter.instruction("jz __rt_json_decode_empty");                           // empty input returns an empty borrowed string slice immediately
    emitter.instruction("movzx r10, BYTE PTR [rax]");                           // load the first JSON byte to decide whether this is a quoted string payload
    emitter.instruction("cmp r10b, 34");                                        // does the JSON slice start with a double quote?
    emitter.instruction("jne __rt_json_decode_passthrough");                    // non-string JSON payloads stay as their original borrowed input slice
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the source JSON pointer across the decode loop and concat-buffer writes
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the source JSON length across the decode loop and concat-buffer writes
    emitter.instruction("mov r10, QWORD PTR [rip + _concat_off]");              // load the current concat-buffer absolute offset before appending decoded bytes
    emitter.instruction("lea r11, [rip + _concat_buf]");                        // materialize the concat-buffer base pointer for the decoded output slice
    emitter.instruction("add r11, r10");                                        // compute the current concat-buffer write pointer from the base plus offset
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // save the decoded-string start pointer for the final string result slice
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the current concat-buffer write pointer for the decode loop
    emitter.instruction("mov QWORD PTR [rbp - 40], 1");                         // initialize the source index to the byte after the opening JSON quote
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the source JSON length before turning it into the closing-quote boundary
    emitter.instruction("sub rcx, 1");                                          // stop the decode loop before the closing JSON quote byte
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // save the closing-quote boundary across the decode loop

    emitter.label("__rt_json_decode_loop");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the current source index at the top of the JSON decode loop
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 48]");                       // have we reached the closing JSON quote boundary already?
    emitter.instruction("jae __rt_json_decode_done");                           // finish once every quoted payload byte has been decoded
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source JSON pointer for the current byte fetch
    emitter.instruction("movzx r11, BYTE PTR [r10 + rcx]");                     // load the next source byte and widen it so escape comparisons stay unsigned
    emitter.instruction("cmp r11b, 92");                                        // does the current source byte start a JSON escape sequence?
    emitter.instruction("jne __rt_json_decode_literal");                        // ordinary bytes copy directly into the decoded output slice
    emitter.instruction("add rcx, 1");                                          // advance past the backslash to inspect the escaped JSON codepoint
    emitter.instruction("movzx r11, BYTE PTR [r10 + rcx]");                     // load the escaped JSON codepoint after the backslash prefix
    emitter.instruction("cmp r11b, 110");                                       // does the escape sequence encode a newline?
    emitter.instruction("jne __rt_json_decode_esc_not_n");                      // keep checking escape families until one matches
    emitter.instruction("mov r11b, 10");                                        // decode \\n into an actual newline byte in the output string
    emitter.instruction("jmp __rt_json_decode_literal");                        // write the decoded newline byte through the shared literal write path

    emitter.label("__rt_json_decode_esc_not_n");
    emitter.instruction("cmp r11b, 116");                                       // does the escape sequence encode a horizontal tab?
    emitter.instruction("jne __rt_json_decode_esc_not_t");                      // keep checking escape families until one matches
    emitter.instruction("mov r11b, 9");                                         // decode \\t into an actual tab byte in the output string
    emitter.instruction("jmp __rt_json_decode_literal");                        // write the decoded tab byte through the shared literal write path

    emitter.label("__rt_json_decode_esc_not_t");
    emitter.instruction("cmp r11b, 114");                                       // does the escape sequence encode a carriage return?
    emitter.instruction("jne __rt_json_decode_literal");                        // quote and backslash escapes fall through as their literal escaped byte
    emitter.instruction("mov r11b, 13");                                        // decode \\r into an actual carriage-return byte in the output string

    emitter.label("__rt_json_decode_literal");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current concat-buffer write pointer before appending the decoded byte
    emitter.instruction("mov BYTE PTR [r10], r11b");                            // write the decoded or literal byte into the concat buffer
    emitter.instruction("add r10, 1");                                          // advance the concat-buffer write pointer after appending the decoded byte
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // persist the updated write pointer for the next iteration
    emitter.instruction("add rcx, 1");                                          // advance to the next source byte after consuming this literal or escape sequence
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // persist the updated source index for the next loop iteration
    emitter.instruction("jmp __rt_json_decode_loop");                           // continue decoding the remaining quoted JSON payload bytes

    emitter.label("__rt_json_decode_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the decoded-string start pointer in the leading x86_64 string result register
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // reload the final concat-buffer write pointer before turning it into a slice length
    emitter.instruction("sub rdx, rax");                                        // compute the decoded-string length from write_end - write_start
    emitter.instruction("lea r10, [rip + _concat_buf]");                        // materialize the concat-buffer base pointer for the global offset update
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // copy the final concat-buffer write pointer before converting it into an absolute offset
    emitter.instruction("sub rcx, r10");                                        // compute the new absolute concat-buffer offset after the decoded string slice
    emitter.instruction("mov QWORD PTR [rip + _concat_off], rcx");              // publish the updated concat-buffer offset so later writers append after this decoded string
    emitter.instruction("jmp __rt_json_decode_ret");                            // return the decoded concat-backed string slice through the shared epilogue

    emitter.label("__rt_json_decode_empty");
    emitter.instruction("xor rax, rax");                                        // return a null pointer for empty decoded output
    emitter.instruction("xor rdx, rdx");                                        // return a zero-length decoded slice for empty decoded output
    emitter.instruction("jmp __rt_json_decode_ret");                            // return the empty decoded slice through the shared epilogue

    emitter.label("__rt_json_decode_passthrough");
    emitter.instruction("jmp __rt_json_decode_ret");                            // return the original borrowed input slice for numeric and literal JSON payloads

    emitter.label("__rt_json_decode_ret");
    emitter.instruction("add rsp, 48");                                         // release the local JSON-decode scratch frame before returning to generated code
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to generated code
    emitter.instruction("ret");                                                 // return the decoded JSON slice in the x86_64 string result registers
}
