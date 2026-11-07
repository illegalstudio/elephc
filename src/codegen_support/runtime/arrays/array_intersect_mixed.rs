//! Purpose:
//! Emits the mixed-element `array_intersect` runtime helpers that compare boxed
//! PHP values by their string rendering instead of by cell address.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - `__rt_array_intersect_mixed` scans two indexed arrays whose 8-byte slots hold
//!   boxed Mixed cells; `__rt_array_intersect_mixed_str` scans a boxed first array
//!   against a 16-byte string second array. Both preserve first-array order and
//!   retain each matched box for the result.
//! - `__rt_mixed_cast_string` may hand back the boxed value's live string or shared
//!   scratch; the intersect helpers compare it and never release it, matching the
//!   existing assoc set-operation contract.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_array_intersect_mixed` (two boxed Mixed arrays).
///
/// Input (AArch64): `x0` = first indexed array, `x1` = second indexed array.
/// Input (x86_64): `rdi` = first indexed array, `rsi` = second indexed array.
/// Output: the integer result register points at a new indexed array of retained
/// Mixed cells.
pub fn emit_array_intersect_mixed(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_intersect_mixed_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_intersect_mixed ---");
    emitter.label_global("__rt_array_intersect_mixed");

    // -- frame: inputs, result, outer index, candidate text, inner scan state --
    emitter.instruction("sub sp, sp, #128");                                    // reserve the mixed intersect stack frame
    emitter.instruction("stp x29, x30, [sp, #112]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #112");                                   // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the first indexed-array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the second indexed-array pointer
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the first array capacity for the result allocation
    emitter.instruction("mov x1, #8");                                          // mixed result slots are pointer-sized boxed cells
    emitter.instruction("bl __rt_array_new");                                   // allocate the result indexed array
    emitter.instruction("str x0, [sp, #16]");                                   // save the result array pointer
    emitter.instruction("str xzr, [sp, #24]");                                  // start the outer scan at the first slot

    emitter.label("__rt_array_isect_mixed_outer");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the first indexed-array pointer
    emitter.instruction("ldr x3, [x0]");                                        // load the first array logical length
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the outer scan index
    emitter.instruction("cmp x4, x3");                                          // has every first-array element been considered?
    emitter.instruction("b.ge __rt_array_isect_mixed_done");                    // finish once the first array is exhausted
    emitter.instruction("add x5, x0, #24");                                     // compute the first array payload base
    emitter.instruction("ldr x6, [x5, x4, lsl #3]");                            // load the candidate boxed Mixed cell
    emitter.instruction("str x6, [sp, #40]");                                   // save the candidate for the result append
    emitter.instruction("mov x0, x6");                                          // pass the candidate box to the string cast
    emitter.instruction("bl __rt_mixed_cast_string");                           // render the candidate as PHP would for array_intersect
    emitter.instruction("str x1, [sp, #32]");                                   // save the borrowed or owned candidate text pointer
    emitter.instruction("str x2, [sp, #48]");                                   // save the candidate text length
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the second indexed-array pointer
    emitter.instruction("ldr x7, [x1]");                                        // load the second array logical length
    emitter.instruction("str x7, [sp, #56]");                                   // save the inner scan bound
    emitter.instruction("add x8, x1, #24");                                     // compute the second array payload base
    emitter.instruction("str x8, [sp, #64]");                                   // save the inner payload base
    emitter.instruction("str xzr, [sp, #72]");                                  // start the inner scan at the first slot

    emitter.label("__rt_array_isect_mixed_inner");
    emitter.instruction("ldr x9, [sp, #72]");                                   // reload the inner scan index
    emitter.instruction("ldr x7, [sp, #56]");                                   // reload the second array length
    emitter.instruction("cmp x9, x7");                                          // has the full second array been scanned?
    emitter.instruction("b.ge __rt_array_isect_mixed_after_inner");             // stop scanning once no element can match
    emitter.instruction("ldr x8, [sp, #64]");                                   // reload the second array payload base
    emitter.instruction("ldr x10, [x8, x9, lsl #3]");                           // load the current second-array boxed cell
    emitter.instruction("mov x0, x10");                                         // pass the second box to the string cast
    emitter.instruction("bl __rt_mixed_cast_string");                           // render the second value by its PHP string form
    emitter.instruction("mov x3, x1");                                          // move the second text pointer into the equality argument
    emitter.instruction("mov x4, x2");                                          // move the second text length into the equality argument
    emitter.instruction("ldr x1, [sp, #32]");                                   // reload the candidate text pointer
    emitter.instruction("ldr x2, [sp, #48]");                                   // reload the candidate text length
    emitter.instruction("bl __rt_str_eq");                                      // compare the two string renderings by value
    emitter.instruction("cbnz x0, __rt_array_isect_mixed_match");               // append the candidate once a value matches

    emitter.label("__rt_array_isect_mixed_next_inner");
    emitter.instruction("ldr x9, [sp, #72]");                                   // reload the inner scan index
    emitter.instruction("add x9, x9, #1");                                      // advance to the next second-array slot
    emitter.instruction("str x9, [sp, #72]");                                   // persist the inner scan index
    emitter.instruction("b __rt_array_isect_mixed_inner");                      // continue scanning the second array

    emitter.label("__rt_array_isect_mixed_match");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the result array pointer
    emitter.instruction("ldr x1, [sp, #40]");                                   // reload the matched boxed candidate
    emitter.instruction("bl __rt_array_push_refcounted");                       // retain and append the matched boxed cell
    emitter.instruction("str x0, [sp, #16]");                                   // persist the result pointer after possible growth

    emitter.label("__rt_array_isect_mixed_after_inner");
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the outer scan index
    emitter.instruction("add x4, x4, #1");                                      // advance to the next first-array slot
    emitter.instruction("str x4, [sp, #24]");                                   // persist the outer scan index
    emitter.instruction("b __rt_array_isect_mixed_outer");                      // continue scanning the first array

    emitter.label("__rt_array_isect_mixed_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the result array pointer
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // release the mixed intersect stack frame
    emitter.instruction("ret");                                                 // return the result array in the integer result register
}

/// Emits `__rt_array_intersect_mixed_str`: a boxed Mixed first array compared against
/// a 16-byte string second array.
///
/// Input (AArch64): `x0` = first indexed array, `x1` = second string array.
/// Input (x86_64): `rdi` = first indexed array, `rsi` = second string array.
/// Output: the integer result register points at a new indexed array of retained
/// Mixed cells.
pub fn emit_array_intersect_mixed_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_intersect_mixed_str_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_intersect_mixed_str ---");
    emitter.label_global("__rt_array_intersect_mixed_str");

    // -- frame: inputs, result, outer index, candidate text, inner scan state --
    emitter.instruction("sub sp, sp, #112");                                    // reserve the mixed/string intersect stack frame
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the first indexed-array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the second string-array pointer
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the first array capacity for the result allocation
    emitter.instruction("mov x1, #8");                                          // mixed result slots are pointer-sized boxed cells
    emitter.instruction("bl __rt_array_new");                                   // allocate the result indexed array
    emitter.instruction("str x0, [sp, #16]");                                   // save the result array pointer
    emitter.instruction("str xzr, [sp, #24]");                                  // start the outer scan at the first slot

    emitter.label("__rt_array_isect_mixed_str_outer");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the first indexed-array pointer
    emitter.instruction("ldr x3, [x0]");                                        // load the first array logical length
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the outer scan index
    emitter.instruction("cmp x4, x3");                                          // has every first-array element been considered?
    emitter.instruction("b.ge __rt_array_isect_mixed_str_done");                // finish once the first array is exhausted
    emitter.instruction("add x5, x0, #24");                                     // compute the first array payload base
    emitter.instruction("ldr x6, [x5, x4, lsl #3]");                            // load the candidate boxed Mixed cell
    emitter.instruction("str x6, [sp, #40]");                                   // save the candidate for the result append
    emitter.instruction("mov x0, x6");                                          // pass the candidate box to the string cast
    emitter.instruction("bl __rt_mixed_cast_string");                           // render the candidate as PHP would for array_intersect
    emitter.instruction("str x1, [sp, #32]");                                   // save the borrowed or owned candidate text pointer
    emitter.instruction("str x2, [sp, #48]");                                   // save the candidate text length
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the second string-array pointer
    emitter.instruction("ldr x7, [x1]");                                        // load the second array logical length
    emitter.instruction("str x7, [sp, #56]");                                   // save the inner scan bound
    emitter.instruction("add x8, x1, #24");                                     // compute the second array payload base
    emitter.instruction("str x8, [sp, #64]");                                   // save the inner payload base
    emitter.instruction("str xzr, [sp, #72]");                                  // start the inner scan at the first slot

    emitter.label("__rt_array_isect_mixed_str_inner");
    emitter.instruction("ldr x9, [sp, #72]");                                   // reload the inner scan index
    emitter.instruction("ldr x7, [sp, #56]");                                   // reload the second array length
    emitter.instruction("cmp x9, x7");                                          // has the full string array been scanned?
    emitter.instruction("b.ge __rt_array_isect_mixed_str_after_inner");         // stop scanning once no element can match
    emitter.instruction("ldr x8, [sp, #64]");                                   // reload the second array payload base
    emitter.instruction("lsl x10, x9, #4");                                     // scale the inner index by the 16-byte string slot
    emitter.instruction("add x10, x8, x10");                                    // address the current string slot
    emitter.instruction("ldr x3, [x10]");                                       // load the second string pointer
    emitter.instruction("ldr x4, [x10, #8]");                                   // load the second string length
    emitter.instruction("ldr x1, [sp, #32]");                                   // reload the candidate text pointer
    emitter.instruction("ldr x2, [sp, #48]");                                   // reload the candidate text length
    emitter.instruction("bl __rt_str_eq");                                      // compare the two string renderings by value
    emitter.instruction("cbnz x0, __rt_array_isect_mixed_str_match");           // append the candidate once a value matches

    emitter.label("__rt_array_isect_mixed_str_next_inner");
    emitter.instruction("ldr x9, [sp, #72]");                                   // reload the inner scan index
    emitter.instruction("add x9, x9, #1");                                      // advance to the next string slot
    emitter.instruction("str x9, [sp, #72]");                                   // persist the inner scan index
    emitter.instruction("b __rt_array_isect_mixed_str_inner");                  // continue scanning the string array

    emitter.label("__rt_array_isect_mixed_str_match");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the result array pointer
    emitter.instruction("ldr x1, [sp, #40]");                                   // reload the matched boxed candidate
    emitter.instruction("bl __rt_array_push_refcounted");                       // retain and append the matched boxed cell
    emitter.instruction("str x0, [sp, #16]");                                   // persist the result pointer after possible growth

    emitter.label("__rt_array_isect_mixed_str_after_inner");
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the outer scan index
    emitter.instruction("add x4, x4, #1");                                      // advance to the next first-array slot
    emitter.instruction("str x4, [sp, #24]");                                   // persist the outer scan index
    emitter.instruction("b __rt_array_isect_mixed_str_outer");                  // continue scanning the first array

    emitter.label("__rt_array_isect_mixed_str_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the result array pointer
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // release the mixed/string intersect stack frame
    emitter.instruction("ret");                                                 // return the result array in the integer result register
}

/// Emits the x86_64 Linux variant of `__rt_array_intersect_mixed`.
fn emit_array_intersect_mixed_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_intersect_mixed ---");
    emitter.label_global("__rt_array_intersect_mixed");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 128");                                        // reserve the mixed intersect stack frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the first indexed-array pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the second indexed-array pointer
    emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");                        // load the first array capacity for the result allocation
    emitter.instruction("mov rsi, 8");                                          // mixed result slots are pointer-sized boxed cells
    emitter.instruction("call __rt_array_new");                                 // allocate the result indexed array
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the result array pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // start the outer scan at the first slot

    emitter.label("__rt_array_isect_mixed_outer_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the outer scan index
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the first indexed-array pointer
    emitter.instruction("cmp rcx, QWORD PTR [r10]");                            // has every first-array element been considered?
    emitter.instruction("jge __rt_array_isect_mixed_done_x86");                 // finish once the first array is exhausted
    emitter.instruction("lea r11, [r10 + 24]");                                 // compute the first array payload base
    emitter.instruction("mov r8, QWORD PTR [r11 + rcx * 8]");                   // load the candidate boxed Mixed cell
    emitter.instruction("mov QWORD PTR [rbp - 56], r8");                        // save the candidate for the result append
    emitter.instruction("mov rax, r8");                                         // pass the box in the string-cast result register
    emitter.instruction("mov rdi, r8");                                         // pass the candidate box to the string cast
    emitter.instruction("call __rt_mixed_cast_string");                         // render the candidate as PHP would for array_intersect
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the borrowed or owned candidate text pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // save the candidate text length
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the second indexed-array pointer
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the second array logical length
    emitter.instruction("mov QWORD PTR [rbp - 64], r11");                       // save the inner scan bound
    emitter.instruction("lea r10, [r10 + 24]");                                 // compute the second array payload base
    emitter.instruction("mov QWORD PTR [rbp - 72], r10");                       // save the inner payload base
    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // start the inner scan at the first slot

    emitter.label("__rt_array_isect_mixed_inner_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 80]");                        // reload the inner scan index
    emitter.instruction("cmp r9, QWORD PTR [rbp - 64]");                        // has the full second array been scanned?
    emitter.instruction("jge __rt_array_isect_mixed_after_inner_x86");          // stop scanning once no element can match
    emitter.instruction("mov r10, QWORD PTR [rbp - 72]");                       // reload the second array payload base
    emitter.instruction("mov r8, QWORD PTR [r10 + r9 * 8]");                    // load the current second-array boxed cell
    emitter.instruction("mov rax, r8");                                         // pass the box in the string-cast result register
    emitter.instruction("mov rdi, r8");                                         // pass the second box to the string cast
    emitter.instruction("call __rt_mixed_cast_string");                         // render the second value by its PHP string form
    emitter.instruction("mov rcx, rdx");                                        // move the second text length into the equality argument
    emitter.instruction("mov rdx, rax");                                        // move the second text pointer into the equality argument
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the candidate text pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // reload the candidate text length
    emitter.instruction("call __rt_str_eq");                                    // compare the two string renderings by value
    emitter.instruction("test rax, rax");                                       // did the two values match?
    emitter.instruction("jne __rt_array_isect_mixed_match_x86");                // append the candidate once a value matches

    emitter.label("__rt_array_isect_mixed_next_inner_x86");
    emitter.instruction("add QWORD PTR [rbp - 80], 1");                         // advance to the next second-array slot
    emitter.instruction("jmp __rt_array_isect_mixed_inner_x86");                // continue scanning the second array

    emitter.label("__rt_array_isect_mixed_match_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the result array pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // reload the matched boxed candidate
    emitter.instruction("call __rt_array_push_refcounted");                     // retain and append the matched boxed cell
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // persist the result pointer after possible growth

    emitter.label("__rt_array_isect_mixed_after_inner_x86");
    emitter.instruction("add QWORD PTR [rbp - 32], 1");                         // advance to the next first-array slot
    emitter.instruction("jmp __rt_array_isect_mixed_outer_x86");                // continue scanning the first array

    emitter.label("__rt_array_isect_mixed_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the result array pointer
    emitter.instruction("mov rsp, rbp");                                        // release the mixed intersect stack frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the result array in rax
}

/// Emits the x86_64 Linux variant of `__rt_array_intersect_mixed_str`.
fn emit_array_intersect_mixed_str_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_intersect_mixed_str ---");
    emitter.label_global("__rt_array_intersect_mixed_str");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 112");                                        // reserve the mixed/string intersect stack frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the first indexed-array pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the second string-array pointer
    emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");                        // load the first array capacity for the result allocation
    emitter.instruction("mov rsi, 8");                                          // mixed result slots are pointer-sized boxed cells
    emitter.instruction("call __rt_array_new");                                 // allocate the result indexed array
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the result array pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // start the outer scan at the first slot

    emitter.label("__rt_array_isect_mixed_str_outer_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the outer scan index
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the first indexed-array pointer
    emitter.instruction("cmp rcx, QWORD PTR [r10]");                            // has every first-array element been considered?
    emitter.instruction("jge __rt_array_isect_mixed_str_done_x86");             // finish once the first array is exhausted
    emitter.instruction("lea r11, [r10 + 24]");                                 // compute the first array payload base
    emitter.instruction("mov r8, QWORD PTR [r11 + rcx * 8]");                   // load the candidate boxed Mixed cell
    emitter.instruction("mov QWORD PTR [rbp - 56], r8");                        // save the candidate for the result append
    emitter.instruction("mov rax, r8");                                         // pass the box in the string-cast result register
    emitter.instruction("mov rdi, r8");                                         // pass the candidate box to the string cast
    emitter.instruction("call __rt_mixed_cast_string");                         // render the candidate as PHP would for array_intersect
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the borrowed or owned candidate text pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // save the candidate text length
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the second string-array pointer
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the second array logical length
    emitter.instruction("mov QWORD PTR [rbp - 64], r11");                       // save the inner scan bound
    emitter.instruction("lea r10, [r10 + 24]");                                 // compute the second array payload base
    emitter.instruction("mov QWORD PTR [rbp - 72], r10");                       // save the inner payload base
    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // start the inner scan at the first slot

    emitter.label("__rt_array_isect_mixed_str_inner_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 80]");                        // reload the inner scan index
    emitter.instruction("cmp r9, QWORD PTR [rbp - 64]");                        // has the full string array been scanned?
    emitter.instruction("jge __rt_array_isect_mixed_str_after_inner_x86");      // stop scanning once no element can match
    emitter.instruction("mov r10, QWORD PTR [rbp - 72]");                       // reload the second array payload base
    emitter.instruction("mov rax, r9");                                         // copy the inner index before scaling it
    emitter.instruction("shl rax, 4");                                          // scale the inner index by the 16-byte string slot
    emitter.instruction("add r10, rax");                                        // address the current string slot
    emitter.instruction("mov rdx, QWORD PTR [r10]");                            // load the second string pointer
    emitter.instruction("mov rcx, QWORD PTR [r10 + 8]");                        // load the second string length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the candidate text pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // reload the candidate text length
    emitter.instruction("call __rt_str_eq");                                    // compare the two string renderings by value
    emitter.instruction("test rax, rax");                                       // did the two values match?
    emitter.instruction("jne __rt_array_isect_mixed_str_match_x86");            // append the candidate once a value matches

    emitter.label("__rt_array_isect_mixed_str_next_inner_x86");
    emitter.instruction("add QWORD PTR [rbp - 80], 1");                         // advance to the next string slot
    emitter.instruction("jmp __rt_array_isect_mixed_str_inner_x86");            // continue scanning the string array

    emitter.label("__rt_array_isect_mixed_str_match_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the result array pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // reload the matched boxed candidate
    emitter.instruction("call __rt_array_push_refcounted");                     // retain and append the matched boxed cell
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // persist the result pointer after possible growth

    emitter.label("__rt_array_isect_mixed_str_after_inner_x86");
    emitter.instruction("add QWORD PTR [rbp - 32], 1");                         // advance to the next first-array slot
    emitter.instruction("jmp __rt_array_isect_mixed_str_outer_x86");            // continue scanning the first array

    emitter.label("__rt_array_isect_mixed_str_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the result array pointer
    emitter.instruction("mov rsp, rbp");                                        // release the mixed/string intersect stack frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the result array in rax
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Pins the x86_64 string-cast input register on all three call sites.
    ///
    /// `__rt_mixed_cast_string` unboxes from the integer result register (`rax`) on x86_64.
    /// The intersect loops overwrite `rax` with the previous string pointer, so each cast must
    /// publish its box there first; passing only the documented `rdi` argument unboxes a string
    /// pointer as a Mixed cell and answers empty strings or crashes.
    #[test]
    fn x86_intersect_helpers_publish_the_box_to_the_string_cast() {
        let mut emitter = Emitter::new(Target::parse("linux-x86_64").unwrap());
        emit_array_intersect_mixed(&mut emitter);
        emit_array_intersect_mixed_str(&mut emitter);
        let asm = emitter.output();
        assert_eq!(
            asm.matches("mov rax, r8\n    mov rdi, r8\n    call __rt_mixed_cast_string")
                .count(),
            3,
            "{asm}"
        );
    }
}
