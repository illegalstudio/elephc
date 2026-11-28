//! Purpose:
//! Emits independent owned storage when copying to and from C strings.
//!
//! Called from:
//! - Compiler pointer interoperability adapters.
//!
//! Key details:
//! - C strings receive a trailing NUL; native strings retain a pointer/length result.
//! - A null C input produces the existing empty-string result without acquiring an owner.

use crate::{abi, Arch, Emitter};

/// Describes the native register shape that must survive C-string cleanup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ForeignReturnShape {
    /// The foreign call has no live return value.
    Void,
    /// The return occupies the integer result register.
    Scalar,
    /// The return occupies the floating-point result register.
    Float,
    /// The return occupies the native string pointer and length registers.
    String,
}

/// Releases call-scoped C-string copies while preserving the foreign call result.
///
/// The owner copies already occupy `cleanup_bytes` below the current stack top.
/// This protocol saves the typed return above them, retires every owner in caller
/// order, restores the return and finally releases the owner record.
pub fn emit_release_call_scoped_c_strings(
    emitter: &mut impl Emitter,
    owner_count: usize,
    cleanup_bytes: usize,
    return_shape: ForeignReturnShape,
) {
    if owner_count == 0 {
        return;
    }

    let saved_return_bytes = match return_shape {
        ForeignReturnShape::Void => 0,
        ForeignReturnShape::Scalar => {
            let result = emitter.int_result_reg();
            emitter.emit_push_reg(result);
            16
        }
        ForeignReturnShape::Float => {
            let result = emitter.float_result_reg();
            emitter.emit_push_float_reg(result);
            16
        }
        ForeignReturnShape::String => {
            let (pointer, length) = emitter.string_result_regs();
            emitter.emit_push_reg_pair(pointer, length);
            16
        }
    };

    for index in 0..owner_count {
        let result = emitter.int_result_reg();
        emitter.emit_load_temporary_stack_slot(
            result,
            saved_return_bytes + index * 16,
        );
        emitter.emit_call_label("__rt_heap_free");
    }

    match return_shape {
        ForeignReturnShape::Void => {}
        ForeignReturnShape::Scalar => {
            let result = emitter.int_result_reg();
            emitter.emit_pop_reg(result);
        }
        ForeignReturnShape::Float => {
            let result = emitter.float_result_reg();
            emitter.emit_pop_float_reg(result);
        }
        ForeignReturnShape::String => {
            let (pointer, length) = emitter.string_result_regs();
            emitter.emit_load_temporary_stack_slot(pointer, 0);
            emitter.emit_load_temporary_stack_slot(length, 8);
            emitter.emit_release_temporary_stack(16);
        }
    }
    emitter.emit_release_temporary_stack(cleanup_bytes);
}

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

/// Copies the nonempty input slice staged by the aarch64 caller into owned storage.
pub fn emit_copy_raw_bytes_aarch64(emitter: &mut impl Emitter) {
    emitter.instruction("mov x0, x1");                                          // allocation size = requested raw byte length
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate owned string payload storage on the elephc heap
    emitter.instruction("mov x3, #1");                                          // heap kind 1 = owned elephc string payload
    emitter.instruction("str x3, [x0, #-8]");                                   // stamp the allocation as a string while preserving allocator refcount

    // -- copy exactly len bytes from raw memory to the owned string payload --
    emitter.instruction("ldr x9, [sp, #0]");                                    // restore the raw source pointer after allocation
    emitter.instruction("ldr x2, [sp, #8]");                                    // restore the original byte length for the result pair
    emitter.instruction("mov x1, x0");                                          // x1 = owned PHP string payload pointer for the return convention
    emitter.instruction("mov x10, x1");                                         // initialize destination cursor to the owned string payload
    emitter.instruction("mov x11, x9");                                         // initialize source cursor to the raw memory address
    emitter.instruction("mov x12, x2");                                         // initialize remaining byte counter from the requested length

    emitter.label("__rt_ptr_read_string_copy_loop");
    emitter.instruction("cbz x12, __rt_ptr_read_string_copy_done");             // finish once every requested byte has been copied
    emitter.instruction("ldrb w13, [x11], #1");                                 // load the next raw source byte and advance the source cursor
    emitter.instruction("strb w13, [x10], #1");                                 // store the byte into the owned string payload and advance the destination cursor
    emitter.instruction("sub x12, x12, #1");                                    // decrement the number of bytes left to copy
    emitter.instruction("b __rt_ptr_read_string_copy_loop");                    // continue copying until the exact requested length is satisfied

    emitter.label("__rt_ptr_read_string_copy_done");
}

/// Copies the nonempty input slice staged by the x86_64 caller into owned storage.
pub fn emit_copy_raw_bytes_x86_64(emitter: &mut impl Emitter) {
    emitter.instruction("mov rax, rdx");                                        // allocation size = requested raw byte length
    crate::strings::str_new_owned::emit_allocate_owned_string(emitter);
    emitter.instruction("mov r8, rax");                                         // initialize destination cursor to the owned string payload
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // restore the raw source pointer after allocation
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // restore the requested byte length as the loop counter

    // -- copy exactly len bytes from raw memory to the owned string payload --
    emitter.label("__rt_ptr_read_string_copy_loop_x86");
    emitter.instruction("test rcx, rcx");                                       // finish once every requested byte has been copied
    emitter.instruction("jz __rt_ptr_read_string_copy_done_x86");               // exit the byte-copy loop when the counter reaches zero
    emitter.instruction("mov r10b, BYTE PTR [r9]");                             // load the next raw source byte
    emitter.instruction("mov BYTE PTR [r8], r10b");                             // store the copied byte into the owned string payload
    emitter.instruction("add r9, 1");                                           // advance the raw source cursor after copying one byte
    emitter.instruction("add r8, 1");                                           // advance the owned destination cursor after copying one byte
    emitter.instruction("sub rcx, 1");                                          // decrement the number of bytes left to copy
    emitter.instruction("jmp __rt_ptr_read_string_copy_loop_x86");              // continue copying until the exact requested length is satisfied

    emitter.label("__rt_ptr_read_string_copy_done_x86");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // restore the byte length for the x86_64 string result pair
    emitter.instruction("sub r8, rdx");                                         // recover the owned string payload base after the post-increment copy loop
    emitter.instruction("mov rax, r8");                                         // return the owned string payload pointer in the x86_64 string result register
}

/// Allocates one block and writes two NUL-terminated copies from staged byte pairs.
/// Returns the two C pointers in the native C argument registers.
pub fn emit_stage_owned_c_string_pair(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.blank();
            emitter.comment("-- stage both operands as NUL-terminated C strings in one heap block --");
            emitter.instruction("ldr x0, [sp, #48]");                           // left byte count
            emitter.instruction("ldr x9, [sp, #64]");                           // right byte count
            emitter.instruction("add x0, x0, x9");                              // bytes needed by both operands
            emitter.instruction("add x0, x0, #2");                              // plus one NUL per operand
            emitter.instruction("bl __rt_heap_alloc");                          // one allocation serves strcoll's two operands
            emitter.instruction("str x0, [sp, #128]");                          // keep the staging block base
            emitter.instruction("ldr x1, [sp, #40]");                           // left operand source
            emitter.instruction("ldr x2, [sp, #48]");                           // left operand length
            emitter.instruction("mov x9, #0");                                  // copy from the first byte
            emitter.instruction("cbz x2, __rt_kcf_locale_left_done");           // an empty operand has no bytes to copy
            emitter.label("__rt_kcf_locale_left_copy");
            emitter.instruction("ldrb w10, [x1, x9]");                          // load the next left byte
            emitter.instruction("strb w10, [x0, x9]");                          // store it into the staged operand
            emitter.instruction("add x9, x9, #1");                              // advance
            emitter.instruction("cmp x9, x2");                                  // copied the whole operand?
            emitter.instruction("b.lo __rt_kcf_locale_left_copy");              // keep copying
            emitter.label("__rt_kcf_locale_left_done");
            emitter.instruction("strb wzr, [x0, x2]");                          // NUL-terminate the left operand
            emitter.instruction("add x0, x0, x2");                              // step past the left operand
            emitter.instruction("add x0, x0, #1");                              // and past its NUL
            emitter.instruction("ldr x1, [sp, #56]");                           // right operand source
            emitter.instruction("ldr x2, [sp, #64]");                           // right operand length
            emitter.instruction("mov x9, #0");                                  // copy from the first byte
            emitter.instruction("cbz x2, __rt_kcf_locale_right_done");          // an empty operand has no bytes to copy
            emitter.label("__rt_kcf_locale_right_copy");
            emitter.instruction("ldrb w10, [x1, x9]");                          // load the next right byte
            emitter.instruction("strb w10, [x0, x9]");                          // store it into the staged operand
            emitter.instruction("add x9, x9, #1");                              // advance
            emitter.instruction("cmp x9, x2");                                  // copied the whole operand?
            emitter.instruction("b.lo __rt_kcf_locale_right_copy");             // keep copying
            emitter.label("__rt_kcf_locale_right_done");
            emitter.instruction("strb wzr, [x0, x2]");                          // NUL-terminate the right operand
            emitter.instruction("ldr x0, [sp, #128]");                          // pass the staged left C string
            emitter.instruction("ldr x9, [sp, #48]");                           // left byte count
            emitter.instruction("add x1, x0, x9");                              // step past the left operand
            emitter.instruction("add x1, x1, #1");                              // and past its NUL: the staged right C string
        }
        Arch::X86_64 => {
            emitter.blank();
            emitter.comment("-- stage both operands as NUL-terminated C strings in one heap block --");
            emitter.instruction("mov rax, QWORD PTR [rbp - 56]");               // left byte count
            emitter.instruction("add rax, QWORD PTR [rbp - 72]");               // bytes needed by both operands
            emitter.instruction("add rax, 2");                                  // plus one NUL per operand
            emitter.instruction("call __rt_heap_alloc");                        // one allocation serves strcoll's two operands
            emitter.instruction("mov QWORD PTR [rbp - 136], rax");              // keep the staging block base
            emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");               // left operand source
            emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");               // left operand length
            emitter.instruction("xor edx, edx");                                // copy from the first byte
            emitter.instruction("test rcx, rcx");                               // an empty operand has no bytes to copy
            emitter.instruction("jz __rt_kcf_locale_left_done");                // skip the copy loop
            emitter.label("__rt_kcf_locale_left_copy");
            emitter.instruction("mov r8b, BYTE PTR [rsi + rdx]");               // load the next left byte
            emitter.instruction("mov BYTE PTR [rax + rdx], r8b");               // store it into the staged operand
            emitter.instruction("inc rdx");                                     // advance
            emitter.instruction("cmp rdx, rcx");                                // copied the whole operand?
            emitter.instruction("jb __rt_kcf_locale_left_copy");                // keep copying
            emitter.label("__rt_kcf_locale_left_done");
            emitter.instruction("mov BYTE PTR [rax + rcx], 0");                 // NUL-terminate the left operand
            emitter.instruction("add rax, rcx");                                // step past the left operand
            emitter.instruction("inc rax");                                     // and past its NUL
            emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");               // right operand source
            emitter.instruction("mov rcx, QWORD PTR [rbp - 72]");               // right operand length
            emitter.instruction("xor edx, edx");                                // copy from the first byte
            emitter.instruction("test rcx, rcx");                               // an empty operand has no bytes to copy
            emitter.instruction("jz __rt_kcf_locale_right_done");               // skip the copy loop
            emitter.label("__rt_kcf_locale_right_copy");
            emitter.instruction("mov r8b, BYTE PTR [rsi + rdx]");               // load the next right byte
            emitter.instruction("mov BYTE PTR [rax + rdx], r8b");               // store it into the staged operand
            emitter.instruction("inc rdx");                                     // advance
            emitter.instruction("cmp rdx, rcx");                                // copied the whole operand?
            emitter.instruction("jb __rt_kcf_locale_right_copy");               // keep copying
            emitter.label("__rt_kcf_locale_right_done");
            emitter.instruction("mov BYTE PTR [rax + rcx], 0");                 // NUL-terminate the right operand
            emitter.instruction("mov rdi, QWORD PTR [rbp - 136]");              // pass the staged left C string
            emitter.instruction("mov rsi, rdi");                                // start the right C string at the block base...
            emitter.instruction("add rsi, QWORD PTR [rbp - 56]");               // ...past the left operand...
            emitter.instruction("inc rsi");                                     // ...and past its NUL
        }
    }
}

/// Releases the staged pair after the caller has consumed its C pointers.
pub fn emit_release_owned_c_string_pair(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("ldr x0, [sp, #128]");                          // release the staging block
            emitter.instruction("bl __rt_heap_free");                           // return the block to the runtime heap
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, QWORD PTR [rbp - 136]");              // release the staging block
            emitter.instruction("call __rt_heap_free");                         // return the block to the runtime heap
        }
    }
}

/// Copies a staged environment assignment into storage with the lifetime required by libc putenv.
pub fn emit_environment_assignment(emitter: &mut impl Emitter, copy_loop: &str, copy_done: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("add x0, x2, #1");                              // allocate space for the environment string plus trailing null
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve the source string pointer and length across heap allocation
            abi::emit_call_label(emitter, "__rt_heap_alloc");
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the source string pointer and length after heap allocation
            emitter.instruction("mov x3, x0");                                  // keep the persistent destination buffer for copying and putenv()
            emitter.instruction("mov x4, #0");                                  // start copying at byte offset zero
            emitter.label(copy_loop);
            emitter.instruction("cmp x4, x2");                                  // compare the copied byte count with the source length
            emitter.instruction(&format!("b.ge {}", copy_done));                // finish once every source byte has been persisted
            emitter.instruction("ldrb w5, [x1, x4]");                           // load one byte from the source environment assignment
            emitter.instruction("strb w5, [x3, x4]");                           // copy the byte into the persistent putenv buffer
            emitter.instruction("add x4, x4, #1");                              // advance to the next source byte
            emitter.instruction(&format!("b {}", copy_loop));                   // continue copying the environment assignment
            emitter.label(copy_done);
            emitter.instruction("strb wzr, [x3, x4]");                          // append the C null terminator required by putenv()
            emitter.instruction("mov x0, x3");                                  // pass the persistent environment buffer to putenv()
        }
        Arch::X86_64 => {
            emitter.instruction("sub rsp, 16");                                 // reserve aligned spill space for the source string across heap allocation
            emitter.instruction("mov QWORD PTR [rsp], rax");                    // save the source environment string pointer
            emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                // save the source environment string length
            emitter.instruction("mov rax, rdx");                                // seed the heap allocation size from the source length
            emitter.instruction("add rax, 1");                                  // allocate space for the environment string plus trailing null
            abi::emit_call_label(emitter, "__rt_heap_alloc");
            emitter.instruction("mov rcx, QWORD PTR [rsp]");                    // restore the source environment string pointer
            emitter.instruction("mov r8, QWORD PTR [rsp + 8]");                 // restore the source environment string length
            emitter.instruction("add rsp, 16");                                 // release the temporary source string spill space
            emitter.instruction("mov r9, rax");                                 // keep the persistent destination buffer for copying and putenv()
            emitter.instruction("mov r10, 0");                                  // start copying at byte offset zero
            emitter.label(copy_loop);
            emitter.instruction("cmp r10, r8");                                 // compare the copied byte count with the source length
            emitter.instruction(&format!("jae {}", copy_done));                 // finish once every source byte has been persisted
            emitter.instruction("mov r11b, BYTE PTR [rcx + r10]");              // load one byte from the source environment assignment
            emitter.instruction("mov BYTE PTR [r9 + r10], r11b");               // copy the byte into the persistent putenv buffer
            emitter.instruction("add r10, 1");                                  // advance to the next source byte
            emitter.instruction(&format!("jmp {}", copy_loop));                 // continue copying the environment assignment
            emitter.label(copy_done);
            emitter.instruction("mov BYTE PTR [r9 + r10], 0");                  // append the C null terminator required by putenv()
            emitter.instruction("mov rdi, r9");                                 // pass the persistent environment buffer to putenv()
        }
    }
}

/// Copies a frame-backed byte slice to raw heap scratch, reserving a byte for a terminator.
pub fn emit_stage_length_bounded_input(emitter: &mut impl Emitter, source: usize, length: usize, buffer: usize) {
    let result = emitter.int_result_reg();
    let (arg0, arg1, arg2) = (emitter.int_arg_reg_name(0), emitter.int_arg_reg_name(1), emitter.int_arg_reg_name(2));
    abi::emit_reg_move(emitter, result, arg1);
    emitter.instruction(match emitter.arch() { Arch::AArch64 => "add x0, x0, #1", Arch::X86_64 => "add rax, 1" }); // reserve space for the native terminator
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    abi::store_at_offset(emitter, result, buffer);
    abi::emit_reg_move(emitter, arg0, result);
    abi::load_at_offset(emitter, arg1, source);
    abi::load_at_offset(emitter, arg2, length);
    emitter.bl_c("memcpy");                                                     // copy exactly the PHP byte range into privately owned scratch
}

/// Reserves an owned native path buffer with a uniform string header on every target.
pub fn emit_allocate_path_buffer(emitter: &mut impl Emitter, capacity: usize) {
    let result = emitter.int_result_reg();
    emitter.emit_load_int_immediate(result, capacity as i64);
    crate::strings::str_new_owned::emit_allocate_owned_string(emitter);
}

/// Releases a failed path buffer and returns the empty string sentinel.
///
/// `aarch64_spill` is SP-relative while `x86_frame_spill` is the positive
/// distance below RBP used by the surrounding runtime helper.
pub fn emit_release_failed_path_buffer(
    emitter: &mut impl Emitter,
    aarch64_spill: usize,
    x86_frame_spill: usize,
) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x0, [sp, #{aarch64_spill}]"));     // detach the unused owned path buffer
            emitter.emit_call_label("__rt_heap_free_safe");
            emitter.instruction("mov x1, #0");                                 // return the empty path pointer sentinel
            emitter.instruction("mov x2, #0");                                 // return zero path bytes
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {x86_frame_spill}]")); // detach the unused owned path buffer
            emitter.emit_call_label("__rt_heap_free_safe");
            emitter.instruction("xor eax, eax");                               // return the empty path pointer sentinel
            emitter.instruction("xor edx, edx");                               // return zero path bytes
        }
    }
}
