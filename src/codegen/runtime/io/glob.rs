use crate::codegen::emit::Emitter;

/// glob: find pathnames matching a pattern.
/// Input:  x1/x2=pattern string
/// Output: x0=array pointer (array of matching path strings)
pub fn emit_glob(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: glob ---");
    emitter.label("__rt_glob");

    // -- set up stack frame (128 bytes for glob_t + locals + frame) --
    emitter.instruction("sub sp, sp, #176");                                    // allocate 176 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #160]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #160");                                   // establish new frame pointer

    // -- null-terminate pattern --
    emitter.instruction("bl __rt_cstr");                                        // convert pattern to C string, x0=cstr

    // -- call glob(pattern, 0, NULL, &glob_result) --
    emitter.instruction("str x0, [sp, #0]");                                    // save pattern cstr
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload pattern as first arg
    emitter.instruction("mov x1, #0");                                          // flags = 0
    emitter.instruction("mov x2, #0");                                          // errfunc = NULL
    emitter.instruction("add x3, sp, #16");                                     // pointer to glob_t struct on stack
    emitter.instruction("bl _glob");                                            // call glob(), x0=return code
    emitter.instruction("str x0, [sp, #8]");                                    // save return code

    // -- create result array --
    emitter.instruction("mov x0, #128");                                        // initial capacity of 128 elements
    emitter.instruction("mov x1, #16");                                         // element size = 16 bytes (ptr + len)
    emitter.instruction("bl __rt_array_new");                                   // create array, x0=array pointer
    emitter.instruction("str x0, [sp, #144]");                                  // save array pointer on stack

    // -- check if glob succeeded --
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload return code
    emitter.instruction("cbnz x9, __rt_glob_ret");                              // if non-zero, return empty array

    // -- loop through matched paths --
    emitter.instruction("ldr x9, [sp, #16]");                                   // load gl_pathc (number of matches, offset 0)
    emitter.instruction("str x9, [sp, #152]");                                  // save match count
    emitter.instruction("ldr x10, [sp, #40]");                                  // load gl_pathv (char**, offset 24 in glob_t)
    emitter.instruction("mov x11, #0");                                         // initialize loop index

    emitter.label("__rt_glob_loop");
    emitter.instruction("ldr x9, [sp, #152]");                                  // reload match count
    emitter.instruction("cmp x11, x9");                                         // check if we've processed all matches
    emitter.instruction("b.hs __rt_glob_free");                                 // if done, free and return
    emitter.instruction("str x11, [sp, #148]");                                 // save current index

    // -- load path pointer from pathv[i] --
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload gl_pathv base
    emitter.instruction("lsl x12, x11, #3");                                    // byte offset = index * 8
    emitter.instruction("ldr x1, [x10, x12]");                                  // load pathv[i] = char* to path

    // -- calculate string length by scanning for null --
    emitter.instruction("mov x2, #0");                                          // initialize length counter
    emitter.label("__rt_glob_strlen");
    emitter.instruction("ldrb w13, [x1, x2]");                                  // load byte at current position
    emitter.instruction("cbz w13, __rt_glob_push");                             // if null terminator, done counting
    emitter.instruction("add x2, x2, #1");                                      // increment length
    emitter.instruction("b __rt_glob_strlen");                                  // continue scanning

    // -- copy string and push to array --
    emitter.label("__rt_glob_push");
    emitter.instruction("bl __rt_strcopy");                                     // copy to concat_buf for persistence
    emitter.instruction("ldr x0, [sp, #144]");                                  // reload array pointer
    emitter.instruction("bl __rt_array_push_str");                              // push path to array

    // -- advance to next entry --
    emitter.instruction("ldr x11, [sp, #148]");                                 // reload current index
    emitter.instruction("add x11, x11, #1");                                    // increment index
    emitter.instruction("b __rt_glob_loop");                                    // continue loop

    // -- free glob resources --
    emitter.label("__rt_glob_free");
    emitter.instruction("add x0, sp, #16");                                     // pointer to glob_t struct
    emitter.instruction("bl _globfree");                                        // free glob results

    // -- return array pointer --
    emitter.label("__rt_glob_ret");
    emitter.instruction("ldr x0, [sp, #144]");                                  // return array pointer

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #160]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #176");                                    // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
