use crate::codegen::emit::Emitter;

/// file_get_contents: read an entire file into a string.
/// Input:  x1/x2=filename string
/// Output: x1=buffer pointer, x2=bytes read
pub fn emit_file_get_contents(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: file_get_contents ---");
    emitter.label_global("__rt_file_get_contents");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #224");                                    // allocate 224 bytes (144 for stat + locals + frame)
    emitter.instruction("stp x29, x30, [sp, #208]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #208");                                   // establish new frame pointer

    // -- null-terminate the filename --
    emitter.instruction("bl __rt_cstr");                                        // convert filename to C string, x0=cstr path
    emitter.instruction("str x0, [sp, #0]");                                    // save null-terminated path pointer

    // -- call stat64 to get file size --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload path for stat64
    emitter.instruction("add x1, sp, #16");                                     // pointer to stat buffer (144 bytes at sp+16)
    emitter.instruction("mov x16, #338");                                       // syscall 338 = stat64
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel

    // -- extract file size from stat struct (st_size at offset 96) --
    emitter.instruction("ldr x9, [sp, #112]");                                  // load st_size (sp+16+96 = sp+112)
    emitter.instruction("str x9, [sp, #8]");                                    // save file size on stack

    // -- open the file for reading --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload null-terminated path
    emitter.instruction("mov x1, #0");                                          // O_RDONLY = 0
    emitter.instruction("mov x2, #0");                                          // mode not needed for O_RDONLY
    emitter.instruction("mov x16, #5");                                         // syscall 5 = open
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel
    emitter.instruction("str x0, [sp, #160]");                                  // save fd on stack

    // -- allocate heap buffer for file contents --
    emitter.instruction("ldr x0, [sp, #8]");                                    // load file size as allocation request
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate buffer, x0=pointer
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = persisted elephc string
    emitter.instruction("str x9, [x0, #-8]");                                   // store string kind in the uniform heap header
    emitter.instruction("str x0, [sp, #168]");                                  // save heap buffer pointer

    // -- read entire file into buffer --
    emitter.instruction("ldr x0, [sp, #160]");                                  // reload fd
    emitter.instruction("ldr x1, [sp, #168]");                                  // buffer pointer for read
    emitter.instruction("ldr x2, [sp, #8]");                                    // file size = bytes to read
    emitter.instruction("mov x16, #3");                                         // syscall 3 = read
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel
    emitter.instruction("str x0, [sp, #176]");                                  // save actual bytes read

    // -- close the file --
    emitter.instruction("ldr x0, [sp, #160]");                                  // reload fd
    emitter.instruction("mov x16, #6");                                         // syscall 6 = close
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel

    // -- return buffer pointer and bytes read --
    emitter.instruction("ldr x1, [sp, #168]");                                  // return heap buffer pointer
    emitter.instruction("ldr x2, [sp, #176]");                                  // return actual bytes read

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #208]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #224");                                    // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
