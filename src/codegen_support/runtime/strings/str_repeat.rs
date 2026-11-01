//! Purpose:
//! Emits the `__rt_str_repeat` runtime helper assembly for str repeat.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - `length * times` is computed with an overflow check (`umulh` / `imul`+`jo`); a wrapped
//!   product reports PHP's allocation-overflow fatal instead of writing `times * length`
//!   bytes into a destination sized by the wrapped value.
//! - Storage comes from `__rt_concat_reserve`, so large repeated strings fall back to heap
//!   storage instead of overrunning the fixed 64 KiB concat scratch buffer.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::data::STR_REPEAT_TIMES_MSG;


/// Emits the `__rt_str_repeat` runtime helper for repeating a string N times.
///
/// Targets ARM64 (and other non-x86_64) architectures. The helper accepts a source
/// string pointer/length and a repetition count, writing the repeated result into
/// concat scratch storage when it fits within the 64 KiB limit, or into a heap-allocated
/// buffer otherwise.
///
/// # Input (ARM64 calling convention)
/// - `x1`: source string pointer
/// - `x2`: source string length in bytes
/// - `x3`: repetition count (must be non-negative; negative values cause fatal error)
///
/// # Output (ARM64 calling convention)
/// - `x1`: result string pointer (null if result is empty)
/// - `x2`: result string length in bytes
///
/// # Behavior
/// - If `times == 0` or source length is 0, returns an empty string (null pointer, zero length).
/// - Sizes the result through `__rt_concat_reserve`: concat scratch while it fits within the
///   64 KiB buffer, an owned heap allocation otherwise. `__rt_concat_publish` then advances
///   the concat scratch offset only for scratch-backed results.
/// - On negative repetition count, emits a fatal error message and terminates the process.
/// - When `length * times` overflows a machine word, branches to `__rt_alloc_overflow` and
///   terminates with PHP's "Possible integer overflow in memory allocation" fatal error.
pub fn emit_str_repeat(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_repeat_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: str_repeat ---");
    emitter.label_global("__rt_str_repeat");

    // -- set up stack frame (64 bytes) --
    emitter.instruction("sub sp, sp, #64");                                     // allocate spill space for inputs and result metadata
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish new frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save source pointer and length
    emitter.instruction("str x3, [sp, #16]");                                   // save repetition count

    // -- reject negative counts and return empty for zero-byte results --
    emitter.instruction("cmp x3, #0");                                          // check whether the repetition count is negative
    emitter.instruction("b.lt __rt_str_repeat_negative_times");                 // report PHP-compatible failure for negative repetition counts
    emitter.instruction("cbz x3, __rt_str_repeat_empty");                       // return the canonical empty string when no repetitions are requested
    emitter.instruction("cbz x2, __rt_str_repeat_empty");                       // return the canonical empty string when the source has no bytes

    // -- size the result, rejecting a wrapped length * times product --
    emitter.instruction("umulh x5, x2, x3");                                    // capture the high half of the length * repetition-count product
    emitter.instruction("cbnz x5, __rt_str_repeat_size_overflow");              // a non-zero high half means the byte count cannot be represented
    emitter.instruction("mul x4, x2, x3");                                      // compute result length = source length * repetition count
    emitter.instruction("str x4, [sp, #24]");                                   // save result length for finalization
    emitter.instruction("mov x0, x4");                                          // request storage for the full repeated payload
    emitter.instruction("bl __rt_concat_reserve");                              // reserve concat scratch or owned heap storage for the repeated string
    emitter.instruction("str x0, [sp, #32]");                                   // save result start pointer for the return pair

    // -- fill by doubling: one copy of the source, then the filled prefix onto its own end --
    // The copy used to go byte by byte: Symfony's `str_repeat('x', 32768)` on every request
    // cost ~160k instructions. Each memcpy below copies min(filled, remaining) bytes, so the
    // source and destination never overlap and the fill takes log2(times) + 1 calls.
    emitter.instruction("ldp x1, x2, [sp]");                                    // source pointer and length
    emitter.bl_c("memcpy");                                                     // first instance at the start of the result
    emitter.instruction("ldr x9, [sp, #8]");                                    // bytes filled so far = one source length
    emitter.instruction("str x9, [sp, #40]");                                   // keep the filled count across memcpy calls
    emitter.label("__rt_str_repeat_double");
    emitter.instruction("ldr x9, [sp, #40]");                                   // bytes filled so far
    emitter.instruction("ldr x10, [sp, #24]");                                  // total result length
    emitter.instruction("subs x11, x10, x9");                                   // bytes still to fill
    emitter.instruction("b.ls __rt_str_repeat_done");                           // the result is complete
    emitter.instruction("cmp x11, x9");                                         // copy no more than what is already filled
    emitter.instruction("csel x2, x11, x9, lo");                                // n = min(remaining, filled)
    emitter.instruction("add x12, x9, x2");                                     // filled count after this copy
    emitter.instruction("str x12, [sp, #40]");                                  // remember it across memcpy
    emitter.instruction("ldr x1, [sp, #32]");                                   // source = the filled prefix
    emitter.instruction("add x0, x1, x9");                                      // destination = first unfilled byte
    emitter.bl_c("memcpy");                                                     // append the prefix to itself
    emitter.instruction("b __rt_str_repeat_double");                            // until the result is full

    // -- finalize: return the repeated string --
    emitter.label("__rt_str_repeat_done");
    emitter.instruction("ldr x1, [sp, #32]");                                   // return the repeated string pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // return the precomputed repeated string length
    emitter.instruction("bl __rt_concat_publish");                              // advance the concat scratch offset only for scratch-backed results
    emitter.instruction("b __rt_str_repeat_return");                            // skip the empty-string return setup

    // -- empty result: return null pointer with zero length --
    emitter.label("__rt_str_repeat_empty");
    emitter.instruction("mov x1, #0");                                          // empty string has no result payload pointer
    emitter.instruction("mov x2, #0");                                          // empty string length is zero

    // -- restore frame and return --
    emitter.label("__rt_str_repeat_return");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate the repeat-helper stack frame
    emitter.instruction("ret");                                                 // return to caller

    // -- fatal error: negative repetition count --
    emitter.label("__rt_str_repeat_negative_times");
    emitter.instruction("mov x0, #2");                                          // fd = stderr for the negative-repeat diagnostic
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_str_repeat_times_msg");
    emitter.instruction(&format!("mov x2, #{}", STR_REPEAT_TIMES_MSG.len()));   // pass the exact negative-repeat diagnostic byte count
    emitter.syscall(4);
    emitter.instruction("mov x0, #1");                                          // exit code 1 for the negative-repeat abort path
    emitter.syscall(1);

    // -- fatal error: length * times does not fit a machine word --
    emitter.label("__rt_str_repeat_size_overflow");
    emitter.instruction("b __rt_alloc_overflow");                               // unconditional branch keeps the fatal trampoline cross-atom safe
}

/// Emits the `__rt_str_repeat` runtime helper for repeating a string N times on Linux x86_64.
///
/// Uses the standard x86_64 System V ABI: source string in `rax/rdx`, repetition count in `rdi`.
/// Result is returned in `rax/rdx` (pointer/length). Behavior mirrors the ARM64 variant:
/// `__rt_concat_reserve` picks concat scratch or heap storage, `imul`+`jo` rejects a wrapped
/// `length * times` product, and a negative repetition count is a fatal error.
fn emit_str_repeat_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_repeat ---");
    emitter.label_global("__rt_str_repeat");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving the repeat-helper spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for saved inputs and result metadata
    emitter.instruction("sub rsp, 48");                                         // reserve aligned spill slots for source string, repeat count, result length, result pointer, and storage kind
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the source string pointer across the nested copy loops that reuse caller-saved registers
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the source string length across the nested copy loops that reuse caller-saved registers
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // preserve the requested repetition count before the outer loop decrements it

    // -- reject negative counts and return empty for zero-byte results --
    emitter.instruction("test rdi, rdi");                                       // check whether the requested repetition count is negative or zero
    emitter.instruction("jl __rt_str_repeat_negative_times_linux_x86_64");      // report PHP-compatible failure for negative repetition counts
    emitter.instruction("jz __rt_str_repeat_empty_linux_x86_64");               // return an empty string when no repetitions are requested
    emitter.instruction("test rdx, rdx");                                       // check whether the source string contains any bytes to repeat
    emitter.instruction("jz __rt_str_repeat_empty_linux_x86_64");               // return an empty string when the source payload is empty

    // -- size the result, rejecting a wrapped length * times product --
    emitter.instruction("mov rcx, rdx");                                        // seed result length from the source string length
    emitter.instruction("imul rcx, rdi");                                       // compute result length = source length * repetition count
    emitter.instruction("jo __rt_str_repeat_size_overflow_linux_x86_64");       // a wrapped product cannot describe the bytes the copy loop would write
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save result length for finalization
    emitter.instruction("mov rax, rcx");                                        // request storage for the full repeated payload
    emitter.instruction("call __rt_concat_reserve");                            // reserve concat scratch or owned heap storage for the repeated string
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the repeated-string start pointer for the return pair

    // -- fill by doubling: one copy of the source, then the filled prefix onto its own end --
    emitter.instruction("mov rdi, rax");                                        // destination = start of the result
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // source string pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // source string length
    emitter.instruction("call memcpy");                                         // first instance at the start of the result
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // bytes filled so far = one source length
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // keep the filled count across memcpy calls
    emitter.label("__rt_str_repeat_double_linux_x86_64");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // bytes filled so far
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // total result length
    emitter.instruction("sub rdx, rcx");                                        // bytes still to fill
    emitter.instruction("jbe __rt_str_repeat_done_linux_x86_64");               // the result is complete
    emitter.instruction("cmp rdx, rcx");                                        // copy no more than what is already filled
    emitter.instruction("cmova rdx, rcx");                                      // n = min(remaining, filled)
    emitter.instruction("lea r8, [rcx + rdx]");                                 // filled count after this copy
    emitter.instruction("mov QWORD PTR [rbp - 48], r8");                        // remember it across memcpy
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // source = the filled prefix
    emitter.instruction("lea rdi, [rsi + rcx]");                                // destination = first unfilled byte
    emitter.instruction("call memcpy");                                         // append the prefix to itself
    emitter.instruction("jmp __rt_str_repeat_double_linux_x86_64");             // until the result is full

    // -- finalize: return the repeated string --
    emitter.label("__rt_str_repeat_done_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // return the repeated string pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // return the precomputed repeated string length
    emitter.instruction("call __rt_concat_publish");                            // advance the concat scratch offset only for scratch-backed results
    emitter.instruction("jmp __rt_str_repeat_return_linux_x86_64");             // skip the empty-string return setup

    // -- empty result: return null pointer with zero length --
    emitter.label("__rt_str_repeat_empty_linux_x86_64");
    emitter.instruction("xor rax, rax");                                        // empty string has no result payload pointer
    emitter.instruction("xor rdx, rdx");                                        // empty string length is zero

    emitter.label("__rt_str_repeat_return_linux_x86_64");
    emitter.instruction("add rsp, 48");                                         // release the repeat-helper spill slots before returning the repeated string
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the repeated string
    emitter.instruction("ret");                                                 // return the repeated string in the standard x86_64 string result registers

    // -- fatal error: negative repetition count --
    emitter.label("__rt_str_repeat_negative_times_linux_x86_64");
    emitter.instruction("mov edi, 2");                                          // fd = stderr for the negative-repeat diagnostic
    crate::codegen_support::abi::emit_symbol_address(emitter, "rsi", "_str_repeat_times_msg");
    emitter.instruction(&format!("mov edx, {}", STR_REPEAT_TIMES_MSG.len()));   // pass the exact negative-repeat diagnostic byte count
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // emit the fatal negative-repeat message before terminating
    emitter.instruction("mov edi, 1");                                          // exit code 1 for the negative-repeat abort path
    emitter.instruction("mov eax, 231");                                        // Linux x86_64 syscall 231 = exit_group
    emitter.instruction("syscall");                                             // terminate the process after reporting the invalid repeat count

    // -- fatal error: length * times does not fit a machine word --
    emitter.label("__rt_str_repeat_size_overflow_linux_x86_64");
    emitter.instruction("jmp __rt_alloc_overflow");                             // unconditional branch keeps the fatal trampoline reachable from every caller
}
