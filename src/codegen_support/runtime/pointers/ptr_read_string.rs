//! Purpose:
//! Emits the `__rt_ptr_read_string` runtime helper assembly for raw memory to PHP string copies.
//! Allocates owned string storage and copies an exact byte length without null-termination semantics.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::pointers`.
//!
//! Key details:
//! - Negative lengths are fatal, zero length returns the normal empty string pair, and heap kind/refcount must match owned strings.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

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
    elephc_memoir::interop::c_strings::emit_copy_raw_bytes_aarch64(emitter);
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
    elephc_memoir::interop::c_strings::emit_copy_raw_bytes_x86_64(emitter);
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
    emitter.instruction("mov eax, 231");                                        // Linux x86_64 syscall 231 = exit_group
    emitter.instruction("syscall");                                             // terminate the process after reporting the fatal length error
}
