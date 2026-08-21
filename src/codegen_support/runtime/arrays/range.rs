//! Purpose:
//! Emits the `__rt_range`, `__rt_range_descending` runtime helper assembly for range.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Range allocation must size the output array before filling so capacity and heap accounting stay consistent.
//! - The inclusive element count `|end - start| / |step| + 1` is computed in signed 64-bit arithmetic
//!   and can overflow for wide intervals. A real count is always >= 1, so a computed count <= 0 means
//!   the interval wrapped and the range is rejected instead of allocating a mis-sized array.
//! - `__rt_range` takes PHP's `$step` as a THIRD argument (`x2` / `rdx`). Its sign is ignored — the
//!   traversal direction comes from `start` vs `end`, exactly like php-src — and the caller is
//!   responsible for raising PHP's `ValueError`s for a zero step, a negative step on an increasing
//!   range, and a step wider than the spanned interval before calling in.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::data::RANGE_SIZE_MSG;

/// Dispatches to the architecture-specific range-emitter implementation.
pub fn emit_range(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_range_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: range ---");
    emitter.label_global("__rt_range");

    // -- set up stack frame, save arguments --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save start value
    emitter.instruction("str x1, [sp, #8]");                                    // save end value

    // -- normalize the requested step to its traversal magnitude --
    emitter.instruction("cmp x2, #0");                                          // is the requested PHP step negative?
    emitter.instruction("cneg x9, x2, lt");                                     // x9 = |step|, the magnitude every direction walks by
    emitter.instruction("cmp x9, #0");                                          // a zero or unrepresentable magnitude cannot advance the range
    emitter.instruction("b.le __rt_range_size_fail");                           // reject it instead of dividing by zero below

    // -- determine direction and calculate the spanned interval --
    emitter.instruction("cmp x0, x1");                                          // compare start with end
    emitter.instruction("b.gt __rt_range_descending");                          // if start > end, use descending path

    // -- ascending: span = end - start, traversal step = +|step| --
    emitter.instruction("sub x2, x1, x0");                                      // x2 = span = end - start
    emitter.instruction("mov x7, x9");                                          // x7 = step = +|step| (ascending)
    emitter.instruction("b __rt_range_count");                                  // jump to the shared element-count computation

    // -- descending: span = start - end, traversal step = -|step| --
    emitter.label("__rt_range_descending");
    emitter.instruction("sub x2, x0, x1");                                      // x2 = span = start - end
    emitter.instruction("neg x7, x9");                                          // x7 = step = -|step| (descending)

    // -- count = span / |step| + 1 --
    emitter.label("__rt_range_count");
    emitter.instruction("cmp x2, #0");                                          // an inclusive span is never negative
    emitter.instruction("b.lt __rt_range_size_fail");                           // a negative span means the interval overflowed
    emitter.instruction("udiv x2, x2, x9");                                     // x2 = whole steps that fit inside the span
    emitter.instruction("add x2, x2, #1");                                      // x2 = count, the inclusive element count

    // -- allocate array --
    emitter.label("__rt_range_alloc");
    emitter.instruction("cmp x2, #0");                                          // an inclusive range always holds at least one element
    emitter.instruction("b.le __rt_range_size_fail");                           // a non-positive count means the interval overflowed
    emitter.instruction("str x2, [sp, #16]");                                   // save count
    emitter.instruction("str x7, [sp, #8]");                                    // save step (reuse end slot, no longer needed)
    emitter.instruction("mov x0, x2");                                          // x0 = capacity = count
    emitter.instruction("mov x1, #8");                                          // x1 = elem_size = 8 (integers)
    emitter.instruction("bl __rt_array_new");                                   // allocate new array
    emitter.instruction("str x0, [sp, #24]");                                   // save new array pointer

    // -- fill array with values from start, stepping by the signed traversal step --
    emitter.instruction("add x3, x0, #24");                                     // x3 = data base of new array
    emitter.instruction("ldr x4, [sp, #0]");                                    // x4 = current value = start
    emitter.instruction("ldr x5, [sp, #16]");                                   // x5 = count
    emitter.instruction("ldr x7, [sp, #8]");                                    // x7 = signed traversal step
    emitter.instruction("mov x6, #0");                                          // x6 = i = 0

    emitter.label("__rt_range_loop");
    emitter.instruction("cmp x6, x5");                                          // compare i with count
    emitter.instruction("b.ge __rt_range_done");                                // if i >= count, filling complete
    emitter.instruction("str x4, [x3, x6, lsl #3]");                            // data[i] = current value
    emitter.instruction("add x4, x4, x7");                                      // current value += the signed traversal step
    emitter.instruction("add x6, x6, #1");                                      // i += 1
    emitter.instruction("b __rt_range_loop");                                   // continue loop

    // -- set length and return --
    emitter.label("__rt_range_done");
    emitter.instruction("ldr x0, [sp, #24]");                                   // x0 = new array pointer
    emitter.instruction("ldr x9, [sp, #16]");                                   // x9 = count
    emitter.instruction("str x9, [x0]");                                        // set array length = count

    // -- tear down stack frame and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = array [start..end]

    // -- fatal error: the inclusive range does not fit in an array --
    emitter.label("__rt_range_size_fail");
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    abi::emit_symbol_address(emitter, "x1", "_range_size_err_msg");
    emitter.instruction(&format!("mov x2, #{}", RANGE_SIZE_MSG.len()));         // pass the exact range-size diagnostic byte count
    emitter.syscall(4);
    emitter.instruction("mov x0, #1");                                          // exit code 1
    emitter.syscall(1);
}

/// Emits the x86_64 Linux implementation of `__rt_range` for both ascending and descending integer ranges.
/// Input: rdi = start (inclusive), rsi = end (inclusive), rdx = step (sign ignored, magnitude used)
/// Output: rax = pointer to new indexed array containing values from start to end
/// Uses rbp-based frame with spill slots for start, end, count, traversal step, and array pointer.
/// Preserves 16-byte stack alignment for nested calls.
fn emit_range_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: range ---");
    emitter.label_global("__rt_range");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving range-construction spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for start, end, count, step, and destination array bookkeeping
    emitter.instruction("sub rsp, 48");                                         // ROUNDED UP to a 16-byte multiple: `push rbp` already landed rsp on a 16-byte boundary, so reserving an odd multiple of 8 here would leave every `call` in this body misaligned and hand the callee a stack SysV x86_64 forbids. Pinned by `every_x86_64_runtime_call_site_is_sysv_aligned`; see arrays/array_free_deep.rs for the curl SIGSEGV that class of bug produced.
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the inclusive range start value across count calculation and destination-array allocation
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the inclusive range end value across count calculation and destination-array allocation
    emitter.instruction("mov r10, rdx");                                        // copy the requested PHP step before normalizing it to a traversal magnitude
    emitter.instruction("mov r11, r10");                                        // stage the negated requested step for the conditional magnitude select
    emitter.instruction("neg r11");                                             // negate the requested step so a negative one yields its magnitude
    emitter.instruction("test r10, r10");                                       // is the requested PHP step negative?
    emitter.instruction("cmovs r10, r11");                                      // r10 = |step|, the magnitude every direction walks by
    emitter.instruction("cmp r10, 0");                                          // a zero or unrepresentable magnitude cannot advance the range
    emitter.instruction("jle __rt_range_size_fail");                            // reject it instead of dividing by zero below
    emitter.instruction("cmp rdi, rsi");                                        // compare the inclusive range start and end values to choose the traversal direction
    emitter.instruction("jg __rt_range_descending_x86");                        // switch to the descending range path when the start value is greater than the end value
    emitter.instruction("mov rax, rsi");                                        // copy the inclusive range end value before subtracting the start value to derive the spanned interval
    emitter.instruction("sub rax, rdi");                                        // compute end - start for the ascending integer range
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // preserve the ascending traversal step so the fill loop can advance by +|step|
    emitter.instruction("jmp __rt_range_count_x86");                            // jump to the shared element-count computation after preparing the ascending span and step
    emitter.label("__rt_range_descending_x86");
    emitter.instruction("mov rax, rdi");                                        // copy the inclusive range start value before subtracting the end value to derive the spanned interval
    emitter.instruction("sub rax, rsi");                                        // compute start - end for the descending integer range
    emitter.instruction("mov r11, r10");                                        // stage the traversal magnitude before negating it for the descending direction
    emitter.instruction("neg r11");                                             // negate the traversal magnitude so the fill loop walks downwards
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // preserve the descending traversal step so the fill loop can advance by -|step|
    emitter.label("__rt_range_count_x86");
    emitter.instruction("cmp rax, 0");                                          // an inclusive span is never negative
    emitter.instruction("jl __rt_range_size_fail");                             // a negative span means the interval overflowed
    emitter.instruction("xor edx, edx");                                        // clear the high dividend word before the unsigned span division
    emitter.instruction("div r10");                                             // rax = whole steps that fit inside the span
    emitter.instruction("add rax, 1");                                          // convert the whole-step count into the inclusive element count
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the computed element count across destination-array allocation
    emitter.label("__rt_range_alloc_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // pass the final integer range length as the destination indexed-array capacity to the constructor
    emitter.instruction("cmp rdi, 0");                                          // an inclusive range always holds at least one element
    emitter.instruction("jle __rt_range_size_fail");                            // a non-positive count means the interval overflowed
    emitter.instruction("mov rsi, 8");                                          // use 8-byte payload slots because the range helper produces an indexed array of integers
    emitter.instruction("call __rt_array_new");                                 // allocate the destination integer range array through the shared x86_64 indexed-array constructor
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the destination integer range array pointer while the fill loop writes payload slots
    emitter.instruction("lea r8, [rax + 24]");                                  // compute the destination integer range payload base address once before entering the fill loop
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the current integer value from the inclusive range start before entering the fill loop
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the final integer range element count before entering the fill loop
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the signed traversal step before entering the fill loop
    emitter.instruction("xor rcx, rcx");                                        // initialize the range fill loop index to the first destination payload slot
    emitter.label("__rt_range_loop_x86");
    emitter.instruction("cmp rcx, r10");                                        // compare the current range fill loop index against the final element count
    emitter.instruction("jge __rt_range_done_x86");                             // stop once every destination integer payload slot has been initialized
    emitter.instruction("mov QWORD PTR [r8 + rcx * 8], r9");                    // store the current integer value into the selected destination range payload slot
    emitter.instruction("add r9, r11");                                         // advance the current integer value by the preserved signed traversal step for the next payload slot
    emitter.instruction("add rcx, 1");                                          // advance the range fill loop index after initializing one destination payload slot
    emitter.instruction("jmp __rt_range_loop_x86");                             // continue filling integer range payload slots until the inclusive interval is exhausted
    emitter.label("__rt_range_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the destination integer range array pointer before publishing the final logical length
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the computed integer range element count before publishing the final logical length
    emitter.instruction("mov QWORD PTR [rax], r10");                            // publish the final logical length in the destination integer range array header
    emitter.instruction("add rsp, 48");                                         // release the range-construction spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the constructed integer range array pointer in rax

    // -- fatal error: the inclusive range does not fit in an array --
    emitter.label("__rt_range_size_fail");
    emitter.instruction("mov edi, 2");                                          // fd = stderr for the range-size fatal error message
    abi::emit_symbol_address(emitter, "rsi", "_range_size_err_msg");
    emitter.instruction(&format!("mov edx, {}", RANGE_SIZE_MSG.len()));         // pass the exact range-size diagnostic byte count
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // print the fatal range-size message to stderr
    emitter.instruction("mov edi, 1");                                          // exit code 1 for an unrepresentable range size
    emitter.instruction("mov eax, 60");                                         // Linux x86_64 syscall 60 = exit
    emitter.instruction("syscall");                                             // terminate the process after reporting the range-size failure
}
