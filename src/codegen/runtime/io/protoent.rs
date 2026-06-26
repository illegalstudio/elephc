//! Purpose:
//! Emits the `__rt_protoent_load` runtime helper, which reads the whole
//! `/etc/protocols` database into the shared `_protoent_buf` buffer.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//! - `__rt_getprotobyname` / `__rt_getprotobynumber` consume the buffer it fills.
//!
//! Key details:
//! - Returns the buffer base pointer and the byte count read; a zero count
//!   means the file was missing, which the scanners treat as "no entries".

use crate::codegen::{emit::Emitter, platform::Arch};
use crate::codegen::abi;

/// protoent_load: read `/etc/protocols` into `_protoent_buf`.
/// Input:  none
/// Output: AArch64 x0 = buffer pointer, x1 = byte count (0 on failure)
///         x86_64  rax = buffer pointer, rdx = byte count (0 on failure)
pub fn emit_protoent_load(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_protoent_load_linux_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;

    emitter.blank();
    emitter.comment("--- runtime: protoent_load (read /etc/protocols) ---");
    emitter.label_global("__rt_protoent_load");

    // -- set up stack frame --
    emitter.instruction("stp x29, x30, [sp, #-32]!");                           // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish a new frame pointer
    emitter.instruction("stp x19, x20, [sp, #16]");                             // save callee-saved registers for fd and total

    // -- open /etc/protocols read-only --
    abi::emit_symbol_address(emitter, "x0", "_etc_protocols_path");
    emitter.instruction("mov x1, #0");                                          // O_RDONLY = 0
    emitter.instruction("mov x2, #0");                                          // mode is unused for O_RDONLY
    emitter.syscall(5);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // compare the Linux open result against the success sentinel
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_protoent_load_opened")); // continue only when open succeeded
    emitter.instruction("b __rt_protoent_load_fail");                           // a missing file returns an empty buffer
    emitter.label("__rt_protoent_load_opened");
    emitter.instruction("mov x19, x0");                                         // x19 = open file descriptor
    emitter.instruction("mov x20, #0");                                         // x20 = running byte total

    // -- read the whole file into _protoent_buf --
    emitter.label("__rt_protoent_load_read");
    emitter.instruction("mov x2, #32768");                                      // total capacity of the protocols buffer
    emitter.instruction("sub x2, x2, x20");                                     // remaining capacity = capacity - total
    emitter.instruction("cbz x2, __rt_protoent_load_close");                    // stop reading once the buffer is full
    abi::emit_symbol_address(emitter, "x1", "_protoent_buf");
    emitter.instruction("add x1, x1, x20");                                     // write position = buffer base + total
    emitter.instruction("mov x0, x19");                                         // file descriptor for the read
    emitter.syscall(3);
    emitter.instruction("cmp x0, #0");                                          // did read reach end-of-file or fail?
    emitter.instruction("b.le __rt_protoent_load_close");                       // stop on end-of-file or a read error
    emitter.instruction("add x20, x20, x0");                                    // total += bytes read this chunk
    emitter.instruction("b __rt_protoent_load_read");                           // read the next chunk

    // -- close the file and return buffer/length --
    emitter.label("__rt_protoent_load_close");
    emitter.instruction("mov x0, x19");                                         // file descriptor to close
    emitter.syscall(6);
    abi::emit_symbol_address(emitter, "x0", "_protoent_buf");
    emitter.instruction("mov x1, x20");                                         // return the total byte count
    emitter.instruction("ldp x19, x20, [sp, #16]");                             // restore callee-saved registers
    emitter.instruction("ldp x29, x30, [sp], #32");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return the buffer pointer and length

    // -- failure path: return an empty buffer --
    emitter.label("__rt_protoent_load_fail");
    abi::emit_symbol_address(emitter, "x0", "_protoent_buf");
    emitter.instruction("mov x1, #0");                                          // a zero length signals an unreadable file
    emitter.instruction("ldp x19, x20, [sp, #16]");                             // restore callee-saved registers
    emitter.instruction("ldp x29, x30, [sp], #32");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return the empty buffer result
}

/// Emits the Linux x86_64 stream runtime helper for protoent load.
fn emit_protoent_load_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: protoent_load (read /etc/protocols) ---");
    emitter.label_global("__rt_protoent_load");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("push rbx");                                            // save the callee-saved register holding the fd
    emitter.instruction("push r12");                                            // save the callee-saved register holding the total

    // -- open /etc/protocols read-only --
    abi::emit_symbol_address(emitter, "rdi", "_etc_protocols_path");            // path argument for libc open()
    emitter.instruction("xor esi, esi");                                        // O_RDONLY flags for libc open()
    emitter.instruction("call open");                                           // open the protocols file for reading
    emitter.instruction("cmp eax, 0");                                          // did libc open() return a negative C int descriptor?
    emitter.instruction("jl __rt_protoent_load_fail");                          // a missing file returns an empty buffer
    emitter.instruction("cdqe");                                                // normalize the successful C int fd into the runtime's 64-bit descriptor value
    emitter.instruction("mov rbx, rax");                                        // rbx = open file descriptor
    emitter.instruction("xor r12d, r12d");                                      // r12 = running byte total

    // -- read the whole file into _protoent_buf --
    emitter.label("__rt_protoent_load_read");
    emitter.instruction("mov rdx, 32768");                                      // total capacity of the protocols buffer
    emitter.instruction("sub rdx, r12");                                        // remaining capacity = capacity - total
    emitter.instruction("jz __rt_protoent_load_close");                         // stop reading once the buffer is full
    emitter.instruction("mov rdi, rbx");                                        // file descriptor for the read
    abi::emit_symbol_address(emitter, "rsi", "_protoent_buf");                  // base of the protocols buffer
    emitter.instruction("add rsi, r12");                                        // write position = buffer base + total
    emitter.instruction("call read");                                           // read the next chunk from the file
    emitter.instruction("cmp rax, 0");                                          // did read reach end-of-file or fail?
    emitter.instruction("jle __rt_protoent_load_close");                        // stop on end-of-file or a read error
    emitter.instruction("add r12, rax");                                        // total += bytes read this chunk
    emitter.instruction("jmp __rt_protoent_load_read");                         // read the next chunk

    // -- close the file and return buffer/length --
    emitter.label("__rt_protoent_load_close");
    emitter.instruction("mov rdi, rbx");                                        // file descriptor to close
    emitter.instruction("call close");                                          // close the protocols file
    abi::emit_symbol_address(emitter, "rax", "_protoent_buf");                  // return the buffer base pointer
    emitter.instruction("mov rdx, r12");                                        // return the total byte count
    emitter.instruction("jmp __rt_protoent_load_done");                         // share the common epilogue

    // -- failure path: return an empty buffer --
    emitter.label("__rt_protoent_load_fail");
    abi::emit_symbol_address(emitter, "rax", "_protoent_buf");                  // return the buffer base pointer
    emitter.instruction("xor edx, edx");                                        // a zero length signals an unreadable file

    emitter.label("__rt_protoent_load_done");
    emitter.instruction("pop r12");                                             // restore the callee-saved register
    emitter.instruction("pop rbx");                                             // restore the callee-saved register
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the buffer pointer and length
}
