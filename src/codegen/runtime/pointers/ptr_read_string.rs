//! Purpose:
//! Emits the `__rt_ptr_read_string` runtime helper assembly for raw memory to PHP string copies.
//! Allocates owned string storage and copies an exact byte length without null-termination semantics.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::pointers`.
//!
//! Key details:
//! - Negative lengths are fatal, zero length returns the normal empty string pair, and heap kind/refcount must match owned strings.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;
const PTR_READ_STRING_NEG_LEN_MSG_LEN: usize =
    "Fatal error: ptr_read_string() length must be non-negative\n".len();

/// Emits the `__rt_ptr_read_string` runtime helper that copies raw memory into an owned PHP string.
///
/// Dispatches to the architecture-specific implementation. On ARM64 the input is x0=source, x1=length
/// and the output is x1=string pointer, x2=length. On x86_64 the input is rax=source, rdx=length and
/// the output is rax=string pointer, rdx=length. Negative lengths are fatal; zero length returns the
/// canonical empty string pair (null pointer, zero length). The heap kind stamped is 1 (owned string).
pub(crate) fn emit_ptr_read_string(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ptr_read_string_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.raw("    .p2align 2");                                             // ensure 4-byte alignment for ARM64 instructions
    emitter.comment("--- runtime: ptr_read_string ---");
    emitter.label_global("__rt_ptr_read_string");

    // -- reject negative lengths before allocating --
    emitter.instruction("cmp x1, #0");                                          // check whether the requested raw byte length is negative
    emitter.instruction("b.lt __rt_ptr_read_string_neg_len");                   // negative byte counts are fatal instead of being reinterpreted as huge sizes
    emitter.instruction("cbz x1, __rt_ptr_read_string_empty");                  // zero-length reads return the canonical empty PHP string pair

    // -- preserve source metadata across heap allocation --
    emitter.instruction("sub sp, sp, #32");                                     // reserve a frame and spill slots for source pointer plus length
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address across the heap allocator call
    emitter.instruction("add x29, sp, #16");                                    // establish a stable frame pointer for this helper
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the raw source pointer across allocation
    emitter.instruction("str x1, [sp, #8]");                                    // preserve the requested byte length across allocation
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
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper spill frame
    emitter.instruction("ret");                                                 // return x1=owned string pointer, x2=byte length

    emitter.label("__rt_ptr_read_string_empty");
    emitter.instruction("mov x1, #0");                                          // empty strings use the canonical null payload pointer
    emitter.instruction("mov x2, #0");                                          // empty strings have zero byte length
    emitter.instruction("ret");                                                 // return the empty PHP string pair to the caller

    emitter.label("__rt_ptr_read_string_neg_len");
    emitter.instruction("mov x0, #2");                                          // fd = stderr for the fatal negative-length diagnostic
    abi::emit_symbol_address(emitter, "x1", "_ptr_read_string_len_err_msg");
    let len_instr = format!("mov x2, #{}", PTR_READ_STRING_NEG_LEN_MSG_LEN);
    emitter.instruction(&len_instr);                                            // pass the exact negative-length diagnostic byte count
    emitter.syscall(4);
    emitter.instruction("mov x0, #1");                                          // exit code 1 for the fatal negative-length abort path
    emitter.syscall(1);
}

/// Emits the x86_64 Linux implementation of `__rt_ptr_read_string`.
///
/// Input registers: rax = source pointer, rdx = byte length. Output: rax = string pointer, rdx = length.
/// The function checks for negative length (fatal), zero length (returns canonical empty pair), and
/// copies exactly the requested byte count into a heap-allocated owned string stamped with heap kind 1.
/// Uses the x86_64 System V ABI calling convention with aligned spill slots.
fn emit_ptr_read_string_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ptr_read_string ---");
    emitter.label_global("__rt_ptr_read_string");

    // -- reject negative lengths before allocating --
    emitter.instruction("cmp rdx, 0");                                          // check whether the requested raw byte length is negative
    emitter.instruction("jl __rt_ptr_read_string_neg_len_x86");                 // negative byte counts are fatal instead of becoming huge unsigned sizes
    emitter.instruction("test rdx, rdx");                                       // check whether the caller requested an empty raw byte range
    emitter.instruction("jz __rt_ptr_read_string_empty_x86");                   // zero-length reads return the canonical empty PHP string pair

    // -- preserve source metadata across heap allocation --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame pointer for source pointer and length spills
    emitter.instruction("sub rsp, 16");                                         // reserve aligned spill slots while keeping nested helper calls aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the raw source pointer across the heap allocation helper call
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the requested byte length across the heap allocation helper call
    emitter.instruction("mov rax, rdx");                                        // allocation size = requested raw byte length
    emitter.instruction("call __rt_heap_alloc");                                // allocate owned string payload storage on the elephc heap
    let kind_instr = format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 1);
    emitter.instruction(&kind_instr);                                           // materialize the x86_64 owned-string heap kind word
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the allocation as a string while preserving allocator refcount
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
    emitter.instruction("add rsp, 16");                                         // release the helper spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return rax=owned string pointer, rdx=byte length

    emitter.label("__rt_ptr_read_string_empty_x86");
    emitter.instruction("xor eax, eax");                                        // empty strings use the canonical null payload pointer
    emitter.instruction("xor edx, edx");                                        // empty strings have zero byte length
    emitter.instruction("ret");                                                 // return the empty PHP string pair to the caller

    emitter.label("__rt_ptr_read_string_neg_len_x86");
    emitter.instruction("mov edi, 2");                                          // fd = stderr for the fatal negative-length diagnostic
    abi::emit_symbol_address(emitter, "rsi", "_ptr_read_string_len_err_msg");
    let len_instr = format!("mov edx, {}", PTR_READ_STRING_NEG_LEN_MSG_LEN);
    emitter.instruction(&len_instr);                                            // pass the exact negative-length diagnostic byte count
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall number 1 = write
    emitter.instruction("syscall");                                             // emit the fatal negative-length message before terminating the process
    emitter.instruction("mov edi, 1");                                          // return process exit code 1 for the fatal abort path
    emitter.instruction("mov eax, 60");                                         // Linux x86_64 syscall number 60 = exit
    emitter.instruction("syscall");                                             // terminate the process after reporting the fatal length error
}
