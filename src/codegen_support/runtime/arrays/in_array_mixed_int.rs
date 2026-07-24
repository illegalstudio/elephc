//! Purpose:
//! Emits integer-needle `in_array()` scans for indexed arrays of boxed Mixed cells.
//! Implements PHP loose numeric dispatch and strict runtime-tag equality.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//!
//! Key details:
//! - Slots and their Mixed cells are borrowed; the helper performs no ownership transfer.
//! - ARM64 and Linux x86_64 paths share the same int/float/string/bool/null semantics.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the target-specific integer membership helper for boxed-Mixed arrays.
///
/// Inputs are `(array, needle, strict)` in the first three integer ABI
/// registers. The helper returns `1` when a cell matches and `0` otherwise.
pub fn emit_in_array_mixed_int(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_in_array_mixed_int_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: in_array_mixed_int ---");
    emitter.label_global("__rt_in_array_mixed_int");

    // -- preserve source and comparison state across helper calls --
    emitter.instruction("sub sp, sp, #80");                                     // reserve aligned storage for source, loop state, comparison mode, and saved linkage
    emitter.instruction("stp x29, x30, [sp, #64]");                             // preserve the caller frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish the helper frame pointer above local scan state
    emitter.instruction("str x0, [sp]");                                        // save the boxed-Mixed indexed-array pointer
    emitter.instruction("str x1, [sp, #24]");                                   // save the integer needle across cell-unbox calls
    emitter.instruction("str x2, [sp, #32]");                                   // save whether PHP strict comparison was requested
    emitter.instruction("cbz x0, __rt_in_array_mixed_int_false");               // a null-container pointer has no matching elements
    emitter.instruction("ldr x9, [x0]");                                        // load the source indexed-array logical length
    emitter.instruction("str x9, [sp, #8]");                                    // preserve the logical length across nested runtime calls
    emitter.instruction("str xzr, [sp, #16]");                                  // initialize the scan cursor at index zero

    // -- unbox and compare the current element --
    emitter.label("__rt_in_array_mixed_int_loop");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the current boxed-slot index
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the source logical length
    emitter.instruction("cmp x9, x10");                                         // check whether every boxed slot has been visited
    emitter.instruction("b.ge __rt_in_array_mixed_int_false");                  // return false after exhausting the source array
    emitter.instruction("ldr x10, [sp]");                                       // reload the source indexed-array pointer
    emitter.instruction("add x10, x10, #24");                                   // advance from the header to the boxed Mixed slot region
    emitter.instruction("ldr x0, [x10, x9, lsl #3]");                           // load the borrowed Mixed-cell pointer at the current index
    emitter.instruction("bl __rt_mixed_unbox");                                 // unbox the cell into runtime tag x0 and payload words x1/x2
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the strict-comparison flag
    emitter.instruction("cbnz x9, __rt_in_array_mixed_int_strict");             // strict membership requires an integer tag and identical payload
    emitter.instruction("cmp x0, #0");                                          // does the cell hold an integer?
    emitter.instruction("b.eq __rt_in_array_mixed_int_compare_int");            // integers compare directly with the integer needle
    emitter.instruction("cmp x0, #2");                                          // does the cell hold a floating-point number?
    emitter.instruction("b.eq __rt_in_array_mixed_int_compare_float");          // floats compare numerically after widening the needle
    emitter.instruction("cmp x0, #1");                                          // does the cell hold a string?
    emitter.instruction("b.eq __rt_in_array_mixed_int_compare_string");         // strings participate only when they are complete numeric strings
    emitter.instruction("cmp x0, #3");                                          // does the cell hold a boolean?
    emitter.instruction("b.eq __rt_in_array_mixed_int_compare_bool");           // booleans compare against the needle's PHP truthiness
    emitter.instruction("cmp x0, #8");                                          // does the cell hold PHP null?
    emitter.instruction("b.eq __rt_in_array_mixed_int_compare_null");           // null loosely equals only an integer zero needle
    emitter.instruction("b __rt_in_array_mixed_int_next");                      // arrays, hashes, objects, and resources do not equal an integer

    // -- strict integer equality --
    emitter.label("__rt_in_array_mixed_int_strict");
    emitter.instruction("cmp x0, #0");                                          // strict equality first requires the runtime integer tag
    emitter.instruction("b.ne __rt_in_array_mixed_int_next");                   // every non-integer cell fails strict membership

    emitter.label("__rt_in_array_mixed_int_compare_int");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the integer needle
    emitter.instruction("cmp x1, x9");                                          // compare the integer cell payload with the needle
    emitter.instruction("b.eq __rt_in_array_mixed_int_true");                   // stop after the first equal integer payload
    emitter.instruction("b __rt_in_array_mixed_int_next");                      // continue after an integer mismatch

    // -- loose scalar comparisons --
    emitter.label("__rt_in_array_mixed_int_compare_float");
    emitter.instruction("fmov d0, x1");                                         // move the cell's floating-point payload bits into a float register
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the integer needle for numeric widening
    emitter.instruction("scvtf d1, x9");                                        // widen the signed integer needle to a double
    emitter.instruction("fcmp d0, d1");                                         // compare the cell float with the widened integer needle
    emitter.instruction("b.eq __rt_in_array_mixed_int_true");                   // stop after an ordered numerically-equal float
    emitter.instruction("b __rt_in_array_mixed_int_next");                      // continue after a float mismatch or NaN

    emitter.label("__rt_in_array_mixed_int_compare_string");
    emitter.instruction("bl __rt_str_to_number");                               // parse the unboxed bounded string and return flag x0 plus double d0
    emitter.instruction("cbz x0, __rt_in_array_mixed_int_next");                // PHP 8 non-numeric strings are never equal to an integer
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the integer needle after numeric-string parsing
    emitter.instruction("scvtf d1, x9");                                        // widen the integer needle for the parsed-string comparison
    emitter.instruction("fcmp d0, d1");                                         // compare the parsed numeric string with the integer needle
    emitter.instruction("b.eq __rt_in_array_mixed_int_true");                   // stop after an ordered numerically-equal string
    emitter.instruction("b __rt_in_array_mixed_int_next");                      // continue after a numeric-string mismatch

    emitter.label("__rt_in_array_mixed_int_compare_bool");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the integer needle for PHP truthiness
    emitter.instruction("cmp x9, #0");                                          // determine whether the integer needle is truthy
    emitter.instruction("cset x10, ne");                                        // materialize the needle's PHP boolean value
    emitter.instruction("cmp x1, x10");                                         // compare the cell boolean with the needle truthiness
    emitter.instruction("b.eq __rt_in_array_mixed_int_true");                   // stop after equal boolean truthiness
    emitter.instruction("b __rt_in_array_mixed_int_next");                      // continue after a boolean mismatch

    emitter.label("__rt_in_array_mixed_int_compare_null");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the integer needle for null comparison
    emitter.instruction("cbz x9, __rt_in_array_mixed_int_true");                // PHP loose comparison treats integer zero as equal to null

    // -- advance or return --
    emitter.label("__rt_in_array_mixed_int_next");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the current boxed-slot index
    emitter.instruction("add x9, x9, #1");                                      // advance to the next boxed slot
    emitter.instruction("str x9, [sp, #16]");                                   // preserve the advanced cursor across the next unbox call
    emitter.instruction("b __rt_in_array_mixed_int_loop");                      // continue the membership scan

    emitter.label("__rt_in_array_mixed_int_true");
    emitter.instruction("mov x0, #1");                                          // report that a boxed cell matched the integer needle
    emitter.instruction("b __rt_in_array_mixed_int_done");                      // skip the not-found result after a match

    emitter.label("__rt_in_array_mixed_int_false");
    emitter.instruction("mov x0, #0");                                          // report that no boxed cell matched the integer needle

    emitter.label("__rt_in_array_mixed_int_done");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the caller frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the aligned helper frame and local scan state
    emitter.instruction("ret");                                                 // return the membership boolean to the generated caller
}

/// Emits the Linux x86_64 System V implementation of Mixed-array integer membership.
fn emit_in_array_mixed_int_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: in_array_mixed_int ---");
    emitter.label_global("__rt_in_array_mixed_int");

    // -- preserve source and comparison state across helper calls --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer and align the stack for nested calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for scan locals
    emitter.instruction("sub rsp, 48");                                         // reserve source, length, cursor, needle, and strict-mode slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the boxed-Mixed indexed-array pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // save the integer needle across cell-unbox calls
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // save whether PHP strict comparison was requested
    emitter.instruction("test rdi, rdi");                                       // check whether the source is a null-container pointer
    emitter.instruction("je __rt_in_array_mixed_int_false_x86");                // a null-container pointer has no matching elements
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the source indexed-array logical length
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the logical length across nested runtime calls
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // initialize the scan cursor at index zero

    // -- unbox and compare the current element --
    emitter.label("__rt_in_array_mixed_int_loop_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the current boxed-slot index
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                       // check whether every boxed slot has been visited
    emitter.instruction("jge __rt_in_array_mixed_int_false_x86");               // return false after exhausting the source array
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer
    emitter.instruction("mov rax, QWORD PTR [rax + rcx * 8 + 24]");             // load the borrowed Mixed-cell pointer at the current index
    emitter.instruction("call __rt_mixed_unbox");                               // unbox the cell into runtime tag rax and payload words rdi/rdx
    emitter.instruction("cmp QWORD PTR [rbp - 40], 0");                         // was PHP strict comparison requested?
    emitter.instruction("jne __rt_in_array_mixed_int_strict_x86");              // strict membership requires an integer tag and identical payload
    emitter.instruction("cmp rax, 0");                                          // does the cell hold an integer?
    emitter.instruction("je __rt_in_array_mixed_int_compare_int_x86");          // integers compare directly with the integer needle
    emitter.instruction("cmp rax, 2");                                          // does the cell hold a floating-point number?
    emitter.instruction("je __rt_in_array_mixed_int_compare_float_x86");        // floats compare numerically after widening the needle
    emitter.instruction("cmp rax, 1");                                          // does the cell hold a string?
    emitter.instruction("je __rt_in_array_mixed_int_compare_string_x86");       // strings participate only when they are complete numeric strings
    emitter.instruction("cmp rax, 3");                                          // does the cell hold a boolean?
    emitter.instruction("je __rt_in_array_mixed_int_compare_bool_x86");         // booleans compare against the needle's PHP truthiness
    emitter.instruction("cmp rax, 8");                                          // does the cell hold PHP null?
    emitter.instruction("je __rt_in_array_mixed_int_compare_null_x86");         // null loosely equals only an integer zero needle
    emitter.instruction("jmp __rt_in_array_mixed_int_next_x86");                // arrays, hashes, objects, and resources do not equal an integer

    // -- strict integer equality --
    emitter.label("__rt_in_array_mixed_int_strict_x86");
    emitter.instruction("cmp rax, 0");                                          // strict equality first requires the runtime integer tag
    emitter.instruction("jne __rt_in_array_mixed_int_next_x86");                // every non-integer cell fails strict membership

    emitter.label("__rt_in_array_mixed_int_compare_int_x86");
    emitter.instruction("cmp rdi, QWORD PTR [rbp - 32]");                       // compare the integer cell payload with the needle
    emitter.instruction("je __rt_in_array_mixed_int_true_x86");                 // stop after the first equal integer payload
    emitter.instruction("jmp __rt_in_array_mixed_int_next_x86");                // continue after an integer mismatch

    // -- loose scalar comparisons --
    emitter.label("__rt_in_array_mixed_int_compare_float_x86");
    emitter.instruction("movq xmm0, rdi");                                      // move the cell's floating-point payload bits into a float register
    emitter.instruction("cvtsi2sd xmm1, QWORD PTR [rbp - 32]");                 // widen the signed integer needle to a double
    emitter.instruction("ucomisd xmm0, xmm1");                                  // compare the cell float with the widened integer needle
    emitter.instruction("jp __rt_in_array_mixed_int_next_x86");                 // unordered NaN values never compare equal
    emitter.instruction("je __rt_in_array_mixed_int_true_x86");                 // stop after an ordered numerically-equal float
    emitter.instruction("jmp __rt_in_array_mixed_int_next_x86");                // continue after a float mismatch or NaN

    emitter.label("__rt_in_array_mixed_int_compare_string_x86");
    emitter.instruction("mov rax, rdi");                                        // move the unboxed string pointer into the numeric parser input register
    emitter.instruction("call __rt_str_to_number");                             // parse the bounded string and return flag rax plus double xmm0
    emitter.instruction("test rax, rax");                                       // did the complete string parse as numeric?
    emitter.instruction("je __rt_in_array_mixed_int_next_x86");                 // PHP 8 non-numeric strings are never equal to an integer
    emitter.instruction("cvtsi2sd xmm1, QWORD PTR [rbp - 32]");                 // widen the integer needle for the parsed-string comparison
    emitter.instruction("ucomisd xmm0, xmm1");                                  // compare the parsed numeric string with the integer needle
    emitter.instruction("jp __rt_in_array_mixed_int_next_x86");                 // unordered parsed values never compare equal
    emitter.instruction("je __rt_in_array_mixed_int_true_x86");                 // stop after an ordered numerically-equal string
    emitter.instruction("jmp __rt_in_array_mixed_int_next_x86");                // continue after a numeric-string mismatch

    emitter.label("__rt_in_array_mixed_int_compare_bool_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                         // determine whether the integer needle is truthy
    emitter.instruction("setne al");                                            // materialize the needle's PHP boolean value
    emitter.instruction("movzx rax, al");                                       // widen the needle truthiness result
    emitter.instruction("cmp rdi, rax");                                        // compare the cell boolean with the needle truthiness
    emitter.instruction("je __rt_in_array_mixed_int_true_x86");                 // stop after equal boolean truthiness
    emitter.instruction("jmp __rt_in_array_mixed_int_next_x86");                // continue after a boolean mismatch

    emitter.label("__rt_in_array_mixed_int_compare_null_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                         // compare the integer needle with null's loose numeric zero
    emitter.instruction("je __rt_in_array_mixed_int_true_x86");                 // PHP loose comparison treats integer zero as equal to null

    // -- advance or return --
    emitter.label("__rt_in_array_mixed_int_next_x86");
    emitter.instruction("add QWORD PTR [rbp - 24], 1");                         // advance to the next boxed slot
    emitter.instruction("jmp __rt_in_array_mixed_int_loop_x86");                // continue the membership scan

    emitter.label("__rt_in_array_mixed_int_true_x86");
    emitter.instruction("mov rax, 1");                                          // report that a boxed cell matched the integer needle
    emitter.instruction("jmp __rt_in_array_mixed_int_done_x86");                // skip the not-found result after a match

    emitter.label("__rt_in_array_mixed_int_false_x86");
    emitter.instruction("xor eax, eax");                                        // report that no boxed cell matched the integer needle

    emitter.label("__rt_in_array_mixed_int_done_x86");
    emitter.instruction("mov rsp, rbp");                                        // release all helper-local scan storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the membership boolean to the generated caller
}
