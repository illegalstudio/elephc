use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// str_split: split string into array of chunks.
/// Input: x1/x2=string, x3=chunk_length. Output: x0=array pointer.
pub fn emit_str_split(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_split_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: str_split ---");
    emitter.label_global("__rt_str_split");
    emitter.instruction("sub sp, sp, #64");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save string ptr/len
    emitter.instruction("str x3, [sp, #16]");                                   // save chunk length

    // -- create array --
    emitter.instruction("mov x0, #16");                                         // initial capacity
    emitter.instruction("mov x1, #16");                                         // elem_size = 16 (str ptr+len)
    emitter.instruction("bl __rt_array_new");                                   // allocate new array
    emitter.instruction("str x0, [sp, #24]");                                   // save array pointer
    emitter.instruction("str xzr, [sp, #32]");                                  // current position = 0

    emitter.label("__rt_str_split_loop");
    emitter.instruction("ldr x4, [sp, #32]");                                   // load current position
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload string ptr/len
    emitter.instruction("cmp x4, x2");                                          // past end of string?
    emitter.instruction("b.ge __rt_str_split_done");                            // yes → done

    // -- compute this chunk's actual length --
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload chunk length
    emitter.instruction("sub x5, x2, x4");                                      // remaining = len - pos
    emitter.instruction("cmp x5, x3");                                          // remaining vs chunk_length
    emitter.instruction("csel x5, x3, x5, gt");                                 // chunk = min(remaining, chunk_length)

    // -- push chunk as string element --
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload array pointer
    emitter.instruction("add x1, x1, x4");                                      // x1 = base + current position
    emitter.instruction("mov x2, x5");                                          // x2 = chunk length
    emitter.instruction("bl __rt_array_push_str");                              // push chunk onto array
    emitter.instruction("str x0, [sp, #24]");                                   // update array pointer after possible realloc

    // -- advance position by chunk length --
    emitter.instruction("ldr x4, [sp, #32]");                                   // reload position
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload chunk length
    emitter.instruction("add x4, x4, x3");                                      // position += chunk_length
    emitter.instruction("str x4, [sp, #32]");                                   // save updated position
    emitter.instruction("b __rt_str_split_loop");                               // continue

    emitter.label("__rt_str_split_done");
    emitter.instruction("ldr x0, [sp, #24]");                                   // return array pointer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame
    emitter.instruction("add sp, sp, #64");                                     // deallocate
    emitter.instruction("ret");                                                 // return
}

fn emit_str_split_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_split ---");
    emitter.label_global("__rt_str_split");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving str_split() spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved source string, chunk length, array pointer, and cursor
    emitter.instruction("sub rsp, 48");                                         // reserve aligned spill slots for the source string, chunk length, array pointer, and current position
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the source string pointer across array allocation and push helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the source string length across array allocation and push helper calls
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // preserve the requested chunk length across array allocation and push helper calls
    emitter.instruction("mov edi, 16");                                         // seed the result array with the same initial capacity used by the AArch64 str_split() helper
    emitter.instruction("mov esi, 16");                                         // use 16-byte string slots (ptr + len) for the str_split() result array payload
    emitter.instruction("call __rt_array_new");                                 // allocate the result array that will hold the fixed-size string chunks
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the result array pointer across later push helper calls and possible growth
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // start scanning the source string from byte offset zero

    emitter.label("__rt_str_split_loop_linux_x86_64");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the current source-string byte offset before testing loop completion
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the source-string length before testing loop completion
    emitter.instruction("cmp rcx, r8");                                         // have we already consumed every byte of the source string?
    emitter.instruction("jge __rt_str_split_done_linux_x86_64");                // stop once the current offset reaches the source-string length
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the requested chunk length before clamping the final chunk
    emitter.instruction("mov r10, r8");                                         // copy the remaining-length base so the final chunk length can be clamped to the source tail
    emitter.instruction("sub r10, rcx");                                        // compute how many bytes remain from the current source offset to the end of the string
    emitter.instruction("cmp r10, r9");                                         // is the remaining tail shorter than the requested chunk length?
    emitter.instruction("cmovl r9, r10");                                       // clamp the chunk length down to the remaining tail for the final partial chunk
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source string pointer before computing the chunk start address
    emitter.instruction("add rax, rcx");                                        // advance the source string pointer to the start of the current chunk slice
    emitter.instruction("mov rdx, r9");                                         // move the clamped chunk length into the x86_64 string-helper length register
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the current result array pointer before appending the next chunk slice
    emitter.instruction("mov rsi, rax");                                        // pass the current chunk slice pointer to the string-array append helper
    emitter.instruction("call __rt_array_push_str");                            // append the current chunk slice as an owned string entry, growing the result array if needed
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the possibly grown result array pointer returned by the append helper
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the current source-string byte offset before advancing to the next chunk
    emitter.instruction("add rcx, QWORD PTR [rbp - 24]");                       // advance by the requested chunk length so the next iteration starts at the correct source offset
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // preserve the updated source-string byte offset for the next loop iteration
    emitter.instruction("jmp __rt_str_split_loop_linux_x86_64");                // continue splitting the source string until every byte has been chunked

    emitter.label("__rt_str_split_done_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // return the result array pointer after the final chunk append has completed
    emitter.instruction("add rsp, 48");                                         // release the str_split() spill slots before returning the result array
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to the caller
    emitter.instruction("ret");                                                 // return the result array pointer in the standard x86_64 integer result register
}
