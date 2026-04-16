use crate::codegen::{emit::Emitter, platform::Arch};

/// file_put_contents: write data to a file (creating/truncating).
/// Input:  x1/x2=filename string, x3/x4=data string
/// Output: x0=bytes written
pub fn emit_file_put_contents(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_file_put_contents_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: file_put_contents ---");
    emitter.label_global("__rt_file_put_contents");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish new frame pointer

    // -- save data string for after cstr call --
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save data ptr and len on stack

    // -- null-terminate the filename --
    emitter.instruction("bl __rt_cstr");                                        // convert filename to C string, x0=cstr path
    emitter.instruction("str x0, [sp, #0]");                                    // save null-terminated path pointer

    // -- open file with write+create+truncate --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload null-terminated path
    emitter.instruction(&format!("mov x1, #0x{:X}", emitter.platform.o_wronly_creat_trunc())); // O_WRONLY|O_CREAT|O_TRUNC
    emitter.instruction("mov x2, #0x1A4");                                      // file mode 0644 (octal)
    emitter.syscall(5);
    emitter.instruction("str x0, [sp, #8]");                                    // save fd on stack

    // -- write data to file --
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload fd
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload data pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // reload data length
    emitter.syscall(4);
    emitter.instruction("str x0, [sp, #32]");                                   // save bytes written

    // -- close the file --
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload fd
    emitter.syscall(6);

    // -- return bytes written --
    emitter.instruction("ldr x0, [sp, #32]");                                   // return bytes written

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_file_put_contents_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: file_put_contents ---");
    emitter.label_global("__rt_file_put_contents");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while file_put_contents uses stack locals
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for saved pointers and lengths
    emitter.instruction("sub rsp, 48");                                         // reserve aligned stack space for data, path, fd, and byte-count temporaries

    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the data pointer while the filename is converted to a C string
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the data length while the filename is converted to a C string
    emitter.instruction("call __rt_cstr");                                      // convert the elephc filename in rax/rdx into a null-terminated C path in rax
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the C filename pointer for the later open() call

    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // pass the C filename pointer as the first libc open() argument
    emitter.instruction(&format!("mov rsi, 0x{:X}", emitter.platform.o_wronly_creat_trunc())); // pass O_WRONLY|O_CREAT|O_TRUNC as the open() flags
    emitter.instruction("mov rdx, 0x1A4");                                      // pass mode 0644 for newly created files
    emitter.instruction("call open");                                           // open the destination file for overwriting through libc open()
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the opened file descriptor for the later write() and close() calls

    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // pass the file descriptor as the first libc write() argument
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // pass the source data pointer as the second libc write() argument
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // pass the source data length as the third libc write() argument
    emitter.instruction("call write");                                          // write the requested bytes into the opened destination file
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the number of written bytes for the final return value

    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // pass the file descriptor as the first libc close() argument
    emitter.instruction("call close");                                          // close the destination file after the write completes

    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // return the number of bytes reported by libc write()
    emitter.instruction("add rsp, 48");                                         // release the aligned stack locals used by file_put_contents
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller with the write byte count in rax
}
