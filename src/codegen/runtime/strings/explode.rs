use crate::codegen::{emit::Emitter, platform::Arch};

/// explode: split string by delimiter into array of strings.
/// Input: x1/x2=delimiter, x3/x4=string
/// Output: x0 = array pointer
pub fn emit_explode(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_explode_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: explode ---");
    emitter.label_global("__rt_explode");

    // -- set up stack frame (80 bytes) --
    emitter.instruction("sub sp, sp, #80");                                     // allocate 80 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish new frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save delimiter ptr and length
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save input string ptr and length

    // -- create a new string array --
    emitter.instruction("mov x0, #16");                                         // initial array capacity = 16 elements
    emitter.instruction("mov x1, #16");                                         // element size = 16 bytes (ptr + len)
    emitter.instruction("bl __rt_array_new");                                   // call array constructor, returns array in x0
    emitter.instruction("str x0, [sp, #32]");                                   // save array pointer on stack

    // -- initialize scan state --
    emitter.instruction("mov x13, #0");                                         // current scan position = 0
    emitter.instruction("str x13, [sp, #40]");                                  // save current scan position
    emitter.instruction("str x13, [sp, #48]");                                  // segment start = 0

    // -- main loop: scan for delimiter occurrences --
    emitter.label("__rt_explode_loop");
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // reload string ptr and length
    emitter.instruction("ldr x13, [sp, #40]");                                  // reload current scan position
    emitter.instruction("cmp x13, x4");                                         // check if past end of string
    emitter.instruction("b.ge __rt_explode_last");                              // if done, push final segment

    // -- check if delimiter fits at current position --
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload delimiter ptr and length
    emitter.instruction("sub x14, x4, x13");                                    // remaining = string_len - scan_pos
    emitter.instruction("cmp x2, x14");                                         // check if delimiter fits in remaining
    emitter.instruction("b.gt __rt_explode_last");                              // delimiter longer than remaining, done

    // -- compare delimiter at current position --
    emitter.instruction("mov x15, #0");                                         // delimiter comparison index = 0
    emitter.label("__rt_explode_cmp");
    emitter.instruction("cmp x15, x2");                                         // check if all delimiter bytes matched
    emitter.instruction("b.ge __rt_explode_match");                             // full match, delimiter found
    emitter.instruction("add x16, x13, x15");                                   // compute string index = scan_pos + cmp_idx
    emitter.instruction("ldrb w17, [x3, x16]");                                 // load string byte at computed index
    emitter.instruction("ldrb w18, [x1, x15]");                                 // load delimiter byte at cmp index
    emitter.instruction("cmp w17, w18");                                        // compare string and delimiter bytes
    emitter.instruction("b.ne __rt_explode_advance");                           // mismatch, advance by 1
    emitter.instruction("add x15, x15, #1");                                    // advance delimiter index
    emitter.instruction("b __rt_explode_cmp");                                  // continue comparing

    // -- no match: advance scan position by 1 --
    emitter.label("__rt_explode_advance");
    emitter.instruction("add x13, x13, #1");                                    // move scan position forward by 1
    emitter.instruction("str x13, [sp, #40]");                                  // save updated scan position
    emitter.instruction("b __rt_explode_loop");                                 // continue scanning

    // -- delimiter found: push segment before it to array --
    emitter.label("__rt_explode_match");
    emitter.instruction("ldr x0, [sp, #32]");                                   // load array pointer
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // reload string ptr and length
    emitter.instruction("ldr x16, [sp, #48]");                                  // load segment start position
    emitter.instruction("add x1, x3, x16");                                     // segment ptr = string + segment_start
    emitter.instruction("sub x2, x13, x16");                                    // segment len = scan_pos - segment_start
    emitter.instruction("bl __rt_array_push_str");                              // push segment string to array
    emitter.instruction("str x0, [sp, #32]");                                   // update array pointer after possible realloc

    // -- advance past delimiter, update segment start --
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload delimiter ptr and length
    emitter.instruction("ldr x13, [sp, #40]");                                  // reload scan position
    emitter.instruction("add x13, x13, x2");                                    // skip past delimiter
    emitter.instruction("str x13, [sp, #40]");                                  // save new scan position
    emitter.instruction("str x13, [sp, #48]");                                  // update segment start to after delimiter
    emitter.instruction("b __rt_explode_loop");                                 // continue scanning

    // -- push final segment (from last delimiter to end of string) --
    emitter.label("__rt_explode_last");
    emitter.instruction("ldr x0, [sp, #32]");                                   // load array pointer
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // reload string ptr and length
    emitter.instruction("ldr x16, [sp, #48]");                                  // load segment start position
    emitter.instruction("add x1, x3, x16");                                     // segment ptr = string + segment_start
    emitter.instruction("sub x2, x4, x16");                                     // segment len = string_len - segment_start
    emitter.instruction("bl __rt_array_push_str");                              // push final segment to array
    emitter.instruction("str x0, [sp, #32]");                                   // update array pointer after possible realloc

    // -- return array and restore frame --
    emitter.instruction("ldr x0, [sp, #32]");                                   // return array pointer in x0
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_explode_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: explode ---");
    emitter.label_global("__rt_explode");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before the splitter uses stack-backed scan state
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved delimiter, subject string, and scan cursors
    emitter.instruction("sub rsp, 64");                                         // reserve aligned local storage for the saved delimiter pair, subject pair, array pointer, and scan indices
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the delimiter pointer so every scan iteration can reload it without depending on caller-saved registers
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the delimiter length so the fit check survives helper calls and loop back-edges
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // save the subject-string pointer so every scan iteration can reload it without depending on caller-saved registers
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // save the subject-string length so the fit and final-segment checks survive helper calls

    emitter.instruction("mov rdi, 16");                                         // request an initial indexed-array capacity of sixteen string slots for explode()
    emitter.instruction("mov rsi, 16");                                         // declare that each explode() element occupies sixteen bytes as a ptr+len string slot
    emitter.instruction("call __rt_array_new");                                 // allocate the initial indexed array that will receive each extracted string segment
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the indexed-array pointer because every push helper may reallocate it
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // initialize the scan position to the start of the subject string
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // initialize the current segment start position to the start of the subject string

    emitter.label("__rt_explode_loop_linux_x86_64");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the current scan position before checking whether the subject string has been exhausted
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 32]");                       // stop scanning once the scan position reaches the subject-string length
    emitter.instruction("jae __rt_explode_last_linux_x86_64");                  // append the trailing segment when the scan position reaches the end of the subject string
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the delimiter length before checking whether it still fits at the current scan position
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // reload the subject-string length before computing the remaining scan window
    emitter.instruction("sub r9, rcx");                                         // compute the number of subject bytes remaining at the current scan position
    emitter.instruction("cmp r8, r9");                                          // stop scanning once the delimiter becomes longer than the remaining subject-string suffix
    emitter.instruction("ja __rt_explode_last_linux_x86_64");                   // append the trailing segment when the delimiter can no longer fit in the remaining subject suffix

    emitter.instruction("xor r10, r10");                                        // start the delimiter-comparison byte index at zero before checking the current scan position
    emitter.label("__rt_explode_cmp_linux_x86_64");
    emitter.instruction("cmp r10, r8");                                         // stop comparing once every delimiter byte has matched at the current scan position
    emitter.instruction("jae __rt_explode_match_linux_x86_64");                 // treat the current scan position as a delimiter hit when every delimiter byte matched
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the subject-string pointer before reading the candidate byte at the current scan position
    emitter.instruction("mov rax, rcx");                                        // seed the subject-byte offset with the current scan position
    emitter.instruction("add rax, r10");                                        // add the delimiter-comparison byte index to form the exact subject-byte offset to test
    emitter.instruction("mov dl, BYTE PTR [r11 + rax]");                        // load the subject byte that should match the delimiter byte at the same comparison index
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the delimiter pointer before reading the delimiter byte for the same comparison index
    emitter.instruction("mov al, BYTE PTR [r11 + r10]");                        // load the delimiter byte that should match the subject byte at the current comparison index
    emitter.instruction("cmp dl, al");                                          // compare the subject and delimiter bytes at the current comparison index
    emitter.instruction("jne __rt_explode_advance_linux_x86_64");               // abandon the current scan position when any delimiter byte mismatches the subject
    emitter.instruction("add r10, 1");                                          // advance the delimiter-comparison byte index after one successful byte match
    emitter.instruction("jmp __rt_explode_cmp_linux_x86_64");                   // continue comparing the remaining delimiter bytes at the current scan position

    emitter.label("__rt_explode_advance_linux_x86_64");
    emitter.instruction("add rcx, 1");                                          // advance the scan position by one subject byte after a delimiter mismatch
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // publish the advanced scan position before starting the next scan iteration
    emitter.instruction("jmp __rt_explode_loop_linux_x86_64");                  // continue scanning the subject string for the next delimiter occurrence

    emitter.label("__rt_explode_match_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the indexed-array pointer before pushing the subject segment that precedes the matched delimiter
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the subject-string pointer before forming the segment substring pointer
    emitter.instruction("mov r8, QWORD PTR [rbp - 56]");                        // reload the current segment start position before computing the substring pointer and length
    emitter.instruction("lea rsi, [r11 + r8]");                                 // compute the segment substring pointer from the subject-string base plus the saved segment start offset
    emitter.instruction("mov rdx, rcx");                                        // seed the segment length with the current scan position where the delimiter match starts
    emitter.instruction("sub rdx, r8");                                         // convert the scan position into the segment length by subtracting the saved segment start offset
    emitter.instruction("mov rdi, rax");                                        // move the indexed-array pointer into the x86_64 receiver register expected by the string-append helper
    emitter.instruction("call __rt_array_push_str");                            // append the subject segment that precedes the matched delimiter to the indexed result array
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the possibly-reallocated indexed-array pointer returned by the string-append helper
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the scan position because the string-append helper may clobber caller-saved registers
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the delimiter length so the scan position can skip past the full matched delimiter
    emitter.instruction("add rcx, r8");                                         // advance the scan position to the first subject byte after the matched delimiter
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // publish the advanced scan position after skipping the matched delimiter
    emitter.instruction("mov QWORD PTR [rbp - 56], rcx");                       // start the next segment immediately after the matched delimiter
    emitter.instruction("jmp __rt_explode_loop_linux_x86_64");                  // continue scanning for subsequent delimiter occurrences in the remaining subject suffix

    emitter.label("__rt_explode_last_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the indexed-array pointer before pushing the trailing subject segment
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the subject-string pointer before forming the trailing substring pointer
    emitter.instruction("mov r8, QWORD PTR [rbp - 56]");                        // reload the trailing segment start position saved after the last delimiter match
    emitter.instruction("lea rsi, [r11 + r8]");                                 // compute the trailing segment pointer from the subject-string base plus the saved segment start offset
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // seed the trailing segment length with the full subject-string length
    emitter.instruction("sub rdx, r8");                                         // compute the trailing segment length from the full subject length minus the saved segment start offset
    emitter.instruction("mov rdi, rax");                                        // move the indexed-array pointer into the x86_64 receiver register expected by the string-append helper
    emitter.instruction("call __rt_array_push_str");                            // append the trailing subject segment after the final delimiter occurrence to the indexed result array
    emitter.instruction("add rsp, 64");                                         // release the splitter locals after the final segment has been appended to the result array
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the indexed explode() result array
    emitter.instruction("ret");                                                 // return the indexed explode() result array pointer in the standard x86_64 integer result register
}
