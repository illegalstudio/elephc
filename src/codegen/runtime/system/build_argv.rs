use crate::codegen::emit::Emitter;

/// build_argv: create a PHP $argv array from OS argc/argv.
/// Reads _global_argc and _global_argv, builds a string array.
/// Output: x0 = pointer to array
pub fn emit_build_argv(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: build_argv ---");
    emitter.label_global("__rt_build_argv");

    // -- set up stack frame (48 bytes for locals + saved registers) --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer

    // -- load argc from the global variable --
    emitter.instruction("adrp x9, _global_argc@PAGE");                          // load page base of _global_argc into x9
    emitter.instruction("add x9, x9, _global_argc@PAGEOFF");                    // add page offset to get exact address
    emitter.instruction("ldr x19, [x9]");                                       // x19 = argc (callee-saved register)

    // -- load argv pointer from the global variable --
    emitter.instruction("adrp x9, _global_argv@PAGE");                          // load page base of _global_argv into x9
    emitter.instruction("add x9, x9, _global_argv@PAGEOFF");                    // add page offset to get exact address
    emitter.instruction("ldr x20, [x9]");                                       // x20 = argv pointer (callee-saved register)

    // -- save callee-saved registers we're about to use --
    emitter.instruction("stp x19, x20, [sp, #0]");                              // save x19 (argc) and x20 (argv) to stack
    emitter.instruction("str x21, [sp, #16]");                                  // save x21 (will hold array pointer)

    // -- create a new string array with capacity = argc --
    emitter.instruction("mov x0, x19");                                         // arg0: capacity = argc
    emitter.instruction("mov x1, #16");                                         // arg1: elem_size = 16 (ptr + len per string)
    emitter.instruction("bl __rt_array_new");                                   // allocate the array, x0 = array pointer
    emitter.instruction("mov x21, x0");                                         // x21 = array pointer (save in callee-saved reg)

    // -- initialize loop counter i = 0 --
    emitter.instruction("mov x22, #0");                                         // x22 = 0 (loop counter)
    emitter.instruction("str x22, [sp, #24]");                                  // store i on stack (survives function calls)

    // -- loop: for i = 0..argc, convert each C string and push to array --
    emitter.label("__rt_build_argv_loop");
    emitter.instruction("ldr x22, [sp, #24]");                                  // reload i from stack
    emitter.instruction("cmp x22, x19");                                        // compare i with argc
    emitter.instruction("b.ge __rt_build_argv_done");                           // if i >= argc, exit loop

    // -- get pointer to argv[i] (C string) --
    emitter.instruction("ldr x1, [x20, x22, lsl #3]");                          // x1 = argv[i] (load pointer at argv + i*8)

    // -- compute string length by scanning for null terminator --
    emitter.instruction("mov x2, #0");                                          // x2 = 0 (length counter)
    emitter.label("__rt_build_argv_strlen");
    emitter.instruction("ldrb w3, [x1, x2]");                                   // w3 = byte at str[length] (load single byte)
    emitter.instruction("cbz w3, __rt_build_argv_push");                        // if byte == 0 (null terminator), done counting
    emitter.instruction("add x2, x2, #1");                                      // length += 1
    emitter.instruction("b __rt_build_argv_strlen");                            // continue scanning

    // -- push the string (ptr in x1, len in x2) to the array --
    emitter.label("__rt_build_argv_push");
    emitter.instruction("mov x0, x21");                                         // arg0: array pointer
    emitter.instruction("bl __rt_array_push_str");                              // push string element to array
    emitter.instruction("mov x21, x0");                                         // update array pointer after possible realloc

    // -- increment loop counter and continue --
    emitter.instruction("ldr x22, [sp, #24]");                                  // reload i from stack (may have been clobbered)
    emitter.instruction("add x22, x22, #1");                                    // i += 1
    emitter.instruction("str x22, [sp, #24]");                                  // save updated i back to stack
    emitter.instruction("b __rt_build_argv_loop");                              // continue loop

    // -- loop complete, return the array pointer --
    emitter.label("__rt_build_argv_done");
    emitter.instruction("mov x0, x21");                                         // return value: array pointer in x0

    // -- restore callee-saved registers and tear down stack frame --
    emitter.instruction("ldp x19, x20, [sp, #0]");                              // restore x19 (argc) and x20 (argv)
    emitter.instruction("ldr x21, [sp, #16]");                                  // restore x21
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
