//! Purpose:
//! Emits the `__rt_rsort_float`, `__rt_sort_float` runtime helper assembly for sort float.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Sort helpers mutate array payload order in place; float slots are compared as IEEE doubles
//!   (`fcmp`/`ucomisd`) rather than as raw 64-bit integers, so negative and fractional values order
//!   correctly. NaN ordering is unspecified (as in PHP) but never loops or traps.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_sort_float` (ascending) or `__rt_rsort_float` (descending) runtime helper.
/// Uses ARM64 instructions on non-x86_64 targets; delegates to `emit_sort_float_linux_x86_64` for x86_64.
///
/// # Arguments
/// * `emitter` — the assembly emitter with target and context
/// * `reverse` — `false` for ascending sort, `true` for descending sort
///
/// # ABI
/// * Input: array pointer in `x0`
/// * Output: sorted array in-place (no return value register)
/// * Temporaries: integer `x1`–`x7`, float `d4`/`d6`
///
/// # Algorithm
/// Insertion sort over 8-byte double slots: for each element, shift larger/smaller elements right
/// until the correct insertion position is found. Comparisons use floating-point ordering so the
/// IEEE sign bit and fractional bits do not corrupt the order the way an integer compare would.
pub fn emit_sort_float(emitter: &mut Emitter, reverse: bool) {
    if emitter.target.arch == Arch::X86_64 {
        emit_sort_float_linux_x86_64(emitter, reverse);
        return;
    }

    let label = if reverse { "__rt_rsort_float" } else { "__rt_sort_float" };
    let cmp_branch = if reverse { "b.ge" } else { "b.le" };

    emitter.blank();
    emitter.comment(&format!("--- runtime: {} ---", label));
    emitter.label_global(label);

    // -- load array metadata and set up outer loop --
    emitter.instruction("ldr x1, [x0]");                                        // x1 = array length from header
    emitter.instruction("add x2, x0, #24");                                     // x2 = base of data region (skip header)
    emitter.instruction("mov x3, #1");                                          // x3 = i = 1 (outer loop index, start at second element)

    let outer = format!("{}_outer", label);
    let inner = format!("{}_inner", label);
    let insert = format!("{}_insert", label);
    let done = format!("{}_done", label);

    // -- outer loop: iterate i from 1 to length-1 --
    emitter.label(&outer);
    emitter.instruction("cmp x3, x1");                                          // compare i with array length
    emitter.instruction(&format!("b.ge {}", done));                             // if i >= length, sorting is complete
    emitter.instruction("ldr d4, [x2, x3, lsl #3]");                            // d4 = key = data[i] (double element to insert)
    emitter.instruction("sub x5, x3, #1");                                      // x5 = j = i - 1 (scan backwards from here)

    // -- inner loop: shift elements right until correct position found --
    emitter.label(&inner);
    emitter.instruction("cmp x5, #0");                                          // check if j < 0
    emitter.instruction(&format!("b.lt {}", insert));                           // if j < 0, insert position found
    emitter.instruction("ldr d6, [x2, x5, lsl #3]");                            // d6 = data[j] (double element to compare with key)
    emitter.instruction("fcmp d6, d4");                                         // compare data[j] with key as IEEE doubles
    emitter.instruction(&format!("{} {}", cmp_branch, insert));                 // if in order (<= or >= for reverse), insert here
    emitter.instruction("add x7, x5, #1");                                      // x7 = j + 1 (destination for shift)
    emitter.instruction("str d6, [x2, x7, lsl #3]");                            // data[j+1] = data[j] (shift element right)
    emitter.instruction("sub x5, x5, #1");                                      // j -= 1 (move left)
    emitter.instruction(&format!("b {}", inner));                               // continue inner loop

    // -- insert the key at the correct position --
    emitter.label(&insert);
    emitter.instruction("add x7, x5, #1");                                      // x7 = j + 1 (insertion index)
    emitter.instruction("str d4, [x2, x7, lsl #3]");                            // data[j+1] = key (place element)
    emitter.instruction("add x3, x3, #1");                                      // i += 1 (advance outer loop)
    emitter.instruction(&format!("b {}", outer));                               // continue outer loop

    // -- sorting complete --
    emitter.label(&done);
    emitter.instruction("ret");                                                 // return to caller
}

/// x86_64 Linux implementation of the sort-float runtime helpers.
/// Uses System V AMD64 ABI: array pointer in `rdi`, indexed-array length in `[rdi]`.
/// Mirrors the ARM64 behavior but compares double slots with `ucomisd` and the unsigned
/// `jbe`/`jae` branches that match its float-ordering flags.
fn emit_sort_float_linux_x86_64(emitter: &mut Emitter, reverse: bool) {
    let label = if reverse { "__rt_rsort_float" } else { "__rt_sort_float" };
    let break_jump = if reverse {
        "__rt_sort_float_break_desc"
    } else {
        "__rt_sort_float_break_asc"
    };

    emitter.blank();
    emitter.comment(&format!("--- runtime: {} ---", label));
    emitter.label_global(label);

    emitter.instruction("mov r8, QWORD PTR [rdi]");                             // load the indexed-array logical length before starting the insertion-sort outer loop
    emitter.instruction("lea r9, [rdi + 24]");                                  // compute the indexed-array payload base pointer once so the loop body can index slots directly
    emitter.instruction("mov r10, 1");                                          // initialize the insertion-sort outer-loop cursor to the second element

    let outer = format!("{}_outer", label);
    let inner = format!("{}_inner", label);
    let insert = format!("{}_insert", label);
    let done = format!("{}_done", label);

    emitter.label(&outer);
    emitter.instruction("cmp r10, r8");                                         // compare the insertion-sort outer-loop cursor against the indexed-array logical length
    emitter.instruction(&format!("jae {}", done));                              // stop once every indexed-array element has been inserted into the sorted prefix
    emitter.instruction("movsd xmm0, QWORD PTR [r9 + r10 * 8]");                // load the current indexed-array element as the insertion-sort key (double)
    emitter.instruction("mov rcx, r10");                                        // copy the outer-loop cursor before stepping left through the sorted prefix
    emitter.instruction("sub rcx, 1");                                          // initialize the insertion-sort inner-loop cursor to the element immediately left of the key

    emitter.label(&inner);
    emitter.instruction("cmp rcx, -1");                                         // has the insertion-sort inner-loop cursor stepped past the start of the indexed array?
    emitter.instruction(&format!("je {}", insert));                             // insert the key at slot zero once the inner loop runs off the left edge
    emitter.instruction("movsd xmm1, QWORD PTR [r9 + rcx * 8]");                // load the current sorted-prefix element that is being compared against the key
    emitter.instruction("ucomisd xmm1, xmm0");                                  // compare the current sorted-prefix element against the key using float ordering
    if reverse {
        emitter.instruction(&format!("jae {}", break_jump));                    // stop shifting once the descending-order invariant is satisfied for the current slot
    } else {
        emitter.instruction(&format!("jbe {}", break_jump));                    // stop shifting once the ascending-order invariant is satisfied for the current slot
    }
    emitter.instruction("movsd QWORD PTR [r9 + rcx * 8 + 8], xmm1");            // shift the current sorted-prefix element one slot to the right to make room for the key
    emitter.instruction("sub rcx, 1");                                          // move the insertion-sort inner-loop cursor one slot further left through the sorted prefix
    emitter.instruction(&format!("jmp {}", inner));                             // continue scanning left until the correct insertion point is found

    emitter.label(break_jump);
    emitter.instruction(&format!("jmp {}", insert));                            // jump into the shared insertion path once the sorted-prefix comparison says the key belongs here

    emitter.label(&insert);
    emitter.instruction("movsd QWORD PTR [r9 + rcx * 8 + 8], xmm0");            // store the insertion-sort key into the slot immediately to the right of the final inner-loop cursor
    emitter.instruction("add r10, 1");                                          // advance the insertion-sort outer-loop cursor to the next indexed-array element
    emitter.instruction(&format!("jmp {}", outer));                             // continue insertion-sorting the remaining indexed-array suffix

    emitter.label(&done);
    emitter.instruction("ret");                                                 // return after sorting the indexed-array payload in place
}
