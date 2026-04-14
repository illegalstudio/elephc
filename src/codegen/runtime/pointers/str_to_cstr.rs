use crate::codegen::{emit::Emitter, platform::Arch};

/// __rt_str_to_cstr: copy an elephc string into a freshly allocated C string.
/// Input:  x1 = pointer to string bytes, x2 = length
/// Output: x0 = pointer to heap-allocated null-terminated string
pub fn emit_str_to_cstr(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_to_cstr_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.raw("    .p2align 2"); // ensure 4-byte alignment for ARM64 instructions
    emitter.comment("--- runtime: str_to_cstr ---");
    emitter.label_global("__rt_str_to_cstr");

    // -- save return state before calling heap allocator --
    emitter.instruction("sub sp, sp, #32");                                     // allocate stack space for frame and saved arguments
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish a frame pointer for this helper

    // -- allocate len + 1 bytes for copied C string --
    emitter.instruction("str x1, [sp, #0]");                                    // preserve source pointer across heap allocation
    emitter.instruction("str x2, [sp, #8]");                                    // preserve source length across heap allocation
    emitter.instruction("add x0, x2, #1");                                      // requested allocation size = payload + trailing null
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate writable buffer on elephc heap
    emitter.instruction("ldr x1, [sp, #0]");                                    // restore source pointer after allocation
    emitter.instruction("ldr x2, [sp, #8]");                                    // restore source length after allocation

    // -- preserve destination and source state across the copy loop --
    emitter.instruction("mov x9, x0");                                          // keep destination pointer for the final return value
    emitter.instruction("mov x10, x1");                                         // copy source pointer into a scratch register
    emitter.instruction("mov x11, x2");                                         // copy remaining byte count into loop counter

    emitter.label("__rt_str_to_cstr_loop");
    emitter.instruction("cbz x11, __rt_str_to_cstr_done");                      // finish once every source byte has been copied
    emitter.instruction("ldrb w12, [x10], #1");                                 // load one source byte and advance the source pointer
    emitter.instruction("strb w12, [x0], #1");                                  // store one byte into the destination and advance it
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining byte count
    emitter.instruction("b __rt_str_to_cstr_loop");                             // continue copying bytes

    emitter.label("__rt_str_to_cstr_done");
    emitter.instruction("strb wzr, [x0]");                                      // append the trailing null terminator
    emitter.instruction("mov x0, x9");                                          // return the original destination pointer
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and caller return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate local stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_str_to_cstr_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_to_cstr ---");
    emitter.label_global("__rt_str_to_cstr");

    // -- preserve the elephc string payload across the heap allocation helper call --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame pointer for the saved source pointer and length
    emitter.instruction("sub rsp, 16");                                         // reserve local slots for the elephc source pointer and length
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the elephc source pointer across the heap allocation helper call
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the elephc source byte length across the heap allocation helper call

    // -- allocate len + 1 bytes for the dedicated null-terminated C string --
    emitter.instruction("mov rax, rdx");                                        // move the elephc string length into the x86_64 heap helper input register
    emitter.instruction("add rax, 1");                                          // request one extra byte for the trailing C null terminator
    emitter.instruction("call __rt_heap_alloc");                                // allocate writable storage for the foreign C ABI string copy
    emitter.instruction("mov r8, rax");                                         // preserve the destination base pointer for the return value
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the elephc source pointer after the allocator helper returns
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the elephc source byte length after the allocator helper returns

    // -- copy bytes into the dedicated C string buffer --
    emitter.label("__rt_str_to_cstr_loop");
    emitter.instruction("test rcx, rcx");                                       // stop copying once every elephc payload byte has been duplicated
    emitter.instruction("jz __rt_str_to_cstr_done");                            // append the trailing null terminator once the payload copy is complete
    emitter.instruction("mov r10b, BYTE PTR [r9]");                             // load one byte from the elephc string payload
    emitter.instruction("mov BYTE PTR [rax], r10b");                            // store the copied byte into the foreign C string buffer
    emitter.instruction("add r9, 1");                                           // advance the elephc source cursor after copying one byte
    emitter.instruction("add rax, 1");                                          // advance the C-string destination cursor after copying one byte
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining-byte counter
    emitter.instruction("jmp __rt_str_to_cstr_loop");                           // continue copying until the entire payload has been duplicated

    emitter.label("__rt_str_to_cstr_done");
    emitter.instruction("mov BYTE PTR [rax], 0");                               // append the trailing C null terminator after the copied bytes
    emitter.instruction("mov rax, r8");                                         // return the base pointer of the dedicated null-terminated C string
    emitter.instruction("add rsp, 16");                                         // release the temporary spill slots used by the helper
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the call-scoped C string pointer in rax
}
