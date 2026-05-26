//! Purpose:
//! Emits the `__rt_str_repeat`, `__rt_str_repeat_loop` runtime helper assembly for str repeat.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::strings`.
//!
//! Key details:
//! - Large repeated strings fall back to heap storage so they cannot overrun the fixed concat scratch buffer.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::runtime::data::STR_REPEAT_TIMES_MSG;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

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
/// - If the repeated result fits within concat scratch (64 KiB), writes directly there and
///   advances the concat scratch write offset.
/// - If the result exceeds concat scratch capacity, allocates a heap buffer, stamps it as
///   an owned string, and does NOT update concat scratch offset.
/// - On negative repetition count, emits a fatal error message and terminates the process.
pub fn emit_str_repeat(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_repeat_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: str_repeat ---");
    emitter.label_global("__rt_str_repeat");

    // -- set up stack frame (80 bytes) --
    emitter.instruction("sub sp, sp, #80");                                     // allocate spill space for inputs, result metadata, and heap fallback state
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish new frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save source pointer and length
    emitter.instruction("str x3, [sp, #16]");                                   // save repetition count

    // -- reject negative counts and return empty for zero-byte results --
    emitter.instruction("cmp x3, #0");                                          // check whether the repetition count is negative
    emitter.instruction("b.lt __rt_str_repeat_negative_times");                 // report PHP-compatible failure for negative repetition counts
    emitter.instruction("cbz x3, __rt_str_repeat_empty");                       // return the canonical empty string when no repetitions are requested
    emitter.instruction("cbz x2, __rt_str_repeat_empty");                       // return the canonical empty string when the source has no bytes

    // -- choose concat scratch storage when the repeated result fits --
    emitter.instruction("mul x4, x2, x3");                                      // compute result length = source length * repetition count
    emitter.instruction("str x4, [sp, #24]");                                   // save result length for finalization
    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current concat scratch write offset
    emitter.instruction("add x5, x8, x4");                                      // compute concat scratch end offset after this append
    emitter.instruction("mov x12, #65536");                                     // load concat scratch capacity in bytes
    emitter.instruction("cmp x5, x12");                                         // does the repeated result fit in concat scratch storage?
    emitter.instruction("b.hi __rt_str_repeat_heap");                           // use heap fallback when concat scratch would overflow
    crate::codegen::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // compute concat scratch destination pointer
    emitter.instruction("str x9, [sp, #32]");                                   // save result start pointer for the return pair
    emitter.instruction("str xzr, [sp, #40]");                                  // mark result as concat-backed for final offset publication
    emitter.instruction("b __rt_str_repeat_copy_start");                        // skip heap allocation when scratch storage is enough

    // -- heap fallback for results that do not fit in concat scratch storage --
    emitter.label("__rt_str_repeat_heap");
    emitter.instruction("mov x0, x4");                                          // pass requested payload size to the heap allocator
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate owned storage for the repeated string payload
    emitter.instruction("mov x6, #1");                                          // heap kind 1 = owned elephc string
    emitter.instruction("str x6, [x0, #-8]");                                   // stamp the heap allocation as a string payload
    emitter.instruction("mov x9, x0");                                          // initialize destination cursor at the heap payload start
    emitter.instruction("str x0, [sp, #32]");                                   // save result start pointer for the return pair
    emitter.instruction("mov x13, #1");                                         // mark result as heap-backed so concat offset is left unchanged
    emitter.instruction("str x13, [sp, #40]");                                  // save result storage kind for finalization

    // -- outer loop: repeat N times --
    emitter.label("__rt_str_repeat_copy_start");
    emitter.instruction("ldr x10, [sp, #16]");                                  // initialize repetition counter from the saved repeat count
    emitter.label("__rt_str_repeat_loop");
    emitter.instruction("cbz x10, __rt_str_repeat_done");                       // if counter is 0, done repeating
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload source pointer and length
    emitter.instruction("mov x11, x2");                                         // copy length as inner loop counter

    // -- inner loop: copy one instance of the string --
    emitter.label("__rt_str_repeat_copy");
    emitter.instruction("cbz x11, __rt_str_repeat_next");                       // if no bytes remain, move to next repetition
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte from source, advance src ptr
    emitter.instruction("strb w12, [x9], #1");                                  // store byte to dest, advance dest ptr
    emitter.instruction("sub x11, x11, #1");                                    // decrement inner byte counter
    emitter.instruction("b __rt_str_repeat_copy");                              // continue copying bytes
    emitter.label("__rt_str_repeat_next");
    emitter.instruction("sub x10, x10, #1");                                    // decrement repetition counter
    emitter.instruction("b __rt_str_repeat_loop");                              // continue to next repetition

    // -- finalize: return the repeated string --
    emitter.label("__rt_str_repeat_done");
    emitter.instruction("ldr x1, [sp, #32]");                                   // return the repeated string pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // return the precomputed repeated string length
    emitter.instruction("ldr x13, [sp, #40]");                                  // load storage kind: zero means concat-backed, one means heap-backed
    emitter.instruction("cbnz x13, __rt_str_repeat_return");                    // heap-backed results do not advance concat scratch offset
    crate::codegen::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // reload current concat scratch write offset
    emitter.instruction("add x8, x8, x2");                                      // advance concat scratch offset by the repeated string length
    emitter.instruction("str x8, [x6]");                                        // publish updated concat scratch offset
    emitter.instruction("b __rt_str_repeat_return");                            // skip the empty-string return setup

    // -- empty result: return null pointer with zero length --
    emitter.label("__rt_str_repeat_empty");
    emitter.instruction("mov x1, #0");                                          // empty string has no result payload pointer
    emitter.instruction("mov x2, #0");                                          // empty string length is zero

    // -- restore frame and return --
    emitter.label("__rt_str_repeat_return");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate the repeat-helper stack frame
    emitter.instruction("ret");                                                 // return to caller

    // -- fatal error: negative repetition count --
    emitter.label("__rt_str_repeat_negative_times");
    emitter.instruction("mov x0, #2");                                          // fd = stderr for the negative-repeat diagnostic
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_str_repeat_times_msg");
    emitter.instruction(&format!("mov x2, #{}", STR_REPEAT_TIMES_MSG.len()));   // pass the exact negative-repeat diagnostic byte count
    emitter.syscall(4);
    emitter.instruction("mov x0, #1");                                          // exit code 1 for the negative-repeat abort path
    emitter.syscall(1);
}

/// Emits the `__rt_str_repeat` runtime helper for repeating a string N times on Linux x86_64.
///
/// Uses the standard x86_64 System V ABI: source string in `rax/rdx`, repetition count in `rdi`.
/// Result is returned in `rax/rdx` (pointer/length). Behavior mirrors the ARM64 variant:
/// concat scratch fallback when the result fits within 64 KiB, heap allocation otherwise,
/// fatal error on negative repetition count.
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

    // -- choose concat scratch storage when the repeated result fits --
    emitter.instruction("mov rcx, rdx");                                        // seed result length from the source string length
    emitter.instruction("imul rcx, rdi");                                       // compute result length = source length * repetition count
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save result length for finalization
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load current concat scratch write offset
    emitter.instruction("mov rdx, r9");                                         // copy the current offset before adding the requested result length
    emitter.instruction("add rdx, rcx");                                        // compute concat scratch end offset after this append
    emitter.instruction("cmp rdx, 65536");                                      // does the repeated result fit in concat scratch storage?
    emitter.instruction("ja __rt_str_repeat_heap_linux_x86_64");                // use heap fallback when concat scratch would overflow
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("lea r11, [r10 + r9]");                                 // compute concat scratch destination pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // preserve the repeated-string start pointer for the return pair
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // mark result as concat-backed for final offset publication
    emitter.instruction("jmp __rt_str_repeat_copy_start_linux_x86_64");         // skip heap allocation when scratch storage is enough

    // -- heap fallback for results that do not fit in concat scratch storage --
    emitter.label("__rt_str_repeat_heap_linux_x86_64");
    emitter.instruction("mov rax, rcx");                                        // pass requested payload size to the heap allocator
    emitter.instruction("call __rt_heap_alloc");                                // allocate owned storage for the repeated string payload
    emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 1)); // materialize the owned-string heap kind word with the x86_64 heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the heap allocation as a string payload
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the repeated-string start pointer for the return pair
    emitter.instruction("mov r11, rax");                                        // initialize the heap destination cursor at the result payload start
    emitter.instruction("mov QWORD PTR [rbp - 48], 1");                         // mark result as heap-backed so concat offset is left unchanged

    // -- outer loop: repeat N times --
    emitter.label("__rt_str_repeat_copy_start_linux_x86_64");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // seed the outer repetition counter from the saved repeat-count argument
    emitter.label("__rt_str_repeat_loop_linux_x86_64");
    emitter.instruction("test r10, r10");                                       // stop once every requested repetition has been copied into result storage
    emitter.instruction("jz __rt_str_repeat_done_linux_x86_64");                // jump to finalization when the repeat counter reaches zero
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the source string pointer before copying the next repeated instance
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the source string length before copying the next repeated instance

    // -- inner loop: copy one instance of the string --
    emitter.label("__rt_str_repeat_copy_linux_x86_64");
    emitter.instruction("test rsi, rsi");                                       // stop copying the current source instance once every source byte has been consumed
    emitter.instruction("jz __rt_str_repeat_next_linux_x86_64");                // continue with the next repetition after the full source string has been copied
    emitter.instruction("mov dl, BYTE PTR [r8]");                               // load one source byte before appending it to the result destination cursor
    emitter.instruction("mov BYTE PTR [r11], dl");                              // append one source byte into result storage before advancing both cursors
    emitter.instruction("add r8, 1");                                           // advance the source string pointer after copying one byte from the current repetition
    emitter.instruction("add r11, 1");                                          // advance the result destination cursor after storing one byte from the current repetition
    emitter.instruction("sub rsi, 1");                                          // decrement the remaining source byte count for the current repetition
    emitter.instruction("jmp __rt_str_repeat_copy_linux_x86_64");               // continue copying bytes from the current source string instance until it is exhausted

    emitter.label("__rt_str_repeat_next_linux_x86_64");
    emitter.instruction("sub r10, 1");                                          // decrement the remaining repetition count after copying one full source instance
    emitter.instruction("jmp __rt_str_repeat_loop_linux_x86_64");               // continue with the next repetition until the requested count is exhausted

    // -- finalize: return the repeated string --
    emitter.label("__rt_str_repeat_done_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // return the repeated string pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // return the precomputed repeated string length
    emitter.instruction("mov r8, QWORD PTR [rbp - 48]");                        // load storage kind: zero means concat-backed, one means heap-backed
    emitter.instruction("test r8, r8");                                         // check whether the repeated result used heap fallback
    emitter.instruction("jnz __rt_str_repeat_return_linux_x86_64");             // heap-backed results do not advance concat scratch offset
    crate::codegen::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // reload current concat scratch write offset
    emitter.instruction("add r9, rdx");                                         // advance concat scratch offset by the repeated string length
    emitter.instruction("mov QWORD PTR [r8], r9");                              // publish updated concat scratch offset
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
    crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_str_repeat_times_msg");
    emitter.instruction(&format!("mov edx, {}", STR_REPEAT_TIMES_MSG.len()));   // pass the exact negative-repeat diagnostic byte count
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // emit the fatal negative-repeat message before terminating
    emitter.instruction("mov edi, 1");                                          // exit code 1 for the negative-repeat abort path
    emitter.instruction("mov eax, 60");                                         // Linux x86_64 syscall 60 = exit
    emitter.instruction("syscall");                                             // terminate the process after reporting the invalid repeat count
}
