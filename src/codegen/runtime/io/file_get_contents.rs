use crate::codegen::emit::Emitter;

/// file_get_contents: read an entire file into a string.
/// Input:  x1/x2=filename string
/// Output: x1=buffer pointer, x2=bytes read
pub fn emit_file_get_contents(emitter: &mut Emitter) {
    let plat = emitter.platform;
    let stat_buf = plat.stat_buf_size();
    // Layout: [0..8) path ptr, [8..16) file size, [16..16+stat_buf) stat buffer,
    //         then fd, heap ptr, bytes read, + saved frame regs
    let stat_base = 16;
    let st_size_abs = stat_base + plat.stat_size_offset();
    let locals_start = stat_base + stat_buf;
    let fd_off = locals_start;                // +0: fd
    let heap_off = locals_start + 8;          // +8: heap buffer ptr
    let bread_off = locals_start + 16;        // +16: bytes read
    let frame_size = (locals_start + 24 + 16 + 15) & !15; // +24 locals + 16 saved regs, aligned
    let save_offset = frame_size - 16;

    emitter.blank();
    emitter.comment("--- runtime: file_get_contents ---");
    emitter.label_global("__rt_file_get_contents");

    // -- set up stack frame --
    emitter.instruction(&format!("sub sp, sp, #{}", frame_size));               // allocate stack for stat buf + locals + frame
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", save_offset));      // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", save_offset));             // establish new frame pointer

    // -- null-terminate the filename --
    emitter.instruction("bl __rt_cstr");                                        // convert filename to C string, x0=cstr path
    emitter.instruction("str x0, [sp, #0]");                                    // save null-terminated path pointer

    // -- call stat64 to get file size --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload path for stat64
    emitter.instruction(&format!("add x1, sp, #{}", stat_base));                // pointer to stat buffer on stack
    emitter.syscall(338);

    // -- extract file size from stat struct --
    emitter.instruction(&format!("ldr x9, [sp, #{}]", st_size_abs));            // load st_size from stat struct
    emitter.instruction("str x9, [sp, #8]");                                    // save file size on stack

    // -- open the file for reading --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload null-terminated path
    emitter.instruction("mov x1, #0");                                          // O_RDONLY = 0
    emitter.instruction("mov x2, #0");                                          // mode not needed for O_RDONLY
    emitter.syscall(5);
    emitter.instruction(&format!("str x0, [sp, #{}]", fd_off));                 // save fd on stack

    // -- allocate heap buffer for file contents --
    emitter.instruction("ldr x0, [sp, #8]");                                    // load file size as allocation request
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate buffer, x0=pointer
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = persisted elephc string
    emitter.instruction("str x9, [x0, #-8]");                                   // store string kind in the uniform heap header
    emitter.instruction(&format!("str x0, [sp, #{}]", heap_off));               // save heap buffer pointer

    // -- read entire file into buffer --
    emitter.instruction(&format!("ldr x0, [sp, #{}]", fd_off));                 // reload fd
    emitter.instruction(&format!("ldr x1, [sp, #{}]", heap_off));               // buffer pointer for read
    emitter.instruction("ldr x2, [sp, #8]");                                    // file size = bytes to read
    emitter.syscall(3);
    emitter.instruction(&format!("str x0, [sp, #{}]", bread_off));              // save actual bytes read

    // -- close the file --
    emitter.instruction(&format!("ldr x0, [sp, #{}]", fd_off));                 // reload fd
    emitter.syscall(6);

    // -- return buffer pointer and bytes read --
    emitter.instruction(&format!("ldr x1, [sp, #{}]", heap_off));               // return heap buffer pointer
    emitter.instruction(&format!("ldr x2, [sp, #{}]", bread_off));              // return actual bytes read

    // -- restore frame and return --
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_offset));      // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));               // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
