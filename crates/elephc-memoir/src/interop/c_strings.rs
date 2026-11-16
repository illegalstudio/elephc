//! Purpose:
//! Emits independent owned storage when copying to and from C strings.
//!
//! Called from:
//! - Compiler pointer interoperability adapters.
//!
//! Key details:
//! - C strings receive a trailing NUL; native strings retain a pointer/length result.
//! - A null C input produces the existing empty-string result without acquiring an owner.

use crate::{Arch, Emitter};

/// __rt_str_to_cstr: copy an elephc string into a freshly allocated C string.
/// Input:  x1 = pointer to string bytes, x2 = length
/// Output: x0 = pointer to heap-allocated null-terminated string
pub fn emit_str_to_cstr(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::X86_64 {
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

/// Emits the x86_64 Linux variant of the `__rt_str_to_cstr` runtime helper.
/// Saves the source pointer and length across the heap allocation call, copies the
/// payload byte-by-byte, appends a trailing null, and returns the destination pointer in `rax`.
fn emit_str_to_cstr_linux_x86_64(emitter: &mut impl Emitter) {
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

/// __rt_cstr_to_str: convert a null-terminated C string to an owned elephc string.
/// Input:  x0 = pointer to null-terminated C string
/// Output: x1 = heap-allocated string pointer, x2 = computed length
pub fn emit_cstr_to_str(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::X86_64 {
        emit_cstr_to_str_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.raw("    .p2align 2"); // ensure 4-byte alignment for ARM64 instructions
    emitter.comment("--- runtime: cstr_to_str ---");
    emitter.label_global("__rt_cstr_to_str");

    // -- handle null pointer --
    emitter.instruction("cbz x0, __rt_cstr_to_str_null");                       // null pointer → empty string

    // -- preserve source pointer and scan for null terminator --
    emitter.instruction("mov x9, x0");                                          // preserve original C string pointer for the copy pass
    emitter.instruction("mov x2, #0");                                          // length counter = 0

    emitter.label("__rt_cstr_to_str_loop");
    emitter.instruction("ldrb w3, [x9, x2]");                                   // load byte at offset x2 from the C string
    emitter.instruction("cbz w3, __rt_cstr_to_str_done");                       // null terminator found
    emitter.instruction("add x2, x2, #1");                                      // increment length
    emitter.instruction("b __rt_cstr_to_str_loop");                             // continue scanning

    emitter.label("__rt_cstr_to_str_null");
    emitter.instruction("mov x1, #0");                                          // null pointer → empty string pointer
    emitter.instruction("mov x2, #0");                                          // null pointer → zero length
    emitter.instruction("ret");                                                 // return empty string

    emitter.label("__rt_cstr_to_str_done");
    // -- allocate and copy owned elephc string bytes --
    emitter.instruction("sub sp, sp, #32");                                     // allocate stack space for frame and saved metadata
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address before nested call
    emitter.instruction("add x29, sp, #16");                                    // establish a frame pointer for this helper
    emitter.instruction("str x9, [sp, #0]");                                    // preserve source pointer across heap allocation
    emitter.instruction("str x2, [sp, #8]");                                    // preserve computed length across heap allocation
    emitter.instruction("mov x0, x2");                                          // allocation size = computed string length
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate owned elephc string storage
    emitter.instruction("mov x3, #1");                                          // heap kind 1 = persisted elephc string
    emitter.instruction("str x3, [x0, #-8]");                                   // store string kind in the uniform heap header
    emitter.instruction("ldr x9, [sp, #0]");                                    // restore source pointer after allocation
    emitter.instruction("ldr x2, [sp, #8]");                                    // restore computed length after allocation
    emitter.instruction("mov x1, x0");                                          // x1 = result pointer for elephc string
    emitter.instruction("mov x10, x1");                                         // keep destination pointer for the byte copy loop
    emitter.instruction("mov x11, x9");                                         // keep source pointer for the byte copy loop
    emitter.instruction("mov x12, x2");                                         // copy remaining length into loop counter

    emitter.label("__rt_cstr_to_str_copy_loop");
    emitter.instruction("cbz x12, __rt_cstr_to_str_copy_done");                 // stop copying once all bytes are moved
    emitter.instruction("ldrb w3, [x11], #1");                                  // load one byte from the C string and advance source
    emitter.instruction("strb w3, [x10], #1");                                  // store one byte into the owned elephc buffer
    emitter.instruction("sub x12, x12, #1");                                    // decrement remaining byte count
    emitter.instruction("b __rt_cstr_to_str_copy_loop");                        // continue copying

    emitter.label("__rt_cstr_to_str_copy_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate helper stack frame
    emitter.instruction("ret");                                                 // return x1=ptr, x2=len
}

/// Emits x86_64 Linux assembly for `__rt_cstr_to_str`: null-terminated C string to owned elefphc string.
/// Input:  rax = pointer to null-terminated C string
/// Output: rax = heap-allocated string pointer, rdx = computed length
/// Saves and restores rbp; uses 16 bytes of spill stack space.
fn emit_cstr_to_str_linux_x86_64(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: cstr_to_str ---");
    emitter.label_global("__rt_cstr_to_str");

    // -- handle null pointers as empty elephc strings --
    emitter.instruction("test rax, rax");                                       // check whether the incoming foreign C string pointer is null
    emitter.instruction("jz __rt_cstr_to_str_null");                            // null pointers map to the empty elephc string convention

    // -- scan for the trailing C null terminator to compute the string length --
    emitter.instruction("mov r8, rax");                                         // preserve the original C string pointer for the later copy pass
    emitter.instruction("xor rdx, rdx");                                        // initialize the computed elephc string length to zero
    emitter.label("__rt_cstr_to_str_loop");
    emitter.instruction("cmp BYTE PTR [r8 + rdx], 0");                          // stop scanning once the trailing C null terminator is reached
    emitter.instruction("je __rt_cstr_to_str_done");                            // fall through to the allocation step once the byte length is known
    emitter.instruction("add rdx, 1");                                          // increment the computed elephc string length by one byte
    emitter.instruction("jmp __rt_cstr_to_str_loop");                           // continue scanning until the C string terminator is found

    emitter.label("__rt_cstr_to_str_null");
    emitter.instruction("mov rax, 0");                                          // null pointers map to the empty elephc string pointer
    emitter.instruction("mov rdx, 0");                                          // null pointers map to the empty elephc string length
    emitter.instruction("ret");                                                 // return the empty elephc string convention to the caller

    emitter.label("__rt_cstr_to_str_done");
    // -- preserve source metadata while the helper allocates owned elephc storage --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame pointer for the saved source pointer and length
    emitter.instruction("sub rsp, 16");                                         // reserve local slots for the source pointer and computed byte length
    emitter.instruction("mov QWORD PTR [rbp - 8], r8");                         // save the original foreign C string pointer across the heap allocation helper call
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the computed elephc string byte length across the heap allocation helper call
    emitter.instruction("mov rax, rdx");                                        // move the computed byte length into the x86_64 heap helper input register
    emitter.instruction("call __rt_heap_alloc");                                // allocate owned elephc string storage and return the payload pointer in rax
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::layout::x86_64_heap_kind_word(1))); // materialize the owned-string heap kind word with the x86_64 heap magic marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the allocated payload as a persisted elephc string in the uniform heap header
    emitter.instruction("mov r8, rax");                                         // preserve the destination payload pointer for the byte-copy loop and final return value
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the source C string pointer after the allocator helper returns
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the computed byte length after the allocator helper returns

    // -- copy bytes from the foreign C string into owned elephc storage --
    emitter.label("__rt_cstr_to_str_copy_loop");
    emitter.instruction("test rcx, rcx");                                       // stop copying once every C-string payload byte has been duplicated
    emitter.instruction("jz __rt_cstr_to_str_copy_done");                       // finish once the owned elephc payload has been fully initialized
    emitter.instruction("mov r10b, BYTE PTR [r9]");                             // load one byte from the foreign C string payload
    emitter.instruction("mov BYTE PTR [r8], r10b");                             // store the copied byte into the owned elephc payload
    emitter.instruction("add r9, 1");                                           // advance the foreign source cursor after copying one byte
    emitter.instruction("add r8, 1");                                           // advance the owned destination cursor after copying one byte
    emitter.instruction("sub rcx, 1");                                          // decrement the number of bytes still left to duplicate
    emitter.instruction("jmp __rt_cstr_to_str_copy_loop");                      // continue the byte-copy loop until the full payload has been duplicated

    emitter.label("__rt_cstr_to_str_copy_done");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // restore the elephc string byte length for the x86_64 string result pair
    emitter.instruction("sub r8, rdx");                                         // recover the base pointer of the owned payload after the post-increment copy loop
    emitter.instruction("mov rax, r8");                                         // return the owned elephc string pointer in the x86_64 string result register
    emitter.instruction("add rsp, 16");                                         // release the temporary spill slots reserved by the helper
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the owned elephc string pair in rax=ptr, rdx=len
}
