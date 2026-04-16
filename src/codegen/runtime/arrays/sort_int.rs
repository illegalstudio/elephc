use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// sort_int / rsort_int: insertion sort on integer array (in-place).
/// Input: x0 = array pointer
pub fn emit_sort_int(emitter: &mut Emitter, reverse: bool) {
    if emitter.target.arch == Arch::X86_64 {
        emit_sort_int_linux_x86_64(emitter, reverse);
        return;
    }

    let label = if reverse { "__rt_rsort_int" } else { "__rt_sort_int" };
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
    emitter.instruction("ldr x4, [x2, x3, lsl #3]");                            // x4 = key = data[i] (element to insert)
    emitter.instruction("sub x5, x3, #1");                                      // x5 = j = i - 1 (scan backwards from here)

    // -- inner loop: shift elements right until correct position found --
    emitter.label(&inner);
    emitter.instruction("cmp x5, #0");                                          // check if j < 0
    emitter.instruction(&format!("b.lt {}", insert));                           // if j < 0, insert position found
    emitter.instruction("ldr x6, [x2, x5, lsl #3]");                            // x6 = data[j] (element to compare with key)
    emitter.instruction("cmp x6, x4");                                          // compare data[j] with key
    emitter.instruction(&format!("{} {}", cmp_branch, insert));                 // if in order (<= or >= for reverse), insert here
    emitter.instruction("add x7, x5, #1");                                      // x7 = j + 1 (destination for shift)
    emitter.instruction("str x6, [x2, x7, lsl #3]");                            // data[j+1] = data[j] (shift element right)
    emitter.instruction("sub x5, x5, #1");                                      // j -= 1 (move left)
    emitter.instruction(&format!("b {}", inner));                               // continue inner loop

    // -- insert the key at the correct position --
    emitter.label(&insert);
    emitter.instruction("add x7, x5, #1");                                      // x7 = j + 1 (insertion index)
    emitter.instruction("str x4, [x2, x7, lsl #3]");                            // data[j+1] = key (place element)
    emitter.instruction("add x3, x3, #1");                                      // i += 1 (advance outer loop)
    emitter.instruction(&format!("b {}", outer));                               // continue outer loop

    // -- sorting complete --
    emitter.label(&done);
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_sort_int_linux_x86_64(emitter: &mut Emitter, reverse: bool) {
    let label = if reverse { "__rt_rsort_int" } else { "__rt_sort_int" };
    let break_jump = if reverse {
        "__rt_sort_int_break_desc"
    } else {
        "__rt_sort_int_break_asc"
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
    emitter.instruction("mov r11, QWORD PTR [r9 + r10 * 8]");                   // load the current indexed-array element as the insertion-sort key
    emitter.instruction("mov rcx, r10");                                        // copy the outer-loop cursor before stepping left through the sorted prefix
    emitter.instruction("sub rcx, 1");                                          // initialize the insertion-sort inner-loop cursor to the element immediately left of the key

    emitter.label(&inner);
    emitter.instruction("cmp rcx, -1");                                         // has the insertion-sort inner-loop cursor stepped past the start of the indexed array?
    emitter.instruction(&format!("je {}", insert));                             // insert the key at slot zero once the inner loop runs off the left edge
    emitter.instruction("mov rdx, QWORD PTR [r9 + rcx * 8]");                   // load the current sorted-prefix element that is being compared against the key
    emitter.instruction("cmp rdx, r11");                                        // compare the current sorted-prefix element against the insertion-sort key
    if reverse {
        emitter.instruction(&format!("jge {}", break_jump));                    // stop shifting once the descending-order invariant is satisfied for the current slot
    } else {
        emitter.instruction(&format!("jle {}", break_jump));                    // stop shifting once the ascending-order invariant is satisfied for the current slot
    }
    emitter.instruction("mov QWORD PTR [r9 + rcx * 8 + 8], rdx");               // shift the current sorted-prefix element one slot to the right to make room for the key
    emitter.instruction("sub rcx, 1");                                          // move the insertion-sort inner-loop cursor one slot further left through the sorted prefix
    emitter.instruction(&format!("jmp {}", inner));                             // continue scanning left until the correct insertion point is found

    emitter.label(break_jump);
    emitter.instruction(&format!("jmp {}", insert));                            // jump into the shared insertion path once the sorted-prefix comparison says the key belongs here

    emitter.label(&insert);
    emitter.instruction("mov QWORD PTR [r9 + rcx * 8 + 8], r11");               // store the insertion-sort key into the slot immediately to the right of the final inner-loop cursor
    emitter.instruction("add r10, 1");                                          // advance the insertion-sort outer-loop cursor to the next indexed-array element
    emitter.instruction(&format!("jmp {}", outer));                             // continue insertion-sorting the remaining indexed-array suffix

    emitter.label(&done);
    emitter.instruction("ret");                                                 // return after sorting the indexed-array payload in place
}
