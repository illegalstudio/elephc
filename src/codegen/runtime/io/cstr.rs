use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// cstr: convert an elephc string (x1=ptr, x2=len) to a null-terminated C string.
/// Uses _cstr_buf (4096 bytes) as scratch space.
/// Input:  x1=ptr, x2=len
/// Output: x0=pointer to null-terminated string in _cstr_buf
///
/// cstr2: same but uses _cstr_buf2 for a second path (needed by rename/copy).
/// Input:  x1=ptr, x2=len
/// Output: x0=pointer to null-terminated string in _cstr_buf2
pub fn emit_cstr(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_cstr_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: cstr ---");
    emitter.label_global("__rt_cstr");

    // -- load destination buffer address --
    emitter.adrp("x9", "_cstr_buf");                             // load page address of cstr scratch buffer
    emitter.add_lo12("x9", "x9", "_cstr_buf");                       // resolve exact address of cstr buffer

    // -- copy bytes from source to buffer --
    emitter.instruction("mov x10, x9");                                         // save buffer start for return value
    emitter.instruction("mov x11, x2");                                         // copy length as loop counter
    emitter.label("__rt_cstr_loop");
    emitter.instruction("cbz x11, __rt_cstr_null");                             // if no bytes remain, append null terminator
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte from source, advance source ptr
    emitter.instruction("strb w12, [x9], #1");                                  // store byte to buffer, advance buffer ptr
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining byte count
    emitter.instruction("b __rt_cstr_loop");                                    // continue copying

    // -- append null terminator and return --
    emitter.label("__rt_cstr_null");
    emitter.instruction("strb wzr, [x9]");                                      // write null terminator after last byte
    emitter.instruction("mov x0, x10");                                         // return pointer to null-terminated string
    emitter.instruction("ret");                                                 // return to caller

    emitter.blank();
    emitter.comment("--- runtime: cstr2 ---");
    emitter.label_global("__rt_cstr2");

    // -- load second buffer address --
    emitter.adrp("x9", "_cstr_buf2");                            // load page address of second cstr buffer
    emitter.add_lo12("x9", "x9", "_cstr_buf2");                      // resolve exact address of second buffer

    // -- copy bytes from source to buffer --
    emitter.instruction("mov x10, x9");                                         // save buffer start for return value
    emitter.instruction("mov x11, x2");                                         // copy length as loop counter
    emitter.label("__rt_cstr2_loop");
    emitter.instruction("cbz x11, __rt_cstr2_null");                            // if no bytes remain, append null terminator
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte from source, advance source ptr
    emitter.instruction("strb w12, [x9], #1");                                  // store byte to buffer, advance buffer ptr
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining byte count
    emitter.instruction("b __rt_cstr2_loop");                                   // continue copying

    // -- append null terminator and return --
    emitter.label("__rt_cstr2_null");
    emitter.instruction("strb wzr, [x9]");                                      // write null terminator after last byte
    emitter.instruction("mov x0, x10");                                         // return pointer to null-terminated string
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_cstr_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: cstr ---");
    emitter.label_global("__rt_cstr");

    abi::emit_symbol_address(emitter, "r8", "_cstr_buf");
    emitter.instruction("mov r9, r8");                                          // preserve the start of the primary C-string scratch buffer for the return value
    emitter.instruction("mov r10, rax");                                        // copy the elephc source pointer into a dedicated source cursor
    emitter.instruction("mov rcx, rdx");                                        // copy the elephc source length into the loop counter
    emitter.label("__rt_cstr_loop");
    emitter.instruction("test rcx, rcx");                                       // stop copying once the full elephc string length has been consumed
    emitter.instruction("je __rt_cstr_null");                                   // append the null terminator once no bytes remain
    emitter.instruction("mov r11b, BYTE PTR [r10]");                            // load one byte from the elephc string payload
    emitter.instruction("mov BYTE PTR [r8], r11b");                             // store the byte into the primary C-string scratch buffer
    emitter.instruction("add r10, 1");                                          // advance the source cursor to the next elephc byte
    emitter.instruction("add r8, 1");                                           // advance the destination cursor to the next scratch byte
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining-byte counter
    emitter.instruction("jmp __rt_cstr_loop");                                  // continue copying until every byte has been moved

    emitter.label("__rt_cstr_null");
    emitter.instruction("mov BYTE PTR [r8], 0");                                // append the trailing C null terminator after the copied bytes
    emitter.instruction("mov rax, r9");                                         // return the start of the primary C-string scratch buffer
    emitter.instruction("ret");                                                 // return to the caller with a null-terminated path pointer

    emitter.blank();
    emitter.comment("--- runtime: cstr2 ---");
    emitter.label_global("__rt_cstr2");

    abi::emit_symbol_address(emitter, "r8", "_cstr_buf2");
    emitter.instruction("mov r9, r8");                                          // preserve the start of the secondary C-string scratch buffer for the return value
    emitter.instruction("mov r10, rax");                                        // copy the elephc source pointer into a dedicated source cursor
    emitter.instruction("mov rcx, rdx");                                        // copy the elephc source length into the loop counter
    emitter.label("__rt_cstr2_loop");
    emitter.instruction("test rcx, rcx");                                       // stop copying once the full elephc string length has been consumed
    emitter.instruction("je __rt_cstr2_null");                                  // append the null terminator once no bytes remain
    emitter.instruction("mov r11b, BYTE PTR [r10]");                            // load one byte from the elephc string payload
    emitter.instruction("mov BYTE PTR [r8], r11b");                             // store the byte into the secondary C-string scratch buffer
    emitter.instruction("add r10, 1");                                          // advance the source cursor to the next elephc byte
    emitter.instruction("add r8, 1");                                           // advance the destination cursor to the next scratch byte
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining-byte counter
    emitter.instruction("jmp __rt_cstr2_loop");                                 // continue copying until every byte has been moved

    emitter.label("__rt_cstr2_null");
    emitter.instruction("mov BYTE PTR [r8], 0");                                // append the trailing C null terminator after the copied bytes
    emitter.instruction("mov rax, r9");                                         // return the start of the secondary C-string scratch buffer
    emitter.instruction("ret");                                                 // return to the caller with a null-terminated path pointer
}
