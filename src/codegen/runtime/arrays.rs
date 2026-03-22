use crate::codegen::emit::Emitter;

/// heap_alloc: bump allocator.
/// Input: x0 = bytes needed
/// Output: x0 = pointer to allocated memory
pub fn emit_heap_alloc(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: heap_alloc ---");
    emitter.label("__rt_heap_alloc");

    // -- load the current heap offset from the global variable --
    emitter.instruction("adrp x9, _heap_off@PAGE");                             // load page base of _heap_off into x9
    emitter.instruction("add x9, x9, _heap_off@PAGEOFF");                       // add page offset to get exact address of _heap_off
    emitter.instruction("ldr x10, [x9]");                                       // x10 = current heap offset (bytes used so far)

    // -- compute the base address of the heap buffer --
    emitter.instruction("adrp x11, _heap_buf@PAGE");                            // load page base of _heap_buf into x11
    emitter.instruction("add x11, x11, _heap_buf@PAGEOFF");                     // add page offset to get exact address of _heap_buf

    // -- bump the allocator: return current position, advance offset --
    emitter.instruction("add x12, x11, x10");                                   // x12 = heap_buf + offset = pointer to free memory
    emitter.instruction("add x10, x10, x0");                                    // advance offset by requested byte count
    emitter.instruction("str x10, [x9]");                                       // store updated offset back to _heap_off
    emitter.instruction("mov x0, x12");                                         // return the allocated pointer in x0
    emitter.instruction("ret");                                                 // return to caller
}

/// array_new: create a new array on the heap.
/// Input: x0 = capacity, x1 = element size (8 or 16)
/// Output: x0 = pointer to array header
/// Layout: [length:8][capacity:8][elem_size:8][elements...]
pub fn emit_array_new(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_new ---");
    emitter.label("__rt_array_new");

    // -- set up stack frame, save arguments for use after heap_alloc call --
    emitter.instruction("sub sp, sp, #32");                                     // allocate 32 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save capacity to stack (need it after bl)
    emitter.instruction("str x1, [sp, #8]");                                    // save elem_size to stack (need it after bl)

    // -- calculate total bytes needed: 24-byte header + (capacity * elem_size) --
    emitter.instruction("mul x2, x0, x1");                                      // x2 = capacity * elem_size = data region size
    emitter.instruction("add x0, x2, #24");                                     // x0 = data size + 24-byte header
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate memory, x0 = pointer to array

    // -- initialize the array header fields --
    emitter.instruction("str xzr, [x0]");                                       // header[0]: length = 0 (array starts empty)
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload capacity from stack
    emitter.instruction("str x9, [x0, #8]");                                    // header[8]: capacity = original x0 arg
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload elem_size from stack
    emitter.instruction("str x9, [x0, #16]");                                   // header[16]: elem_size = original x1 arg

    // -- tear down stack frame and return --
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = array pointer
}

/// array_push_int: push an integer element to an array.
/// Input: x0 = array pointer, x1 = value
pub fn emit_array_push_int(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_push_int ---");
    emitter.label("__rt_array_push_int");

    // -- store the integer at the next available slot --
    emitter.instruction("ldr x9, [x0]");                                        // x9 = current array length
    emitter.instruction("add x10, x0, #24");                                    // x10 = base of data region (skip 24-byte header)
    emitter.instruction("str x1, [x10, x9, lsl #3]");                           // store value at data[length * 8] (8 bytes per int)

    // -- increment the array length --
    emitter.instruction("add x9, x9, #1");                                      // length += 1
    emitter.instruction("str x9, [x0]");                                        // write updated length back to header
    emitter.instruction("ret");                                                 // return to caller
}

/// array_push_str: push a string element (ptr+len) to an array.
/// Input: x0 = array pointer, x1 = str ptr, x2 = str len
pub fn emit_array_push_str(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_push_str ---");
    emitter.label("__rt_array_push_str");

    // -- compute address of the next string slot (16 bytes per element) --
    emitter.instruction("ldr x9, [x0]");                                        // x9 = current array length
    emitter.instruction("lsl x10, x9, #4");                                     // x10 = length * 16 (byte offset, 16 bytes per string)
    emitter.instruction("add x10, x0, x10");                                    // x10 = array base + byte offset
    emitter.instruction("add x10, x10, #24");                                   // x10 = skip 24-byte header to reach data region

    // -- store the string pointer and length as a pair --
    emitter.instruction("str x1, [x10]");                                       // store string pointer at slot[0..8]
    emitter.instruction("str x2, [x10, #8]");                                   // store string length at slot[8..16]

    // -- increment the array length --
    emitter.instruction("add x9, x9, #1");                                      // length += 1
    emitter.instruction("str x9, [x0]");                                        // write updated length back to header
    emitter.instruction("ret");                                                 // return to caller
}

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
