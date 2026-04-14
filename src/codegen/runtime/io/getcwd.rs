use crate::codegen::{emit::Emitter, platform::Arch};

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// getcwd: get the current working directory.
/// Input:  none
/// Output: x1=string pointer, x2=string length
pub fn emit_getcwd(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_getcwd_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: getcwd ---");
    emitter.label_global("__rt_getcwd");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #32");                                     // allocate 32 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish new frame pointer

    // -- allocate heap buffer for path --
    emitter.instruction("mov x0, #1024");                                       // request 1024 bytes for path buffer
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate, x0=buffer pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save buffer pointer on stack

    // -- call libc getcwd --
    emitter.instruction("mov x1, #1024");                                       // buffer size
    emitter.bl_c("getcwd");                                          // getcwd(buf, size), x0=buf on success

    // -- calculate string length by scanning for null --
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload buffer pointer as string start
    emitter.instruction("mov x2, #0");                                          // initialize length counter
    emitter.label("__rt_getcwd_len");
    emitter.instruction("ldrb w9, [x1, x2]");                                   // load byte at current position
    emitter.instruction("cbz w9, __rt_getcwd_done");                            // if null terminator, length is complete
    emitter.instruction("add x2, x2, #1");                                      // increment length counter
    emitter.instruction("b __rt_getcwd_len");                                   // continue scanning

    // -- return string pointer and length --
    emitter.label("__rt_getcwd_done");

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_getcwd_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: getcwd ---");
    emitter.label_global("__rt_getcwd");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while getcwd uses local spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the owned buffer pointer and recovered length
    emitter.instruction("sub rsp, 16");                                         // reserve aligned spill slots for the owned buffer pointer and the scanned string length

    emitter.instruction("mov rax, 1024");                                       // request a fixed 1024-byte owned buffer for the current working directory path
    emitter.instruction("call __rt_heap_alloc");                                // allocate owned heap storage for the getcwd() destination buffer
    emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 1)); // materialize the owned-string heap kind word with the x86_64 heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the allocated buffer as a persisted elephc string in the uniform heap header
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the owned buffer pointer across the libc getcwd() and length-scan calls

    emitter.instruction("mov rdi, rax");                                        // pass the owned buffer pointer as the first libc getcwd() argument
    emitter.instruction("mov rsi, 1024");                                       // pass the fixed buffer capacity as the second libc getcwd() argument
    emitter.instruction("call getcwd");                                         // fill the owned buffer with the current working directory through libc getcwd()
    emitter.instruction("test rax, rax");                                       // detect libc getcwd() failure before scanning for the trailing null terminator
    emitter.instruction("jz __rt_getcwd_fail");                                 // return the empty string when libc getcwd() fails

    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the owned buffer pointer before scanning for the trailing C null terminator
    emitter.instruction("xor rdx, rdx");                                        // start the returned string length at zero before scanning the owned buffer
    emitter.label("__rt_getcwd_len");
    emitter.instruction("mov r9b, BYTE PTR [r8 + rdx]");                        // load the next byte from the owned getcwd() buffer while measuring its elephc string length
    emitter.instruction("test r9b, r9b");                                       // stop scanning once the trailing C null terminator is reached
    emitter.instruction("jz __rt_getcwd_done");                                 // finish once the owned buffer has been measured through the first null byte
    emitter.instruction("add rdx, 1");                                          // advance the elephc string length after consuming one non-null path byte
    emitter.instruction("jmp __rt_getcwd_len");                                 // continue scanning the owned buffer until the path length is fully measured

    emitter.label("__rt_getcwd_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the owned buffer pointer in the x86_64 string result register
    emitter.instruction("add rsp, 16");                                         // release the temporary spill slots used by the getcwd() helper
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the owned path string
    emitter.instruction("ret");                                                 // return the current working directory as an owned elephc string

    emitter.label("__rt_getcwd_fail");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the owned buffer pointer so the failed helper path can release it safely
    emitter.instruction("call __rt_heap_free");                                 // release the unused owned buffer when libc getcwd() reports failure
    emitter.instruction("xor eax, eax");                                        // return an empty string pointer when the working directory cannot be queried
    emitter.instruction("xor edx, edx");                                        // return an empty string length when the working directory cannot be queried
    emitter.instruction("add rsp, 16");                                         // release the temporary spill slots used by the failed getcwd() path
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the empty string
    emitter.instruction("ret");                                                 // return the empty string result for the failed getcwd() query
}
