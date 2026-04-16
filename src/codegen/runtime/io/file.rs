use crate::codegen::{emit::Emitter, platform::Arch};

/// file: read a file into an array of lines.
/// Input:  x1/x2=filename string
/// Output: x0=array pointer (array of strings, each line includes trailing \n)
pub fn emit_file(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_file_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: file ---");
    emitter.label_global("__rt_file");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish new frame pointer

    // -- read entire file contents --
    emitter.instruction("bl __rt_file_get_contents");                           // read file, x1=ptr, x2=len
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save file data ptr and len on stack

    // -- create a new string array (capacity = 256 lines) --
    emitter.instruction("mov x0, #256");                                        // initial capacity of 256 elements
    emitter.instruction("mov x1, #16");                                         // element size = 16 bytes (ptr + len)
    emitter.instruction("bl __rt_array_new");                                   // create array, x0=array pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save array pointer on stack

    // -- scan file data for newlines and push each line --
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // reload file data ptr and total len
    emitter.instruction("mov x3, x1");                                          // x3 = current line start pointer
    emitter.instruction("add x4, x1, x2");                                      // x4 = pointer past end of data
    emitter.instruction("mov x5, #0");                                          // x5 = current line length counter

    emitter.label("__rt_file_scan");
    emitter.instruction("cmp x3, x4");                                          // check if we've reached end of data
    emitter.instruction("b.hs __rt_file_last");                                 // if at or past end, handle last line

    // -- check current byte --
    emitter.instruction("ldrb w6, [x3]");                                       // load current byte
    emitter.instruction("add x3, x3, #1");                                      // advance scan pointer
    emitter.instruction("add x5, x5, #1");                                      // increment line length
    emitter.instruction("cmp w6, #0x0A");                                       // compare with newline
    emitter.instruction("b.ne __rt_file_scan");                                 // if not newline, continue scanning

    // -- found newline: push this line to array --
    emitter.instruction("str x3, [sp, #24]");                                   // save scan pointer (push_str clobbers x3)
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload array pointer
    emitter.instruction("sub x1, x3, x5");                                      // line start = current pos - line length
    emitter.instruction("mov x2, x5");                                          // line length (including \n)
    emitter.instruction("bl __rt_array_push_str");                              // push line to array (x0 = possibly new array)
    emitter.instruction("str x0, [sp, #16]");                                   // update array pointer after possible growth
    emitter.instruction("ldr x3, [sp, #24]");                                   // restore scan pointer
    emitter.instruction("mov x5, #0");                                          // reset line length for next line

    // -- reload scan state and continue --
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // reload original data ptr and len
    emitter.instruction("add x4, x1, x2");                                      // recompute end pointer
    emitter.instruction("b __rt_file_scan");                                    // continue scanning

    // -- handle last line (no trailing newline) --
    emitter.label("__rt_file_last");
    emitter.instruction("cbz x5, __rt_file_ret");                               // if last line is empty, skip it
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload array pointer
    emitter.instruction("sub x1, x3, x5");                                      // line start = current pos - line length
    emitter.instruction("mov x2, x5");                                          // line length
    emitter.instruction("bl __rt_array_push_str");                              // push last line to array

    // -- return array pointer --
    emitter.label("__rt_file_ret");
    emitter.instruction("ldr x0, [sp, #16]");                                   // return array pointer

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_file_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: file ---");
    emitter.label_global("__rt_file");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while file() uses scan state and array spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the file payload, scan cursors, and result array pointer
    emitter.instruction("sub rsp, 64");                                         // reserve aligned spill slots for the file payload, line scan cursors, and result array pointer

    emitter.instruction("call __rt_file_get_contents");                         // read the full file payload into an owned elephc string before splitting it into lines
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the owned file payload pointer across the later array allocation and line pushes
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the owned file payload length across the later array allocation and scan loop

    emitter.instruction("mov rdi, 256");                                        // request an initial array capacity of 256 line slots for the line-splitting helper
    emitter.instruction("mov rsi, 16");                                         // request 16-byte elements so each slot can hold a string pointer and string length pair
    emitter.instruction("call __rt_array_new");                                 // allocate the result array that will collect the split file lines
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the result array pointer across the line-scan loop and possible growth helpers

    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // load the owned file payload pointer into the active scan cursor register
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // preserve the current line start pointer separately from the active scan cursor
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // load the full file payload length before computing the end-of-buffer pointer
    emitter.instruction("lea r11, [r8 + r10]");                                 // compute the pointer one byte past the end of the owned file payload
    emitter.instruction("xor rcx, rcx");                                        // start the current line length counter at zero before scanning the file payload

    emitter.label("__rt_file_scan");
    emitter.instruction("cmp r8, r11");                                         // stop the scan loop once the active cursor reaches the end of the file payload
    emitter.instruction("jae __rt_file_last");                                  // finish with the final partial line once the end of the file payload is reached
    emitter.instruction("mov dl, BYTE PTR [r8]");                               // load the current file payload byte before deciding whether a line terminator was reached
    emitter.instruction("add r8, 1");                                           // advance the active scan cursor after consuming one source byte from the file payload
    emitter.instruction("add rcx, 1");                                          // extend the current line length after consuming one source byte from the file payload
    emitter.instruction("cmp dl, 0x0A");                                        // test whether the consumed byte is a line-feed terminator
    emitter.instruction("jne __rt_file_scan");                                  // continue scanning the current line until a terminating line-feed is found

    emitter.instruction("mov QWORD PTR [rbp - 32], r8");                        // preserve the active scan cursor because array_push_str() is free to clobber caller-saved registers
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the result array pointer into the x86_64 append-helper receiver register
    emitter.instruction("mov rsi, r9");                                         // pass the current line start pointer as the string payload argument to array_push_str()
    emitter.instruction("mov rdx, rcx");                                        // pass the completed line length, including the trailing newline, to array_push_str()
    emitter.instruction("call __rt_array_push_str");                            // append the completed line slice as an owned string in the result array
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the updated array pointer after array_push_str() handles possible growth
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // restore the active scan cursor after the append helper clobbers caller-saved registers
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the full file payload length before rebuilding the end-of-buffer pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the owned file payload base pointer before rebuilding the end-of-buffer pointer
    emitter.instruction("add r11, r10");                                        // rebuild the pointer one byte past the end of the owned file payload after the helper call
    emitter.instruction("mov r9, r8");                                          // start the next line at the scan cursor immediately after the consumed newline
    emitter.instruction("xor rcx, rcx");                                        // reset the current line length counter before scanning the next line
    emitter.instruction("jmp __rt_file_scan");                                  // continue scanning the remaining bytes in the file payload for more newline terminators

    emitter.label("__rt_file_last");
    emitter.instruction("test rcx, rcx");                                       // detect whether the file ended with a partial line that still needs to be appended
    emitter.instruction("jz __rt_file_cleanup");                                // skip the final push when the file already ended exactly on a newline boundary
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the result array pointer into the x86_64 append-helper receiver register
    emitter.instruction("mov rsi, r9");                                         // pass the trailing line start pointer as the string payload argument to array_push_str()
    emitter.instruction("mov rdx, rcx");                                        // pass the trailing line length without a newline terminator to array_push_str()
    emitter.instruction("call __rt_array_push_str");                            // append the trailing partial line as an owned string in the result array
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the updated array pointer after appending the trailing partial line

    emitter.label("__rt_file_cleanup");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the result array pointer in the canonical x86_64 integer result register
    emitter.instruction("add rsp, 64");                                         // release the temporary file payload and scan-state spill slots used by file()
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the line array
    emitter.instruction("ret");                                                 // return the array of file lines to the caller
}
