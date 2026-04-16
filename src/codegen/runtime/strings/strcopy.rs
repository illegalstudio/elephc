use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// strcopy: copy a string to concat_buf (for in-place modification).
/// Input:  x1=ptr, x2=len
/// Output: x1=new_ptr (in concat_buf), x2=len (unchanged)
pub fn emit_strcopy(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_strcopy_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: strcopy ---");
    emitter.label_global("__rt_strcopy");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #16");                                     // allocate 16 bytes on the stack
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer

    // -- get concat_buf write position --
    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current write offset into concat_buf
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // compute destination: buf + offset

    // -- copy bytes from source to concat_buf --
    emitter.instruction("mov x10, x9");                                         // save destination start pointer
    emitter.instruction("mov x11, x2");                                         // copy length as loop counter
    emitter.label("__rt_strcopy_loop");
    emitter.instruction("cbz x11, __rt_strcopy_done");                          // if no bytes remain, done copying
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte from source, advance source ptr
    emitter.instruction("strb w12, [x9], #1");                                  // store byte to dest, advance dest ptr
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining byte count
    emitter.instruction("b __rt_strcopy_loop");                                 // continue copying

    // -- update concat_off and return new pointer --
    emitter.label("__rt_strcopy_done");
    emitter.instruction("add x8, x8, x2");                                      // advance offset by bytes copied
    emitter.instruction("str x8, [x6]");                                        // store updated offset to _concat_off
    emitter.instruction("mov x1, x10");                                         // return new pointer (start of copy)
    // x2 unchanged

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_strcopy_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strcopy ---");
    emitter.label_global("__rt_strcopy");

    // -- get concat_buf write position --
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the current concat-buffer write offset before cloning the input string into mutable storage
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("lea r11, [r10 + r9]");                                 // compute the concat-buffer destination pointer where the copied string starts
    emitter.instruction("mov rcx, rdx");                                        // copy the source string length into the loop counter so the return length survives the byte-copy loop
    emitter.instruction("mov rsi, rdx");                                        // preserve the original source string length for the returned string result after the loop clobbers caller-saved registers
    emitter.instruction("mov r8, rax");                                         // preserve the source string pointer in a dedicated cursor register before the copy loop mutates caller-saved registers
    emitter.instruction("mov rax, r11");                                        // preserve the concat-buffer destination start as the returned string pointer

    // -- copy bytes from source to concat_buf --
    emitter.label("__rt_strcopy_loop_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // stop once every source byte has been copied into concat storage
    emitter.instruction("jz __rt_strcopy_done_linux_x86_64");                   // finish once the full source string length has been consumed
    emitter.instruction("mov dl, BYTE PTR [r8]");                               // load one source byte before appending it to the concat-buffer destination cursor
    emitter.instruction("mov BYTE PTR [r11], dl");                              // store one copied byte into concat storage before advancing both cursors
    emitter.instruction("add r8, 1");                                           // advance the source string cursor after copying one byte
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer destination cursor after storing one byte
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining byte count after copying one source byte
    emitter.instruction("jmp __rt_strcopy_loop_linux_x86_64");                  // continue copying bytes until the full source string has been cloned

    // -- update concat_off and return new pointer --
    emitter.label("__rt_strcopy_done_linux_x86_64");
    emitter.instruction("add r9, rsi");                                         // advance the concat-buffer write offset by the original string length that strcopy() cloned
    emitter.instruction("mov QWORD PTR [rip + _concat_off], r9");               // persist the updated concat-buffer write offset after producing the copied mutable string
    emitter.instruction("mov rdx, rsi");                                        // restore the original source string length into the x86_64 string result length register before returning
    emitter.instruction("ret");                                                 // return the concat-backed copied string in the standard x86_64 string result registers
}
