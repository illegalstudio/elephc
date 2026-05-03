use crate::codegen::{emit::Emitter, platform::Arch};

/// fnmatch: shell-glob match between a pattern and a filename.
///
/// AArch64 input: x1/x2 = pattern, x3/x4 = filename, x5 = flags
/// AArch64 output: x0 = 1 on match, 0 otherwise
///
/// x86_64 input: rax/rdx = pattern, rdi/rsi = filename, rcx = flags
/// x86_64 output: rax = 1 on match, 0 otherwise
///
/// Flag values are target-libc values. The codegen constant prescan emits the
/// same platform-specific numbers PHP exposes on the selected target.
pub fn emit_fnmatch(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_fnmatch_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment after preceding runtime literals
    emitter.comment("--- runtime: fnmatch ---");
    emitter.label_global("__rt_fnmatch");

    // Frame layout:
    //   sp+ 0  : filename ptr
    //   sp+ 8  : filename len
    //   sp+16  : flags
    //   sp+24  : C pattern ptr
    //   sp+32  : x29 / x30
    emitter.instruction("sub sp, sp, #48");                                     // reserve aligned spill space for the two C-string conversions
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller frame while calling helpers and libc
    emitter.instruction("add x29, sp, #32");                                    // establish a stable frame pointer for the wrapper
    emitter.instruction("str x3, [sp, #0]");                                    // save filename pointer while converting the pattern
    emitter.instruction("str x4, [sp, #8]");                                    // save filename length while converting the pattern
    emitter.instruction("str x5, [sp, #16]");                                   // save PHP fnmatch flags while converting both strings

    emitter.instruction("bl __rt_cstr");                                        // convert the pattern to the primary null-terminated scratch buffer
    emitter.instruction("str x0, [sp, #24]");                                   // preserve the C pattern pointer while converting the filename

    emitter.instruction("ldr x1, [sp, #0]");                                    // reload filename pointer for the secondary C-string conversion
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload filename length for the secondary C-string conversion
    emitter.instruction("bl __rt_cstr2");                                       // convert the filename to the secondary null-terminated scratch buffer

    emitter.instruction("mov x1, x0");                                          // second libc argument: C filename pointer
    emitter.instruction("ldr x0, [sp, #24]");                                   // first libc argument: C pattern pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // third libc argument: PHP/POSIX fnmatch flags
    emitter.bl_c("fnmatch");                                                    // libc fnmatch(pattern, filename, flags) returns 0 on match
    emitter.instruction("cmp x0, #0");                                          // PHP expects true when libc reports an exact match
    emitter.instruction("cset x0, eq");                                         // normalize libc's 0/non-zero status to bool 1/0

    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the fnmatch wrapper frame
    emitter.instruction("ret");                                                 // return the boolean match result
}

fn emit_fnmatch_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fnmatch ---");
    emitter.label_global("__rt_fnmatch");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while calling helpers and libc
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame pointer for local spill slots
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill space for filename, flags, and C pattern
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save filename pointer while converting the pattern
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save filename length while converting the pattern
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save PHP fnmatch flags while converting both strings

    emitter.instruction("call __rt_cstr");                                      // convert the pattern to the primary null-terminated scratch buffer
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the C pattern pointer while converting the filename

    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload filename pointer for the secondary C-string conversion
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload filename length for the secondary C-string conversion
    emitter.instruction("call __rt_cstr2");                                     // convert the filename to the secondary null-terminated scratch buffer

    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // first libc argument: C pattern pointer
    emitter.instruction("mov rsi, rax");                                        // second libc argument: C filename pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // third libc argument: PHP/POSIX fnmatch flags
    emitter.bl_c("fnmatch");                                                    // libc fnmatch(pattern, filename, flags) returns 0 on match
    emitter.instruction("cmp eax, 0");                                          // PHP expects true when libc reports an exact match
    emitter.instruction("sete al");                                             // set the low byte when libc returned success
    emitter.instruction("movzx eax, al");                                       // widen the boolean result to the standard integer result register

    emitter.instruction("add rsp, 32");                                         // release local spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the boolean match result
}
