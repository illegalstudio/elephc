use crate::codegen::{emit::Emitter, platform::Arch};

/// ftoa: convert double-precision float to string.
/// Input:  d0 = float value
/// Output: x1 = pointer to string, x2 = length
/// Uses _snprintf with "%.14G" format.
/// On Apple ARM64 variadic ABI, the double goes on the stack.
pub fn emit_ftoa(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ftoa_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ftoa ---");
    emitter.label_global("__rt_ftoa");

    // -- set up stack frame (64 bytes) --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish new frame pointer

    // -- get current concat_buf position --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current write offset
    emitter.instruction("str x10, [sp, #32]");                                  // save original offset on stack
    emitter.instruction("str x9, [sp, #40]");                                   // save offset variable address on stack

    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x0, x11, x10");                                    // compute output buffer: concat_buf + offset
    emitter.instruction("str x0, [sp, #24]");                                   // save output buffer start on stack

    // -- call snprintf(buf, 32, "%.14G", double) --
    emitter.instruction("mov x1, #32");                                         // buffer size limit = 32 bytes
    emitter.adrp("x2", "_fmt_g");                                // load page address of format string "%.14G"
    emitter.add_lo12("x2", "x2", "_fmt_g");                          // resolve exact address of format string
    // -- Apple ARM64 variadic ABI: float arg goes on stack, not in SIMD reg --
    emitter.instruction("str d0, [sp]");                                        // push double onto stack for variadic call
    emitter.bl_c("snprintf");                                        // call snprintf; returns char count in x0

    // -- x0 = number of chars written --
    emitter.instruction("mov x2, x0");                                          // save string length as return value

    // -- update concat_off by chars written --
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload offset variable address
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload original offset
    emitter.instruction("add x10, x10, x2");                                    // new offset = original + chars written
    emitter.instruction("str x10, [x9]");                                       // store updated offset

    // -- set return pointer --
    emitter.instruction("ldr x1, [sp, #24]");                                   // return pointer to start of formatted string

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_ftoa_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ftoa ---");
    emitter.label_global("__rt_ftoa");

    emitter.instruction("push rbp");                                            // save the caller frame pointer before using stack locals
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the formatting helper
    emitter.instruction("sub rsp, 32");                                         // reserve aligned scratch space for concat offsets and the output pointer

    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the current concat cursor so formatted bytes append after prior output
    emitter.instruction("mov QWORD PTR [rbp - 8], r9");                         // save the original concat cursor for the final offset update
    emitter.instruction("mov QWORD PTR [rbp - 16], r8");                        // save the concat cursor symbol address for the final store

    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("lea rdi, [r10 + r9]");                                 // compute the destination buffer inside the concat scratch area
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // preserve the destination pointer for the return value

    emitter.instruction("mov esi, 32");                                         // cap float formatting to the same 32-byte scratch window used on AArch64
    crate::codegen::abi::emit_symbol_address(emitter, "rdx", "_fmt_g");
    emitter.instruction("mov eax, 1");                                          // SysV variadic ABI: one SIMD register is live for the double argument
    emitter.instruction("call snprintf");                                       // format xmm0 using "%.14G" into the concat scratch buffer

    emitter.instruction("mov rdx, rax");                                        // return the formatted byte count in the string-length result register
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the concat cursor symbol address
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the original concat cursor
    emitter.instruction("add r9, rdx");                                         // advance the concat cursor by the number of formatted bytes
    emitter.instruction("mov QWORD PTR [r8], r9");                              // publish the updated concat cursor for subsequent string writes
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the pointer to the formatted float text

    emitter.instruction("add rsp, 32");                                         // release the local scratch area before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return pointer+length in rax/rdx
}

#[cfg(test)]
mod tests {
    use crate::codegen::platform::{Arch, Platform, Target};

    use super::*;

    #[test]
    fn test_emit_ftoa_linux_x86_64_uses_sysv_variadic_call() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_ftoa(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("__rt_ftoa:\n"));
        assert!(asm.contains("mov eax, 1\n"));
        assert!(asm.contains("call snprintf\n"));
        assert!(asm.contains("mov rdx, rax\n"));
    }
}
