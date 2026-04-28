use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// __rt_fileperms: get file permission bits via stat().
/// Input:  x1=path ptr, x2=path len
/// Output: x0=st_mode & 0xFFF (or 0 on error)
pub(crate) fn emit_fileperms(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_fileperms_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: fileperms ---");
    emitter.label_global("__rt_fileperms");

    // -- set up stack frame --
    // macOS ARM64 struct stat is ~128 bytes; allocate 144 + 16 = 160 bytes
    emitter.instruction("sub sp, sp, #160");                                    // allocate 160 bytes (144 for stat + 16 for frame)
    emitter.instruction("stp x29, x30, [sp, #144]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #144");                                   // set new frame pointer

    // -- null-terminate the path string --
    emitter.instruction("bl __rt_cstr");                                        // convert to C string → x0=null-terminated path

    // -- call libc stat() --
    emitter.instruction("mov x1, sp");                                          // x1 = pointer to stat buffer on stack
    emitter.bl_c("stat");                                                       // stat(path, &st_buf) → x0=0 on success, -1 on error

    // -- check for stat() error --
    emitter.instruction("cbnz x0, __rt_fileperms_error");                       // if stat() returned non-zero, return 0

    // -- extract st_mode and mask with 0xFFF (permission bits) --
    emitter.instruction("ldr w0, [sp, #4]");                                    // w0 = st_mode (offset 4 in macOS ARM64 struct stat)
    emitter.instruction("and w0, w0, #0xFFF");                                  // w0 = st_mode & 0xFFF (filesystem permission bits only)

    // -- tear down stack frame --
    emitter.instruction("ldp x29, x30, [sp, #144]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #160");                                    // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    // -- return 0 on stat() error --
    emitter.label("__rt_fileperms_error");
    emitter.instruction("mov x0, #0");                                          // return 0 on error (maps to false)
    emitter.instruction("ldp x29, x30, [sp, #144]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #160");                                    // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_fileperms_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fileperms ---");
    emitter.label_global("__rt_fileperms");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while the fileperms helper performs nested libc calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the x86_64 fileperms helper
    emitter.instruction("sub rsp, 160");                                        // reserve aligned stack storage for one struct stat plus scratch padding before the libc call

    abi::emit_call_label(emitter, "__rt_cstr");                                 // convert the path string result regs into a null-terminated C string in the scratch buffer
    emitter.instruction("mov rdi, rax");                                        // pass the null-terminated path in the SysV first-argument register
    emitter.instruction("lea rsi, [rsp]");                                      // pass the address of the local stat buffer as the SysV second argument
    emitter.bl_c("stat");                                                       // stat(path, &st_buf) → eax=0 on success, -1 on error

    emitter.instruction("test eax, eax");                                       // did stat() succeed?
    emitter.instruction("jne __rt_fileperms_error");                            // failures map to the zero PHP integer result

    emitter.instruction("mov eax, DWORD PTR [rsp + 8]");                         // load st_mode from the stat buffer (offset 8 on Linux x86_64)
    emitter.instruction("and eax, 0xFFF");                                      // mask the st_mode value to keep only the filesystem permission bits (07777)
    emitter.instruction("leave");                                               // release the local stat buffer and restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the st_mode permission bits to generated code

    emitter.label("__rt_fileperms_error");
    emitter.instruction("mov eax, 0");                                          // return 0 on stat() error (maps to false)
    emitter.instruction("leave");                                               // release the local stat buffer and restore the caller frame pointer before returning zero
    emitter.instruction("ret");                                                 // return to the caller with the zero (failure) result
}
