use crate::codegen::{emit::Emitter, platform::Arch};

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// tempnam: create a temporary filename.
/// Input:  x1/x2=dir string, x3/x4=prefix string
/// Output: x1=temp filename pointer, x2=temp filename length
pub fn emit_tempnam(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_tempnam_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: tempnam ---");
    emitter.label_global("__rt_tempnam");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish new frame pointer

    // -- save inputs --
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save dir ptr and len
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save prefix ptr and len

    // -- build template path: dir + "/" + prefix + "XXXXXX" in _cstr_buf --
    emitter.adrp("x9", "_cstr_buf");                             // load page address of cstr buffer
    emitter.add_lo12("x9", "x9", "_cstr_buf");                       // resolve exact buffer address
    emitter.instruction("mov x10, x9");                                         // save buffer start

    // -- copy dir bytes --
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // reload dir ptr and len
    emitter.label("__rt_tempnam_dir");
    emitter.instruction("cbz x2, __rt_tempnam_slash");                          // if no bytes remain, add slash
    emitter.instruction("ldrb w11, [x1], #1");                                  // load byte from dir, advance ptr
    emitter.instruction("strb w11, [x9], #1");                                  // store byte to buffer, advance ptr
    emitter.instruction("sub x2, x2, #1");                                      // decrement counter
    emitter.instruction("b __rt_tempnam_dir");                                  // continue copying

    // -- append '/' separator --
    emitter.label("__rt_tempnam_slash");
    emitter.instruction("mov w11, #0x2F");                                      // '/' character
    emitter.instruction("strb w11, [x9], #1");                                  // append slash to buffer

    // -- copy prefix bytes --
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // reload prefix ptr and len
    emitter.label("__rt_tempnam_pfx");
    emitter.instruction("cbz x2, __rt_tempnam_xx");                             // if no bytes remain, add XXXXXX
    emitter.instruction("ldrb w11, [x1], #1");                                  // load byte from prefix, advance ptr
    emitter.instruction("strb w11, [x9], #1");                                  // store byte to buffer, advance ptr
    emitter.instruction("sub x2, x2, #1");                                      // decrement counter
    emitter.instruction("b __rt_tempnam_pfx");                                  // continue copying

    // -- append "XXXXXX" template suffix --
    emitter.label("__rt_tempnam_xx");
    emitter.instruction("mov w11, #0x58");                                      // 'X' character
    emitter.instruction("strb w11, [x9], #1");                                  // append X #1
    emitter.instruction("strb w11, [x9], #1");                                  // append X #2
    emitter.instruction("strb w11, [x9], #1");                                  // append X #3
    emitter.instruction("strb w11, [x9], #1");                                  // append X #4
    emitter.instruction("strb w11, [x9], #1");                                  // append X #5
    emitter.instruction("strb w11, [x9], #1");                                  // append X #6
    emitter.instruction("strb wzr, [x9]");                                      // null-terminate the template

    // -- call mkstemp to create the temp file (modifies XXXXXX in-place) --
    emitter.instruction("str x10, [sp, #32]");                                  // save buffer start (clobbered by mkstemp)
    emitter.instruction("mov x0, x10");                                         // pass template buffer to mkstemp
    emitter.bl_c("mkstemp");                                         // mkstemp(template), x0=fd

    // -- close the temp file (we only need the name) --
    emitter.syscall(6);

    // -- calculate length of resulting path --
    emitter.instruction("ldr x1, [sp, #32]");                                   // reload buffer start (was clobbered by mkstemp)
    emitter.instruction("mov x2, #0");                                          // initialize length counter
    emitter.label("__rt_tempnam_len");
    emitter.instruction("ldrb w11, [x1, x2]");                                  // load byte at current position
    emitter.instruction("cbz w11, __rt_tempnam_copy");                          // if null, done counting
    emitter.instruction("add x2, x2, #1");                                      // increment length
    emitter.instruction("b __rt_tempnam_len");                                  // continue scanning

    // -- copy result to concat_buf for safe return --
    emitter.label("__rt_tempnam_copy");
    emitter.instruction("bl __rt_str_persist");                                 // copy to heap, x1=new ptr, x2=len

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_tempnam_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: tempnam ---");
    emitter.label_global("__rt_tempnam");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while tempnam() uses path-component and template spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the preserved lengths, C strings, template pointer, and file descriptor
    emitter.instruction("sub rsp, 64");                                         // reserve aligned spill slots for the directory/prefix lengths, C strings, template pointer, and mkstemp() fd
    emitter.instruction("mov QWORD PTR [rbp - 8], rdx");                        // preserve the elephc directory string length across C-string conversion and template construction
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the elephc prefix string length across C-string conversion and template construction
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // preserve the elephc prefix string pointer across the directory C-string conversion helper call
    emitter.instruction("call __rt_cstr");                                      // convert the elephc directory string in rax/rdx into a null-terminated C path in rax
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the C directory path pointer across the prefix conversion and template-construction loop
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the elephc prefix string pointer into the primary x86_64 string argument register
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the elephc prefix string length into the primary x86_64 string length register
    emitter.instruction("call __rt_cstr2");                                     // convert the elephc prefix string into the secondary null-terminated C string buffer
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the C prefix string pointer across template construction and mkstemp()
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the directory string length before sizing the mutable mkstemp() template buffer
    emitter.instruction("add rax, QWORD PTR [rbp - 16]");                       // include the prefix string length in the mutable mkstemp() template buffer size
    emitter.instruction("add rax, 8");                                          // include '/', the six X template bytes, and the trailing null terminator in the mutable buffer size
    emitter.instruction("call __rt_heap_alloc");                                // allocate a mutable owned buffer for the mkstemp() template path
    emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 1)); // materialize the owned-string heap kind word with the x86_64 heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the template buffer as a persisted elephc string in the uniform heap header
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // preserve the mutable template buffer pointer across the mkstemp() and close() calls
    emitter.instruction("mov r8, rax");                                         // keep a running destination cursor while copying the directory and prefix components into the template
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // reload the C directory path pointer before copying its bytes into the template
    emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // reload the directory string length before copying the directory bytes into the template
    emitter.label("__rt_tempnam_dir_copy_x86");
    emitter.instruction("test rcx, rcx");                                       // stop copying once every directory byte has been materialized into the mutable template buffer
    emitter.instruction("jz __rt_tempnam_dir_done_x86");                        // continue into the slash separator once the directory component has been fully copied
    emitter.instruction("mov r10b, BYTE PTR [r9]");                             // load the next byte from the C directory path while constructing the mutable template
    emitter.instruction("mov BYTE PTR [r8], r10b");                             // store the copied directory byte into the mutable mkstemp() template buffer
    emitter.instruction("add r9, 1");                                           // advance the C directory path cursor after copying one directory byte
    emitter.instruction("add r8, 1");                                           // advance the mutable template cursor after copying one directory byte
    emitter.instruction("sub rcx, 1");                                          // decrement the number of remaining directory bytes left to copy
    emitter.instruction("jmp __rt_tempnam_dir_copy_x86");                       // continue copying until the directory component is fully materialized
    emitter.label("__rt_tempnam_dir_done_x86");
    emitter.instruction("mov BYTE PTR [r8], 0x2F");                             // append the '/' separator between the directory component and the prefix component
    emitter.instruction("add r8, 1");                                           // advance the mutable template cursor past the inserted directory separator
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the C prefix string pointer before copying its bytes into the mutable template
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the prefix string length before copying the prefix bytes into the template
    emitter.label("__rt_tempnam_prefix_copy_x86");
    emitter.instruction("test rcx, rcx");                                       // stop copying once every prefix byte has been materialized into the mutable template buffer
    emitter.instruction("jz __rt_tempnam_xs_x86");                              // continue into the XXXXXX suffix once the prefix component has been fully copied
    emitter.instruction("mov r10b, BYTE PTR [r9]");                             // load the next byte from the C prefix string while constructing the mutable template
    emitter.instruction("mov BYTE PTR [r8], r10b");                             // store the copied prefix byte into the mutable mkstemp() template buffer
    emitter.instruction("add r9, 1");                                           // advance the C prefix cursor after copying one prefix byte
    emitter.instruction("add r8, 1");                                           // advance the mutable template cursor after copying one prefix byte
    emitter.instruction("sub rcx, 1");                                          // decrement the number of remaining prefix bytes left to copy
    emitter.instruction("jmp __rt_tempnam_prefix_copy_x86");                    // continue copying until the prefix component is fully materialized
    emitter.label("__rt_tempnam_xs_x86");
    emitter.instruction("mov BYTE PTR [r8], 0x58");                             // append template X #1 into the mutable mkstemp() buffer
    emitter.instruction("mov BYTE PTR [r8 + 1], 0x58");                         // append template X #2 into the mutable mkstemp() buffer
    emitter.instruction("mov BYTE PTR [r8 + 2], 0x58");                         // append template X #3 into the mutable mkstemp() buffer
    emitter.instruction("mov BYTE PTR [r8 + 3], 0x58");                         // append template X #4 into the mutable mkstemp() buffer
    emitter.instruction("mov BYTE PTR [r8 + 4], 0x58");                         // append template X #5 into the mutable mkstemp() buffer
    emitter.instruction("mov BYTE PTR [r8 + 5], 0x58");                         // append template X #6 into the mutable mkstemp() buffer
    emitter.instruction("mov BYTE PTR [r8 + 6], 0");                            // append the trailing null terminator required by libc mkstemp()
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // pass the mutable template buffer to libc mkstemp(), which rewrites the trailing XXXXXX in place
    emitter.instruction("call mkstemp");                                        // create a unique temp file and rewrite the mutable template buffer into the final temp path
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // preserve the returned file descriptor so the temp file can be closed before returning the path
    emitter.instruction("cmp rax, 0");                                          // detect mkstemp() failure before trying to close the returned file descriptor
    emitter.instruction("jl __rt_tempnam_fail_x86");                            // release the allocated template buffer and return the empty string when mkstemp() fails
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // reload the mkstemp() file descriptor before closing the newly created temp file
    emitter.instruction("call close");                                          // close the temp file immediately because tempnam() returns only the path, not an open descriptor
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // return the owned mutable template buffer, now rewritten into the final temp path, in the x86_64 string result register
    emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");                        // rebuild the temp path length from the original directory string length
    emitter.instruction("add rdx, QWORD PTR [rbp - 16]");                       // include the original prefix string length in the returned temp path length
    emitter.instruction("add rdx, 7");                                          // include the inserted '/' plus the six rewritten mkstemp() suffix characters in the returned temp path length
    emitter.instruction("add rsp, 64");                                         // release the temporary tempnam() spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the owned temp path string
    emitter.instruction("ret");                                                 // return the owned temp path string in the canonical x86_64 string result registers

    emitter.label("__rt_tempnam_fail_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the allocated template buffer pointer so the failed mkstemp() path can release it safely
    emitter.instruction("call __rt_heap_free");                                 // release the allocated template buffer when libc mkstemp() fails to create a temp file
    emitter.instruction("xor eax, eax");                                        // return an empty string pointer when mkstemp() fails
    emitter.instruction("xor edx, edx");                                        // return an empty string length when mkstemp() fails
    emitter.instruction("add rsp, 64");                                         // release the temporary tempnam() spill slots on the failure path
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the empty string
    emitter.instruction("ret");                                                 // return the empty string result for the failed tempnam() helper
}
