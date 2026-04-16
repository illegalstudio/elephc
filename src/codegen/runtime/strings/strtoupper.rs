use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// strtoupper: copy string to concat_buf, uppercasing a-z.
pub fn emit_strtoupper(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_strtoupper_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: strtoupper ---");
    emitter.label_global("__rt_strtoupper");

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

    // -- copy bytes, converting lowercase to uppercase --
    emitter.label("__rt_strtoupper_loop");
    emitter.instruction("cbz x11, __rt_strtoupper_done");                       // if no bytes remain, done
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte from source, advance ptr
    emitter.instruction("cmp w12, #97");                                        // compare with 'a' (0x61)
    emitter.instruction("b.lt __rt_strtoupper_store");                          // if below 'a', store unchanged
    emitter.instruction("cmp w12, #122");                                       // compare with 'z' (0x7A)
    emitter.instruction("b.gt __rt_strtoupper_store");                          // if above 'z', store unchanged
    emitter.instruction("sub w12, w12, #32");                                   // convert a-z to A-Z by subtracting 32
    emitter.label("__rt_strtoupper_store");
    emitter.instruction("strb w12, [x9], #1");                                  // store (possibly uppered) byte, advance dest
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining count
    emitter.instruction("b __rt_strtoupper_loop");                              // continue processing next byte

    // -- update concat_off and return --
    emitter.label("__rt_strtoupper_done");
    emitter.instruction("add x8, x8, x2");                                      // advance offset by string length
    emitter.instruction("str x8, [x6]");                                        // store updated offset to _concat_off
    emitter.instruction("mov x1, x10");                                         // return new pointer (start of uppered copy)

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_strtoupper_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strtoupper ---");
    emitter.label_global("__rt_strtoupper");

    // -- get concat_buf write position --
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the current concat-buffer write offset before copying the uppercased string bytes
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("lea r11, [r10 + r9]");                                 // compute the concat-buffer destination pointer for the uppercased string
    emitter.instruction("mov rcx, rdx");                                        // copy the source string length into the loop counter so the returned byte length remains unchanged
    emitter.instruction("mov rsi, rdx");                                        // preserve the original source string length for the final string result after the byte loop clobbers caller-saved registers
    emitter.instruction("mov r8, rax");                                         // preserve the source string pointer in a dedicated cursor register before the loop mutates caller-saved registers
    emitter.instruction("mov rax, r11");                                        // preserve the concat-buffer start pointer as the returned string pointer

    // -- copy bytes, converting lowercase to uppercase --
    emitter.label("__rt_strtoupper_loop_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // stop once every source byte has been copied into concat storage
    emitter.instruction("jz __rt_strtoupper_done_linux_x86_64");                // jump to finalization when the full source string length has been consumed
    emitter.instruction("mov dl, BYTE PTR [r8]");                               // load one source byte before deciding whether it lies in the lowercase ASCII range
    emitter.instruction("cmp dl, 97");                                          // compare the current source byte against 'a' to detect lowercase ASCII letters
    emitter.instruction("jb __rt_strtoupper_store_linux_x86_64");               // leave bytes below 'a' unchanged because they are not lowercase ASCII letters
    emitter.instruction("cmp dl, 122");                                         // compare the current source byte against 'z' to bound the lowercase ASCII range
    emitter.instruction("ja __rt_strtoupper_store_linux_x86_64");               // leave bytes above 'z' unchanged because they are not lowercase ASCII letters
    emitter.instruction("sub dl, 32");                                          // convert lowercase ASCII to uppercase by subtracting the alphabetic case delta

    emitter.label("__rt_strtoupper_store_linux_x86_64");
    emitter.instruction("mov BYTE PTR [r11], dl");                              // store the possibly uppercased byte into concat storage before advancing both cursors
    emitter.instruction("add r8, 1");                                           // advance the source string pointer after consuming one byte
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination pointer after storing one byte
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining byte count after copying one source byte
    emitter.instruction("jmp __rt_strtoupper_loop_linux_x86_64");               // continue processing the remaining source bytes until the string is exhausted

    // -- update concat_off and return --
    emitter.label("__rt_strtoupper_done_linux_x86_64");
    emitter.instruction("add r9, rsi");                                         // advance the concat-buffer write offset by the original string length that strtoupper() copied
    emitter.instruction("mov QWORD PTR [rip + _concat_off], r9");               // persist the updated concat-buffer write offset after materializing the uppercased string
    emitter.instruction("mov rdx, rsi");                                        // restore the original string length into the x86_64 string result length register before returning
    emitter.instruction("ret");                                                 // return the concat-backed uppercased string in the standard x86_64 string result registers
}
