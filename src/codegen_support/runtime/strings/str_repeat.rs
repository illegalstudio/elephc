//! Purpose:
//! Emits the `__rt_str_repeat`, `__rt_str_repeat_loop` runtime helper assembly for str repeat.
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
    emitter.instruction("mov x9, x0");                                          // initialize the destination cursor at the reserved payload start

    // -- copy the first source instance, then double the result until it is complete --
    emitter.instruction("str x0, [sp, #32]");                                   // save the repeated-string start pointer
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the source pointer for the first instance
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the source length for the first instance
    emitter.instruction("mov x9, x0");                                          // initialize the destination cursor at the reserved payload
    emitter.label("__rt_str_repeat_first_words");
    emitter.instruction("cmp x2, #8");                                          // can the first instance copy a full word?
    emitter.instruction("b.lt __rt_str_repeat_first_tail");                     // short first instances copy their remaining bytes
    emitter.instruction("ldr x3, [x1], #8");                                    // load eight source bytes
    emitter.instruction("str x3, [x9], #8");                                    // store eight destination bytes
    emitter.instruction("sub x2, x2, #8");                                      // account for the copied word
    emitter.instruction("b __rt_str_repeat_first_words");                       // continue copying full source words
    emitter.label("__rt_str_repeat_first_tail");
    emitter.instruction("cbz x2, __rt_str_repeat_first_done");                  // stop once the first instance is copied
    emitter.instruction("ldrb w3, [x1], #1");                                   // load one trailing source byte
    emitter.instruction("strb w3, [x9], #1");                                   // store one trailing destination byte
    emitter.instruction("sub x2, x2, #1");                                      // account for the copied byte
    emitter.instruction("b __rt_str_repeat_first_tail");                        // continue copying trailing source bytes
    emitter.label("__rt_str_repeat_first_done");
    emitter.instruction("ldr x10, [sp, #8]");                                   // initialize the copied length to one source instance
    emitter.instruction("str x10, [sp, #40]");                                  // publish the copied-length cursor

    emitter.label("__rt_str_repeat_double_loop");
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the copied length
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the requested result length
    emitter.instruction("cmp x10, x11");                                        // has the destination reached the requested length?
    emitter.instruction("b.ge __rt_str_repeat_done");                           // finish once every requested byte is present
    emitter.instruction("sub x12, x11, x10");                                   // compute the still-missing byte count
    emitter.instruction("cmp x12, x10");                                        // can the whole copied prefix be doubled?
    emitter.instruction("csel x12, x12, x10, lo");                              // copy min(missing, copied) bytes
    emitter.instruction("ldr x13, [sp, #32]");                                  // reload the repeated-string start pointer
    emitter.instruction("mov x14, x13");                                        // source cursor = the already copied prefix
    emitter.instruction("add x13, x13, x10");                                   // destination cursor = the end of the copied prefix
    emitter.instruction("mov x15, x12");                                        // initialize the doubling copy counter
    emitter.label("__rt_str_repeat_double_words");
    emitter.instruction("cmp x15, #8");                                         // can the doubling copy a full word?
    emitter.instruction("b.lt __rt_str_repeat_double_tail");                    // short tails copy their remaining bytes
    emitter.instruction("ldr x3, [x14], #8");                                   // load eight already copied bytes
    emitter.instruction("str x3, [x13], #8");                                   // append eight doubled bytes
    emitter.instruction("sub x15, x15, #8");                                    // account for the copied word
    emitter.instruction("b __rt_str_repeat_double_words");                      // continue doubling full words
    emitter.label("__rt_str_repeat_double_tail");
    emitter.instruction("cbz x15, __rt_str_repeat_double_done");                // stop once this doubling chunk is copied
    emitter.instruction("ldrb w3, [x14], #1");                                  // load one trailing copied byte
    emitter.instruction("strb w3, [x13], #1");                                  // append one trailing doubled byte
    emitter.instruction("sub x15, x15, #1");                                    // account for the copied byte
    emitter.instruction("b __rt_str_repeat_double_tail");                       // continue doubling trailing bytes
    emitter.label("__rt_str_repeat_double_done");
    emitter.instruction("add x10, x10, x12");                                   // extend the copied length by this chunk
    emitter.instruction("str x10, [sp, #40]");                                  // publish the extended copied length
    emitter.instruction("b __rt_str_repeat_double_loop");                       // double again until the request is complete

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
    emitter.instruction("mov r11, rax");                                        // initialize the destination cursor at the reserved payload start

    // -- copy the first source instance, then double the result until it is complete --
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the repeated-string start pointer
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the source pointer for the first instance
    emitter.instruction("mov r9, rax");                                         // initialize the destination cursor at the reserved payload
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the source length for the first instance
    emitter.label("__rt_str_repeat_first_words_linux_x86_64");
    emitter.instruction("cmp r10, 8");                                          // can the first instance copy a full word?
    emitter.instruction("jl __rt_str_repeat_first_tail_linux_x86_64");          // short first instances copy their remaining bytes
    emitter.instruction("mov r11, QWORD PTR [r8]");                             // load eight source bytes
    emitter.instruction("mov QWORD PTR [r9], r11");                             // store eight destination bytes
    emitter.instruction("add r8, 8");                                           // advance the source cursor by one word
    emitter.instruction("add r9, 8");                                           // advance the destination cursor by one word
    emitter.instruction("sub r10, 8");                                          // account for the copied word
    emitter.instruction("jmp __rt_str_repeat_first_words_linux_x86_64");        // continue copying full source words
    emitter.label("__rt_str_repeat_first_tail_linux_x86_64");
    emitter.instruction("test r10, r10");                                       // stop once the first instance is copied
    emitter.instruction("jz __rt_str_repeat_first_done_linux_x86_64");          // continue to the doubling loop
    emitter.instruction("mov r11b, BYTE PTR [r8]");                             // load one trailing source byte
    emitter.instruction("mov BYTE PTR [r9], r11b");                             // store one trailing destination byte
    emitter.instruction("inc r8");                                              // advance the source cursor by one byte
    emitter.instruction("inc r9");                                              // advance the destination cursor by one byte
    emitter.instruction("dec r10");                                             // account for the copied byte
    emitter.instruction("jmp __rt_str_repeat_first_tail_linux_x86_64");         // continue copying trailing source bytes
    emitter.label("__rt_str_repeat_first_done_linux_x86_64");
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // initialize the copied length to one source instance
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // publish the copied-length cursor

    emitter.label("__rt_str_repeat_double_loop_linux_x86_64");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the copied length
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the requested result length
    emitter.instruction("cmp r10, r11");                                        // has the destination reached the requested length?
    emitter.instruction("jge __rt_str_repeat_done_linux_x86_64");               // finish once every requested byte is present
    emitter.instruction("mov rcx, r11");                                        // compute the still-missing byte count
    emitter.instruction("sub rcx, r10");                                        // remaining = requested - copied
    emitter.instruction("cmp rcx, r10");                                        // can the whole copied prefix be doubled?
    emitter.instruction("cmova rcx, r10");                                      // chunk = min(missing, copied)
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the repeated-string start pointer
    emitter.instruction("mov r8, rax");                                         // source cursor = the already copied prefix
    emitter.instruction("add rax, r10");                                        // destination cursor = the end of the copied prefix
    emitter.instruction("mov r9, rax");                                         // keep the doubling destination cursor
    emitter.instruction("mov rdx, rcx");                                        // initialize the doubling copy counter
    emitter.label("__rt_str_repeat_double_words_linux_x86_64");
    emitter.instruction("cmp rdx, 8");                                          // can the doubling copy a full word?
    emitter.instruction("jl __rt_str_repeat_double_tail_linux_x86_64");         // short tails copy their remaining bytes
    emitter.instruction("mov r11, QWORD PTR [r8]");                             // load eight already copied bytes
    emitter.instruction("mov QWORD PTR [r9], r11");                             // append eight doubled bytes
    emitter.instruction("add r8, 8");                                           // advance the source cursor by one word
    emitter.instruction("add r9, 8");                                           // advance the destination cursor by one word
    emitter.instruction("sub rdx, 8");                                          // account for the copied word
    emitter.instruction("jmp __rt_str_repeat_double_words_linux_x86_64");       // continue doubling full words
    emitter.label("__rt_str_repeat_double_tail_linux_x86_64");
    emitter.instruction("test rdx, rdx");                                       // stop once this doubling chunk is copied
    emitter.instruction("jz __rt_str_repeat_double_done_linux_x86_64");         // continue with the next doubling chunk
    emitter.instruction("mov r11b, BYTE PTR [r8]");                             // load one trailing copied byte
    emitter.instruction("mov BYTE PTR [r9], r11b");                             // append one trailing doubled byte
    emitter.instruction("inc r8");                                              // advance the source cursor by one byte
    emitter.instruction("inc r9");                                              // advance the destination cursor by one byte
    emitter.instruction("dec rdx");                                             // account for the copied byte
    emitter.instruction("jmp __rt_str_repeat_double_tail_linux_x86_64");        // continue doubling trailing bytes
    emitter.label("__rt_str_repeat_double_done_linux_x86_64");
    emitter.instruction("add r10, rcx");                                        // extend the copied length by this chunk
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // publish the extended copied length
    emitter.instruction("jmp __rt_str_repeat_double_loop_linux_x86_64");        // double again until the request is complete

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
