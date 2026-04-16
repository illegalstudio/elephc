use crate::codegen::{emit::Emitter, platform::Arch};

/// concat: concatenate two strings.
/// Input:  x1=left_ptr, x2=left_len, x3=right_ptr, x4=right_len
/// Output: x1=result_ptr, x2=result_len
pub fn emit_concat(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_concat_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: concat ---");
    emitter.label_global("__rt_concat");

    // -- set up stack frame (64 bytes) --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish new frame pointer

    // -- save input arguments to stack --
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save left string ptr and length
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save right string ptr and length
    emitter.instruction("add x5, x2, x4");                                      // compute total result length
    emitter.instruction("str x5, [sp, #32]");                                   // save total length on stack

    // -- get concat_buf write position --
    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current write offset
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // compute destination pointer: buf + offset
    emitter.instruction("str x9, [sp, #40]");                                   // save result start pointer on stack

    // -- copy left string bytes --
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // reload left ptr and length
    emitter.instruction("mov x10, x9");                                         // set dest cursor to start of output
    emitter.label("__rt_concat_cl");
    emitter.instruction("cbz x2, __rt_concat_cr_setup");                        // if no bytes left, move to right string
    emitter.instruction("ldrb w11, [x1], #1");                                  // load byte from left string, advance src
    emitter.instruction("strb w11, [x10], #1");                                 // store byte to dest, advance dest
    emitter.instruction("sub x2, x2, #1");                                      // decrement remaining left bytes
    emitter.instruction("b __rt_concat_cl");                                    // continue copying left string

    // -- copy right string bytes --
    emitter.label("__rt_concat_cr_setup");
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // reload right ptr and length
    emitter.label("__rt_concat_cr");
    emitter.instruction("cbz x4, __rt_concat_done");                            // if no bytes left, concatenation complete
    emitter.instruction("ldrb w11, [x3], #1");                                  // load byte from right string, advance src
    emitter.instruction("strb w11, [x10], #1");                                 // store byte to dest, advance dest
    emitter.instruction("sub x4, x4, #1");                                      // decrement remaining right bytes
    emitter.instruction("b __rt_concat_cr");                                    // continue copying right string

    // -- update concat_buf offset and return result --
    emitter.label("__rt_concat_done");
    emitter.instruction("ldr x5, [sp, #32]");                                   // reload total result length
    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    emitter.instruction("add x8, x8, x5");                                      // advance offset by total length written
    emitter.instruction("str x8, [x6]");                                        // store updated offset

    // -- set return values and restore frame --
    emitter.instruction("ldr x1, [sp, #40]");                                   // return result pointer (start of output)
    emitter.instruction("ldr x2, [sp, #32]");                                   // return result length
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_concat_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: concat ---");
    emitter.label_global("__rt_concat");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while concat uses stack locals
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for concat bookkeeping
    emitter.instruction("sub rsp, 48");                                         // reserve local slots for input strings, total length, and result pointer

    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save left string pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save left string length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // save right string pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // save right string length
    emitter.instruction("mov r8, rdx");                                         // seed total length from the left string length
    emitter.instruction("add r8, rsi");                                         // total length = left length + right length
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // save total length for offset update and return

    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load current concat write offset
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("lea r10, [r10 + r9]");                                 // compute destination pointer: concat_buf + offset
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save result start pointer for return

    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // load left source pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // load remaining left byte count
    emitter.label("__rt_concat_cl");
    emitter.instruction("test r9, r9");                                         // check whether all left bytes have been copied
    emitter.instruction("je __rt_concat_cr_setup");                             // continue with the right string when left is exhausted
    emitter.instruction("mov r11b, BYTE PTR [r8]");                             // load one byte from the left string
    emitter.instruction("mov BYTE PTR [r10], r11b");                            // store the byte into the concat destination
    emitter.instruction("add r8, 1");                                           // advance the left source pointer
    emitter.instruction("add r10, 1");                                          // advance the concat destination pointer
    emitter.instruction("sub r9, 1");                                           // decrement remaining left bytes
    emitter.instruction("jmp __rt_concat_cl");                                  // continue copying left bytes

    emitter.label("__rt_concat_cr_setup");
    emitter.instruction("mov r8, QWORD PTR [rbp - 24]");                        // load right source pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // load remaining right byte count
    emitter.label("__rt_concat_cr");
    emitter.instruction("test r9, r9");                                         // check whether all right bytes have been copied
    emitter.instruction("je __rt_concat_done");                                 // finish once the right string is exhausted
    emitter.instruction("mov r11b, BYTE PTR [r8]");                             // load one byte from the right string
    emitter.instruction("mov BYTE PTR [r10], r11b");                            // store the byte into the concat destination
    emitter.instruction("add r8, 1");                                           // advance the right source pointer
    emitter.instruction("add r10, 1");                                          // advance the concat destination pointer
    emitter.instruction("sub r9, 1");                                           // decrement remaining right bytes
    emitter.instruction("jmp __rt_concat_cr");                                  // continue copying right bytes

    emitter.label("__rt_concat_done");
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // reload current concat write offset
    emitter.instruction("add r9, QWORD PTR [rbp - 40]");                        // advance offset by total bytes written
    emitter.instruction("mov QWORD PTR [r8], r9");                              // store updated concat write offset
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // return result pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // return result length
    emitter.instruction("add rsp, 48");                                         // release concat local slots
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return concatenated string in rax/rdx
}

#[cfg(test)]
mod tests {
    use crate::codegen::platform::{Arch, Platform, Target};

    use super::*;

    #[test]
    fn test_emit_concat_linux_x86_64_uses_native_copy_loop() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_concat(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("__rt_concat:\n"));
        assert!(asm.contains("mov QWORD PTR [rbp - 8], rax\n"));
        assert!(asm.contains("mov r11b, BYTE PTR [r8]\n"));
        assert!(asm.contains("mov rax, QWORD PTR [rbp - 48]\n"));
    }
}
