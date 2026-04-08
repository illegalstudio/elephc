use crate::codegen::emit::Emitter;

/// fgets: read one line from a file descriptor.
/// Input:  x0=fd
/// Output: x1=string pointer (in concat_buf), x2=string length (including \n if present)
/// Side effect: sets _eof_flags[fd] if EOF reached
pub fn emit_fgets(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fgets ---");
    emitter.label_global("__rt_fgets");

    // -- check for invalid fd (negative = fopen failed) --
    emitter.instruction("cmp x0, #0");                                          // check if fd is negative
    emitter.instruction("b.ge __rt_fgets_fd_ok");                               // if fd >= 0, proceed normally
    emitter.instruction("mov x1, #0");                                          // return empty string: null pointer
    emitter.instruction("mov x2, #0");                                          // return empty string: zero length
    emitter.instruction("ret");                                                 // return immediately to caller

    // -- set up stack frame --
    emitter.label("__rt_fgets_fd_ok");
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish new frame pointer

    // -- save fd and record starting position in concat_buf --
    emitter.instruction("str x0, [sp, #0]");                                    // save file descriptor on stack
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current write offset
    emitter.instruction("str x10, [sp, #8]");                                   // save start offset for calculating length later
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x12, x11, x10");                                   // compute write pointer: buf + offset
    emitter.instruction("str x12, [sp, #16]");                                  // save start pointer for return value

    // -- read loop: one byte at a time until \n or EOF --
    emitter.label("__rt_fgets_loop");
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current write offset
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x1, x11, x10");                                    // buf pointer for read syscall

    // -- read 1 byte via syscall --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload fd for read syscall
    emitter.instruction("mov x2, #1");                                          // read exactly 1 byte
    emitter.syscall(3);

    // -- check if read failed or returned 0 (EOF) --
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: check return value
        emitter.instruction("b.le __rt_fgets_eof");                             // if <= 0, error or EOF
    } else {
        emitter.instruction("b.cs __rt_fgets_eof");                             // macOS: if carry set, read syscall failed
        emitter.instruction("cbz x0, __rt_fgets_eof");                          // if 0 bytes read, we hit EOF
    }

    // -- advance concat_off by 1 --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    emitter.instruction("add x10, x10, #1");                                    // advance by 1 byte
    emitter.instruction("str x10, [x9]");                                       // store updated offset

    // -- check if the byte we just read is \n --
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("sub x13, x10, #1");                                    // offset of byte just read
    emitter.instruction("ldrb w14, [x11, x13]");                                // load the byte we just read
    emitter.instruction("cmp w14, #0x0A");                                      // compare with newline character
    emitter.instruction("b.eq __rt_fgets_done");                                // if newline, line is complete
    emitter.instruction("b __rt_fgets_loop");                                   // otherwise continue reading

    // -- EOF reached: set eof flag for this fd --
    emitter.label("__rt_fgets_eof");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload fd
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_eof_flags");
    emitter.instruction("mov w10, #1");                                         // eof marker value
    emitter.instruction("strb w10, [x9, x0]");                                  // set _eof_flags[fd] = 1

    // -- return result string --
    emitter.label("__rt_fgets_done");
    emitter.instruction("ldr x1, [sp, #16]");                                   // return string start pointer
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current offset (end position)
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload start offset
    emitter.instruction("sub x2, x10, x11");                                    // length = current offset - start offset

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
