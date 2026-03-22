use crate::codegen::emit::Emitter;

/// sort_int / rsort_int: insertion sort on integer array (in-place).
/// Input: x0 = array pointer
pub fn emit_sort_int(emitter: &mut Emitter, reverse: bool) {
    let label = if reverse { "__rt_rsort_int" } else { "__rt_sort_int" };
    let cmp_branch = if reverse { "b.ge" } else { "b.le" };

    emitter.blank();
    emitter.comment(&format!("--- runtime: {} ---", label));
    emitter.label(label);

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
