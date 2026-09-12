//! Purpose:
//! Emits scalar and boxed `array_multisort` runtime helpers over two parallel indexed arrays.
//! Orders rows by the first array and uses the second array to break equal-key ties.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - The scalar helper compares signed integer slots directly.
//! - The boxed helper calls the shared PHP Mixed comparator after codegen validates scalar tags.
//! - Both helpers reject unequal lengths before moving any slot owner.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// array_multisort: sort arr1 ascending in place, reordering arr2 in tandem.
/// Input:  x0 = arr1 pointer (primary sort key), x1 = arr2 pointer (reordered to match)
/// Output: x0 = one on success, zero for unequal lengths
///
/// Lexicographic tandem bubble sort: adjacent rows are compared by arr1 and then arr2, and both
/// rows move together. Returns one for success and zero when the array lengths differ.
pub fn emit_array_multisort(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_multisort_linux_x86_64(emitter);
        emit_array_multisort_boxed_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_multisort ---");
    emitter.label_global("__rt_array_multisort");
    emitter.instruction("ldr x9, [x0]");                                        // x9 = arr1 length (used for both arrays)
    emitter.instruction("ldr x8, [x1]");                                        // x8 = arr2 length for the consistency check
    emitter.instruction("cmp x9, x8");                                          // PHP requires every sorted array to have equal length
    emitter.instruction("b.ne __rt_array_multisort_mismatch");                  // reject before moving either array
    emitter.instruction("add x10, x0, #24");                                    // x10 = arr1 data base (skip header)
    emitter.instruction("add x11, x1, #24");                                    // x11 = arr2 data base (skip header)
    emitter.instruction("cmp x9, #2");                                          // arrays shorter than 2 elements are already sorted
    emitter.instruction("b.lt __rt_array_multisort_done");                      // nothing to sort
    emitter.label("__rt_array_multisort_outer");
    emitter.instruction("mov x12, #0");                                         // swapped flag = 0 for this pass
    emitter.instruction("mov x13, #0");                                         // inner index j = 0
    emitter.instruction("sub x14, x9, #1");                                     // x14 = length - 1 (last comparable index)
    emitter.label("__rt_array_multisort_inner");
    emitter.instruction("cmp x13, x14");                                        // has j reached length - 1?
    emitter.instruction("b.ge __rt_array_multisort_pass_end");                  // end of this bubble pass
    emitter.instruction("add x16, x13, #1");                                    // x16 = j + 1
    emitter.instruction("ldr x15, [x10, x13, lsl #3]");                         // x15 = arr1[j]
    emitter.instruction("ldr x17, [x10, x16, lsl #3]");                         // x17 = arr1[j+1]
    emitter.instruction("cmp x15, x17");                                        // compare the primary keys for these adjacent rows
    emitter.instruction("b.gt __rt_array_multisort_swap");                      // a descending primary pair must move together
    emitter.instruction("b.lt __rt_array_multisort_no_swap");                   // an ascending primary pair is already ordered
    emitter.instruction("ldr x5, [x11, x13, lsl #3]");                          // equal primary keys are ordered by arr2[j]
    emitter.instruction("ldr x6, [x11, x16, lsl #3]");                          // load arr2[j+1] as the secondary key
    emitter.instruction("cmp x5, x6");                                          // compare the secondary keys for an equal-primary pair
    emitter.instruction("b.le __rt_array_multisort_no_swap");                   // keep rows whose full key tuple is ordered or equal
    emitter.label("__rt_array_multisort_swap");
    emitter.instruction("str x17, [x10, x13, lsl #3]");                         // swap: arr1[j] = old arr1[j+1]
    emitter.instruction("str x15, [x10, x16, lsl #3]");                         // swap: arr1[j+1] = old arr1[j]
    emitter.instruction("cmp x10, x11");                                        // identical receivers already moved their shared row once
    emitter.instruction("b.eq __rt_array_multisort_swapped");                   // do not undo the primary swap through the same payload
    emitter.instruction("ldr x15, [x11, x13, lsl #3]");                         // x15 = arr2[j]
    emitter.instruction("ldr x17, [x11, x16, lsl #3]");                         // x17 = arr2[j+1]
    emitter.instruction("str x17, [x11, x13, lsl #3]");                         // tandem swap: arr2[j] = old arr2[j+1]
    emitter.instruction("str x15, [x11, x16, lsl #3]");                         // tandem swap: arr2[j+1] = old arr2[j]
    emitter.label("__rt_array_multisort_swapped");
    emitter.instruction("mov x12, #1");                                         // mark that a swap happened this pass
    emitter.label("__rt_array_multisort_no_swap");
    emitter.instruction("add x13, x13, #1");                                    // advance the inner index
    emitter.instruction("b __rt_array_multisort_inner");                        // continue the bubble pass
    emitter.label("__rt_array_multisort_pass_end");
    emitter.instruction("cbnz x12, __rt_array_multisort_outer");                // repeat passes until no swaps occur
    emitter.label("__rt_array_multisort_done");
    emitter.instruction("mov x0, #1");                                          // report successful sorting, including empty arrays
    emitter.instruction("ret");                                                 // return with both arrays sorted in place
    emitter.label("__rt_array_multisort_mismatch");
    emitter.instruction("mov x0, #0");                                          // report inconsistent array sizes without moving slots
    emitter.instruction("ret");                                                 // let codegen raise the catchable ValueError
    emit_array_multisort_boxed_aarch64(emitter);
}

/// x86_64 Linux implementation of `__rt_array_multisort`.
/// Input:  rdi = arr1 pointer, rsi = arr2 pointer
/// Output: rax = one on success, zero for unequal lengths
fn emit_array_multisort_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_multisort ---");
    emitter.label_global("__rt_array_multisort");
    emitter.instruction("mov r9, QWORD PTR [rdi]");                             // r9 = arr1 length (used for both arrays)
    emitter.instruction("cmp r9, QWORD PTR [rsi]");                             // PHP requires every sorted array to have equal length
    emitter.instruction("jne __rt_array_multisort_mismatch");                   // reject before moving either array
    emitter.instruction("lea r10, [rdi + 24]");                                 // r10 = arr1 data base (skip header)
    emitter.instruction("lea r11, [rsi + 24]");                                 // r11 = arr2 data base (skip header)
    emitter.instruction("cmp r9, 2");                                           // arrays shorter than 2 elements are already sorted
    emitter.instruction("jl __rt_array_multisort_done");                        // nothing to sort
    emitter.label("__rt_array_multisort_outer");
    emitter.instruction("xor r8, r8");                                          // swapped flag = 0 for this pass
    emitter.instruction("xor rax, rax");                                        // inner index j = 0
    emitter.label("__rt_array_multisort_inner");
    emitter.instruction("mov rcx, r9");                                         // copy the length
    emitter.instruction("sub rcx, 1");                                          // rcx = length - 1 (last comparable index)
    emitter.instruction("cmp rax, rcx");                                        // has j reached length - 1?
    emitter.instruction("jge __rt_array_multisort_pass_end");                   // end of this bubble pass
    emitter.instruction("mov rcx, QWORD PTR [r10 + rax * 8]");                  // rcx = arr1[j]
    emitter.instruction("mov rdx, QWORD PTR [r10 + rax * 8 + 8]");              // rdx = arr1[j+1]
    emitter.instruction("cmp rcx, rdx");                                        // compare the primary keys for these adjacent rows
    emitter.instruction("jg __rt_array_multisort_swap");                        // a descending primary pair must move together
    emitter.instruction("jl __rt_array_multisort_no_swap");                     // an ascending primary pair is already ordered
    emitter.instruction("mov rdi, QWORD PTR [r11 + rax * 8]");                  // equal primary keys are ordered by arr2[j]
    emitter.instruction("mov rsi, QWORD PTR [r11 + rax * 8 + 8]");              // load arr2[j+1] as the secondary key
    emitter.instruction("cmp rdi, rsi");                                        // compare the secondary keys for an equal-primary pair
    emitter.instruction("jle __rt_array_multisort_no_swap");                    // keep rows whose full key tuple is ordered or equal
    emitter.label("__rt_array_multisort_swap");
    emitter.instruction("mov QWORD PTR [r10 + rax * 8], rdx");                  // swap: arr1[j] = old arr1[j+1]
    emitter.instruction("mov QWORD PTR [r10 + rax * 8 + 8], rcx");              // swap: arr1[j+1] = old arr1[j]
    emitter.instruction("cmp r10, r11");                                        // identical receivers already moved their shared row once
    emitter.instruction("je __rt_array_multisort_swapped");                     // do not undo the primary swap through the same payload
    emitter.instruction("mov rcx, QWORD PTR [r11 + rax * 8]");                  // rcx = arr2[j]
    emitter.instruction("mov rdx, QWORD PTR [r11 + rax * 8 + 8]");              // rdx = arr2[j+1]
    emitter.instruction("mov QWORD PTR [r11 + rax * 8], rdx");                  // tandem swap: arr2[j] = old arr2[j+1]
    emitter.instruction("mov QWORD PTR [r11 + rax * 8 + 8], rcx");              // tandem swap: arr2[j+1] = old arr2[j]
    emitter.label("__rt_array_multisort_swapped");
    emitter.instruction("mov r8, 1");                                           // mark that a swap happened this pass
    emitter.label("__rt_array_multisort_no_swap");
    emitter.instruction("add rax, 1");                                          // advance the inner index
    emitter.instruction("jmp __rt_array_multisort_inner");                      // continue the bubble pass
    emitter.label("__rt_array_multisort_pass_end");
    emitter.instruction("test r8, r8");                                         // did any swap happen this pass?
    emitter.instruction("jnz __rt_array_multisort_outer");                      // repeat passes until no swaps occur
    emitter.label("__rt_array_multisort_done");
    emitter.instruction("mov eax, 1");                                          // report successful sorting, including empty arrays
    emitter.instruction("ret");                                                 // return with both arrays sorted in place
    emitter.label("__rt_array_multisort_mismatch");
    emitter.instruction("xor eax, eax");                                        // report inconsistent array sizes without moving slots
    emitter.instruction("ret");                                                 // let codegen raise the catchable ValueError
}

/// Emits the AArch64 tandem sorter for two indexed arrays of boxed Mixed cells.
///
/// Inputs are array payload pointers in x0 and x1. Codegen has already normalized both arrays
/// to pointer-sized Mixed slots and rejected non-scalar tags. The helper borrows every cell,
/// swaps slot owners without changing their counts, and returns one or zero like the scalar helper.
fn emit_array_multisort_boxed_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_multisort_boxed ---");
    emitter.label_global("__rt_array_multisort_boxed");
    emitter.instruction("sub sp, sp, #80");                                     // reserve sorter state and the saved frame record
    emitter.instruction("stp x29, x30, [sp, #64]");                             // preserve the caller across Mixed comparisons
    emitter.instruction("add x29, sp, #64");                                    // establish an aligned helper frame
    emitter.instruction("stp x0, x1, [sp]");                                    // retain both borrowed array payload pointers
    emitter.instruction("ldr x9, [x0]");                                        // load the primary array length
    emitter.instruction("ldr x10, [x1]");                                       // load the secondary array length
    emitter.instruction("cmp x9, x10");                                         // every row must have a value in both arrays
    emitter.instruction("b.ne __rt_array_multisort_boxed_mismatch");            // reject unequal lengths before moving cell owners
    emitter.instruction("str x9, [sp, #16]");                                   // preserve the common length across comparator calls
    emitter.instruction("cmp x9, #2");                                          // zero and one-row inputs are already ordered
    emitter.instruction("b.lt __rt_array_multisort_boxed_done");                // return success without reading any slot

    emitter.label("__rt_array_multisort_boxed_outer");
    emitter.instruction("str xzr, [sp, #24]");                                  // clear the swapped flag for this bubble pass
    emitter.instruction("str xzr, [sp, #32]");                                  // begin with the first adjacent row pair
    emitter.label("__rt_array_multisort_boxed_inner");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the current left row index
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the common row count
    emitter.instruction("sub x10, x10, #1");                                    // the final row has no right-hand neighbor
    emitter.instruction("cmp x9, x10");                                         // has this pass compared every adjacent pair?
    emitter.instruction("b.hs __rt_array_multisort_boxed_pass_end");            // inspect whether another pass is required
    emitter.instruction("add x10, x9, #1");                                     // form the right row index
    emitter.instruction("ldr x11, [sp]");                                       // reload the primary array payload
    emitter.instruction("add x11, x11, #24");                                   // advance to its Mixed slot base
    emitter.instruction("ldr x0, [x11, x9, lsl #3]");                           // borrow the left primary Mixed cell
    emitter.instruction("ldr x1, [x11, x10, lsl #3]");                          // borrow the right primary Mixed cell
    emitter.instruction("stp x0, x1, [sp, #40]");                               // retain both primary cells across comparisons
    emitter.instruction("bl __rt_php_compare_slots");                           // apply PHP ordering to the primary keys
    emitter.instruction("cmp x0, #0");                                          // classify the primary comparison result
    emitter.instruction("b.gt __rt_array_multisort_boxed_swap");                // descending primary keys require a tandem move
    emitter.instruction("b.lt __rt_array_multisort_boxed_no_swap");             // ascending primary keys already determine the row order
    emitter.instruction("ldr x9, [sp, #32]");                                   // recover the left row index for the secondary lookup
    emitter.instruction("add x10, x9, #1");                                     // recover the adjacent right row index
    emitter.instruction("ldr x11, [sp, #8]");                                   // equal primary keys defer to the secondary array
    emitter.instruction("add x11, x11, #24");                                   // advance to the secondary Mixed slot base
    emitter.instruction("ldr x0, [x11, x9, lsl #3]");                           // borrow the left secondary Mixed cell
    emitter.instruction("ldr x1, [x11, x10, lsl #3]");                          // borrow the right secondary Mixed cell
    emitter.instruction("bl __rt_php_compare_slots");                           // apply PHP ordering to the secondary keys
    emitter.instruction("cmp x0, #0");                                          // equal full tuples stay stable
    emitter.instruction("b.le __rt_array_multisort_boxed_no_swap");             // ordered or equal secondary keys do not move

    // -- exchange both complete rows without changing Mixed cell ownership --
    emitter.label("__rt_array_multisort_boxed_swap");
    emitter.instruction("ldr x9, [sp, #32]");                                   // recover the left row index after comparator calls
    emitter.instruction("add x10, x9, #1");                                     // recover the adjacent right row index
    emitter.instruction("ldr x11, [sp]");                                       // reload the primary array payload
    emitter.instruction("add x11, x11, #24");                                   // advance to its Mixed slot base
    emitter.instruction("ldp x12, x13, [sp, #40]");                             // reload the borrowed primary cell pointers
    emitter.instruction("str x13, [x11, x9, lsl #3]");                          // transfer the right primary owner into the left slot
    emitter.instruction("str x12, [x11, x10, lsl #3]");                         // transfer the left primary owner into the right slot
    emitter.instruction("ldp x12, x13, [sp]");                                  // reload both payloads before considering the secondary swap
    emitter.instruction("cmp x12, x13");                                        // the same reference must exchange each pair only once
    emitter.instruction("b.eq __rt_array_multisort_boxed_swapped");             // skip the second exchange when the payloads coincide
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the secondary array payload
    emitter.instruction("add x11, x11, #24");                                   // advance to its Mixed slot base
    emitter.instruction("ldr x12, [x11, x9, lsl #3]");                          // load the left secondary cell owner
    emitter.instruction("ldr x13, [x11, x10, lsl #3]");                         // load the right secondary cell owner
    emitter.instruction("str x13, [x11, x9, lsl #3]");                          // transfer the right secondary owner into the left slot
    emitter.instruction("str x12, [x11, x10, lsl #3]");                         // transfer the left secondary owner into the right slot
    emitter.label("__rt_array_multisort_boxed_swapped");
    emitter.instruction("mov x11, #1");                                         // mark that this pass changed row order
    emitter.instruction("str x11, [sp, #24]");                                  // preserve the flag across later comparisons

    emitter.label("__rt_array_multisort_boxed_no_swap");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the completed left row index
    emitter.instruction("add x9, x9, #1");                                      // advance to the next adjacent pair
    emitter.instruction("str x9, [sp, #32]");                                   // preserve the next index across comparator calls
    emitter.instruction("b __rt_array_multisort_boxed_inner");                  // continue the current bubble pass
    emitter.label("__rt_array_multisort_boxed_pass_end");
    emitter.instruction("ldr x9, [sp, #24]");                                   // inspect whether this pass moved any row
    emitter.instruction("cbnz x9, __rt_array_multisort_boxed_outer");           // repeat until every tuple is ordered
    emitter.label("__rt_array_multisort_boxed_done");
    emitter.instruction("mov x0, #1");                                          // report successful sorting, including empty arrays
    emitter.instruction("b __rt_array_multisort_boxed_return");                 // share the balanced frame epilogue
    emitter.label("__rt_array_multisort_boxed_mismatch");
    emitter.instruction("mov x0, #0");                                          // report unequal lengths without moving any cell owner
    emitter.label("__rt_array_multisort_boxed_return");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the caller's frame and continuation
    emitter.instruction("add sp, sp, #80");                                     // release all sorter state
    emitter.instruction("ret");                                                 // return the status to codegen
}

/// Emits the System V x86_64 tandem sorter for two indexed arrays of boxed Mixed cells.
fn emit_array_multisort_boxed_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_multisort_boxed ---");
    emitter.label_global("__rt_array_multisort_boxed");
    emitter.instruction("push rbp");                                            // preserve the caller's frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame across Mixed comparisons
    emitter.instruction("sub rsp, 64");                                         // reserve sorter state with SysV call alignment
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // retain the borrowed primary array payload
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // retain the borrowed secondary array payload
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the primary array length
    emitter.instruction("cmp r10, QWORD PTR [rsi]");                            // every row must have a value in both arrays
    emitter.instruction("jne __rt_array_multisort_boxed_mismatch");             // reject unequal lengths before moving cell owners
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // preserve the common length across comparator calls
    emitter.instruction("cmp r10, 2");                                          // zero and one-row inputs are already ordered
    emitter.instruction("jl __rt_array_multisort_boxed_done");                  // return success without reading any slot

    emitter.label("__rt_array_multisort_boxed_outer");
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // clear the swapped flag for this bubble pass
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // begin with the first adjacent row pair
    emitter.label("__rt_array_multisort_boxed_inner");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the current left row index
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the common row count
    emitter.instruction("sub r11, 1");                                          // the final row has no right-hand neighbor
    emitter.instruction("cmp r10, r11");                                        // has this pass compared every adjacent pair?
    emitter.instruction("jae __rt_array_multisort_boxed_pass_end");             // inspect whether another pass is required
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the primary array payload
    emitter.instruction("mov rdi, QWORD PTR [r11 + r10 * 8 + 24]");             // borrow the left primary Mixed cell
    emitter.instruction("mov rsi, QWORD PTR [r11 + r10 * 8 + 32]");             // borrow the right primary Mixed cell
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // retain the left primary cell across comparisons
    emitter.instruction("mov QWORD PTR [rbp - 56], rsi");                       // retain the right primary cell across comparisons
    emitter.instruction("call __rt_php_compare_slots");                         // apply PHP ordering to the primary keys
    emitter.instruction("cmp rax, 0");                                          // classify the primary comparison result
    emitter.instruction("jg __rt_array_multisort_boxed_swap");                  // descending primary keys require a tandem move
    emitter.instruction("jl __rt_array_multisort_boxed_no_swap");               // ascending primary keys already determine the row order
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // recover the left row index for the secondary lookup
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // equal primary keys defer to the secondary array
    emitter.instruction("mov rdi, QWORD PTR [r11 + r10 * 8 + 24]");             // borrow the left secondary Mixed cell
    emitter.instruction("mov rsi, QWORD PTR [r11 + r10 * 8 + 32]");             // borrow the right secondary Mixed cell
    emitter.instruction("call __rt_php_compare_slots");                         // apply PHP ordering to the secondary keys
    emitter.instruction("cmp rax, 0");                                          // equal full tuples stay stable
    emitter.instruction("jle __rt_array_multisort_boxed_no_swap");              // ordered or equal secondary keys do not move

    // -- exchange both complete rows without changing Mixed cell ownership --
    emitter.label("__rt_array_multisort_boxed_swap");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // recover the left row index after comparator calls
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the primary array payload
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload the left primary cell owner
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // reload the right primary cell owner
    emitter.instruction("mov QWORD PTR [r11 + r10 * 8 + 24], rsi");             // transfer the right primary owner into the left slot
    emitter.instruction("mov QWORD PTR [r11 + r10 * 8 + 32], rdi");             // transfer the left primary owner into the right slot
    emitter.instruction("cmp r11, QWORD PTR [rbp - 16]");                       // the same reference must exchange each pair only once
    emitter.instruction("je __rt_array_multisort_boxed_swapped");               // skip the second exchange when the payloads coincide
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the secondary array payload
    emitter.instruction("mov rdi, QWORD PTR [r11 + r10 * 8 + 24]");             // load the left secondary cell owner
    emitter.instruction("mov rsi, QWORD PTR [r11 + r10 * 8 + 32]");             // load the right secondary cell owner
    emitter.instruction("mov QWORD PTR [r11 + r10 * 8 + 24], rsi");             // transfer the right secondary owner into the left slot
    emitter.instruction("mov QWORD PTR [r11 + r10 * 8 + 32], rdi");             // transfer the left secondary owner into the right slot
    emitter.label("__rt_array_multisort_boxed_swapped");
    emitter.instruction("mov QWORD PTR [rbp - 32], 1");                         // mark that this pass changed row order

    emitter.label("__rt_array_multisort_boxed_no_swap");
    emitter.instruction("add QWORD PTR [rbp - 40], 1");                         // advance to the next adjacent pair
    emitter.instruction("jmp __rt_array_multisort_boxed_inner");                // continue the current bubble pass
    emitter.label("__rt_array_multisort_boxed_pass_end");
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                         // inspect whether this pass moved any row
    emitter.instruction("jne __rt_array_multisort_boxed_outer");                // repeat until every tuple is ordered
    emitter.label("__rt_array_multisort_boxed_done");
    emitter.instruction("mov eax, 1");                                          // report successful sorting, including empty arrays
    emitter.instruction("jmp __rt_array_multisort_boxed_return");               // share the balanced frame epilogue
    emitter.label("__rt_array_multisort_boxed_mismatch");
    emitter.instruction("xor eax, eax");                                        // report unequal lengths without moving any cell owner
    emitter.label("__rt_array_multisort_boxed_return");
    emitter.instruction("mov rsp, rbp");                                        // release all sorter state
    emitter.instruction("pop rbp");                                             // restore the caller's frame pointer
    emitter.instruction("ret");                                                 // return the status to codegen
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every supported target emits tuple comparison, length rejection and ownership-neutral swaps.
    #[test]
    fn boxed_multisort_emits_complete_two_array_semantics_on_every_target() {
        for name in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_array_multisort(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_array_multisort_boxed:"), "{name}");
            assert!(asm.matches("__rt_php_compare_slots").count() >= 2, "{name}");
            assert!(asm.contains("__rt_array_multisort_boxed_mismatch"), "{name}");
            assert!(asm.contains("__rt_array_multisort_mismatch"), "{name}");
            if name == "linux-x86_64" {
                assert!(asm.contains("je __rt_array_multisort_swapped"), "{name}");
                assert!(asm.contains("je __rt_array_multisort_boxed_swapped"), "{name}");
                assert!(asm.contains("mov QWORD PTR [r11 + r10 * 8 + 24], rsi"), "{name}");
                assert!(asm.contains("mov QWORD PTR [r11 + r10 * 8 + 32], rdi"), "{name}");
            } else {
                assert!(asm.contains("b.eq __rt_array_multisort_swapped"), "{name}");
                assert!(asm.contains("b.eq __rt_array_multisort_boxed_swapped"), "{name}");
                assert!(asm.contains("str x13, [x11, x9, lsl #3]"), "{name}");
                assert!(asm.contains("str x12, [x11, x10, lsl #3]"), "{name}");
            }
        }
    }
}
