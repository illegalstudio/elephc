use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// strtolower: copy string to concat_buf, lowercasing A-Z.
/// Input:  x1=ptr, x2=len
/// Output: x1=new_ptr, x2=len
pub fn emit_strtolower(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_strtolower_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: strtolower ---");
    emitter.label_global("__rt_strtolower");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #16");                                     // allocate 16 bytes on the stack
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer

    // -- get concat_buf write position --
    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current write offset
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // compute destination pointer
    emitter.instruction("mov x10, x9");                                         // save destination start for return value
    emitter.instruction("mov x11, x2");                                         // copy length as loop counter

    // -- copy bytes, converting uppercase to lowercase --
    emitter.label("__rt_strtolower_loop");
    emitter.instruction("cbz x11, __rt_strtolower_done");                       // if no bytes remain, done
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte from source, advance ptr
    emitter.instruction("cmp w12, #65");                                        // compare with 'A' (0x41)
    emitter.instruction("b.lt __rt_strtolower_store");                          // if below 'A', store unchanged
    emitter.instruction("cmp w12, #90");                                        // compare with 'Z' (0x5A)
    emitter.instruction("b.gt __rt_strtolower_store");                          // if above 'Z', store unchanged
    emitter.instruction("add w12, w12, #32");                                   // convert A-Z to a-z by adding 32
    emitter.label("__rt_strtolower_store");
    emitter.instruction("strb w12, [x9], #1");                                  // store (possibly lowered) byte, advance dest
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining count
    emitter.instruction("b __rt_strtolower_loop");                              // continue processing next byte

    // -- update concat_off and return --
    emitter.label("__rt_strtolower_done");
    emitter.instruction("add x8, x8, x2");                                      // advance offset by string length
    emitter.instruction("str x8, [x6]");                                        // store updated offset to _concat_off
    emitter.instruction("mov x1, x10");                                         // return new pointer (start of lowered copy)

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_strtolower_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strtolower ---");
    emitter.label_global("__rt_strtolower");
    emitter.instruction("call __rt_strcopy");                                   // copy the input string into concat-backed owned storage before lowercasing bytes in place
    emitter.instruction("test rdx, rdx");                                       // skip the bytewise lowercase loop when strtolower() receives an empty string
    emitter.instruction("jz __rt_strtolower_done_linux_x86_64");                // return immediately when there are no bytes to lowercase
    emitter.instruction("mov r8, rax");                                         // seed the mutable string cursor with the concat-backed copy returned by strcopy()
    emitter.instruction("mov rcx, rdx");                                        // seed the remaining-length counter from the copied string length

    emitter.label("__rt_strtolower_loop_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // have all bytes in the copied string been processed?
    emitter.instruction("jz __rt_strtolower_done_linux_x86_64");                // finish once the full copied string has been classified
    emitter.instruction("movzx r9d, BYTE PTR [r8]");                            // load the current byte from the mutable concat-backed copy before applying ASCII lowercase rules
    emitter.instruction("cmp r9b, 65");                                         // is the current byte below uppercase ASCII 'A'?
    emitter.instruction("jb __rt_strtolower_next_linux_x86_64");                // leave bytes below 'A' unchanged
    emitter.instruction("cmp r9b, 90");                                         // is the current byte above uppercase ASCII 'Z'?
    emitter.instruction("ja __rt_strtolower_next_linux_x86_64");                // leave bytes above 'Z' unchanged
    emitter.instruction("add r9b, 32");                                         // lowercase uppercase ASCII bytes by adding the standard case delta
    emitter.instruction("mov BYTE PTR [r8], r9b");                              // store the lowercased ASCII byte back into the mutable concat-backed copy

    emitter.label("__rt_strtolower_next_linux_x86_64");
    emitter.instruction("add r8, 1");                                           // advance the mutable string cursor after classifying one byte
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining byte count after processing one byte
    emitter.instruction("jmp __rt_strtolower_loop_linux_x86_64");               // continue lowercasing bytes until the full copied string has been processed

    emitter.label("__rt_strtolower_done_linux_x86_64");
    emitter.instruction("ret");                                                 // return the lowercased concat-backed string in the standard x86_64 string result registers
}
