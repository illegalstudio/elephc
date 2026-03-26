use crate::codegen::emit::Emitter;

/// scandir: list directory entries as an array of strings.
/// Input:  x1/x2=path string
/// Output: x0=array pointer (array of filename strings)
pub fn emit_scandir(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: scandir ---");
    emitter.label("__rt_scandir");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish new frame pointer

    // -- null-terminate path --
    emitter.instruction("bl __rt_cstr");                                        // convert path to C string, x0=cstr

    // -- open directory --
    emitter.instruction("bl _opendir");                                         // opendir(cstr), x0=DIR* or NULL
    emitter.instruction("str x0, [sp, #0]");                                    // save DIR pointer on stack

    // -- create a new string array (capacity = 128 entries) --
    emitter.instruction("mov x0, #128");                                        // initial capacity of 128 elements
    emitter.instruction("mov x1, #16");                                         // element size = 16 bytes (ptr + len)
    emitter.instruction("bl __rt_array_new");                                   // create array, x0=array pointer
    emitter.instruction("str x0, [sp, #8]");                                    // save array pointer on stack

    // -- read directory entries in a loop --
    emitter.label("__rt_scandir_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload DIR pointer
    emitter.instruction("bl _readdir");                                         // readdir(DIR*), x0=dirent* or NULL
    emitter.instruction("cbz x0, __rt_scandir_close");                          // if NULL, no more entries

    // -- extract d_name and d_namlen from dirent struct --
    emitter.instruction("ldrh w9, [x0, #18]");                                  // load d_namlen (uint16 at offset 18)
    emitter.instruction("add x1, x0, #21");                                     // d_name starts at offset 21
    emitter.instruction("mov x2, x9");                                          // string length = d_namlen

    // -- copy name to concat_buf so it persists after next readdir call --
    emitter.instruction("str x0, [sp, #16]");                                   // save dirent pointer (will be clobbered)
    emitter.instruction("bl __rt_str_persist");                                // copy string to heap, x1=new ptr, x2=len

    // -- push name string to array --
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload array pointer
    emitter.instruction("bl __rt_array_push_str");                              // push name to array
    emitter.instruction("str x0, [sp, #8]");                                   // update array pointer after possible realloc
    emitter.instruction("b __rt_scandir_loop");                                 // continue reading entries

    // -- close directory and return --
    emitter.label("__rt_scandir_close");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload DIR pointer
    emitter.instruction("bl _closedir");                                        // closedir(DIR*)

    // -- return array pointer --
    emitter.instruction("ldr x0, [sp, #8]");                                    // return array pointer

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
