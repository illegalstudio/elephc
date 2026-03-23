use crate::codegen::emit::Emitter;

/// file: read a file into an array of lines.
/// Input:  x1/x2=filename string
/// Output: x0=array pointer (array of strings, each line includes trailing \n)
pub fn emit_file(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: file ---");
    emitter.label("__rt_file");

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
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload array pointer
    emitter.instruction("sub x1, x3, x5");                                      // line start = current pos - line length
    emitter.instruction("mov x2, x5");                                          // line length (including \n)
    emitter.instruction("bl __rt_array_push_str");                              // push line to array
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
