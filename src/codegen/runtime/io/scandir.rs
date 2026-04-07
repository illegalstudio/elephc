use crate::codegen::emit::Emitter;

/// scandir: list directory entries as an array of strings.
/// Input:  x1/x2=path string
/// Output: x0=array pointer (array of filename strings)
pub fn emit_scandir(emitter: &mut Emitter) {
    let name_off = emitter.platform.dirent_name_offset();

    emitter.blank();
    emitter.comment("--- runtime: scandir ---");
    emitter.label_global("__rt_scandir");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish new frame pointer

    // -- null-terminate path --
    emitter.instruction("bl __rt_cstr");                                        // convert path to C string, x0=cstr

    // -- open directory --
    emitter.bl_c("opendir");                                         // opendir(cstr), x0=DIR* or NULL
    emitter.instruction("str x0, [sp, #0]");                                    // save DIR pointer on stack

    // -- create a new string array (capacity = 128 entries) --
    emitter.instruction("mov x0, #128");                                        // initial capacity of 128 elements
    emitter.instruction("mov x1, #16");                                         // element size = 16 bytes (ptr + len)
    emitter.instruction("bl __rt_array_new");                                   // create array, x0=array pointer
    emitter.instruction("str x0, [sp, #8]");                                    // save array pointer on stack

    // -- read directory entries in a loop --
    emitter.label("__rt_scandir_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload DIR pointer
    emitter.bl_c("readdir");                                         // readdir(DIR*), x0=dirent* or NULL
    emitter.instruction("cbz x0, __rt_scandir_close");                          // if NULL, no more entries

    // -- point at d_name and measure it until the terminating NUL --
    emitter.instruction(&format!("add x1, x0, #{}", name_off));                 // x1 = pointer to dirent.d_name for this platform
    emitter.instruction("mov x2, #0");                                          // x2 = filename length
    emitter.label("__rt_scandir_strlen");
    emitter.instruction("ldrb w9, [x1, x2]");                                   // load the next byte from d_name
    emitter.instruction("cbz w9, __rt_scandir_name_ready");                     // stop at the terminating NUL byte
    emitter.instruction("add x2, x2, #1");                                      // count one more filename byte
    emitter.instruction("b __rt_scandir_strlen");                               // continue scanning the filename
    emitter.label("__rt_scandir_name_ready");

    // -- copy name to concat_buf so it persists after next readdir call --
    emitter.instruction("str x0, [sp, #16]");                                   // save dirent pointer (will be clobbered)
    emitter.instruction("bl __rt_str_persist");                                 // copy string to heap, x1=new ptr, x2=len

    // -- push name string to array --
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload array pointer
    emitter.instruction("bl __rt_array_push_str");                              // push name to array
    emitter.instruction("str x0, [sp, #8]");                                    // update array pointer after possible realloc
    emitter.instruction("b __rt_scandir_loop");                                 // continue reading entries

    // -- close directory and return --
    emitter.label("__rt_scandir_close");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload DIR pointer
    emitter.bl_c("closedir");                                        // closedir(DIR*)

    // -- return array pointer --
    emitter.instruction("ldr x0, [sp, #8]");                                    // return array pointer

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
