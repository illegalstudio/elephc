use crate::codegen::emit::Emitter;

/// heap_alloc: bump allocator.
/// Input: x0 = bytes needed
/// Output: x0 = pointer to allocated memory
pub fn emit_heap_alloc(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: heap_alloc ---");
    emitter.label("__rt_heap_alloc");
    emitter.instruction("adrp x9, _heap_off@PAGE");
    emitter.instruction("add x9, x9, _heap_off@PAGEOFF");
    emitter.instruction("ldr x10, [x9]");
    emitter.instruction("adrp x11, _heap_buf@PAGE");
    emitter.instruction("add x11, x11, _heap_buf@PAGEOFF");
    emitter.instruction("add x12, x11, x10");
    emitter.instruction("add x10, x10, x0");
    emitter.instruction("str x10, [x9]");
    emitter.instruction("mov x0, x12");
    emitter.instruction("ret");
}

/// array_new: create a new array on the heap.
/// Input: x0 = capacity, x1 = element size (8 or 16)
/// Output: x0 = pointer to array header
/// Layout: [length:8][capacity:8][elem_size:8][elements...]
pub fn emit_array_new(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_new ---");
    emitter.label("__rt_array_new");
    emitter.instruction("sub sp, sp, #32");
    emitter.instruction("stp x29, x30, [sp, #16]");
    emitter.instruction("add x29, sp, #16");
    emitter.instruction("str x0, [sp, #0]");
    emitter.instruction("str x1, [sp, #8]");
    emitter.instruction("mul x2, x0, x1");
    emitter.instruction("add x0, x2, #24");
    emitter.instruction("bl __rt_heap_alloc");
    emitter.instruction("str xzr, [x0]");
    emitter.instruction("ldr x9, [sp, #0]");
    emitter.instruction("str x9, [x0, #8]");
    emitter.instruction("ldr x9, [sp, #8]");
    emitter.instruction("str x9, [x0, #16]");
    emitter.instruction("ldp x29, x30, [sp, #16]");
    emitter.instruction("add sp, sp, #32");
    emitter.instruction("ret");
}

/// array_push_int: push an integer element to an array.
/// Input: x0 = array pointer, x1 = value
pub fn emit_array_push_int(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_push_int ---");
    emitter.label("__rt_array_push_int");
    emitter.instruction("ldr x9, [x0]");
    emitter.instruction("add x10, x0, #24");
    emitter.instruction("str x1, [x10, x9, lsl #3]");
    emitter.instruction("add x9, x9, #1");
    emitter.instruction("str x9, [x0]");
    emitter.instruction("ret");
}

/// array_push_str: push a string element (ptr+len) to an array.
/// Input: x0 = array pointer, x1 = str ptr, x2 = str len
pub fn emit_array_push_str(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_push_str ---");
    emitter.label("__rt_array_push_str");
    emitter.instruction("ldr x9, [x0]");
    emitter.instruction("lsl x10, x9, #4");
    emitter.instruction("add x10, x0, x10");
    emitter.instruction("add x10, x10, #24");
    emitter.instruction("str x1, [x10]");
    emitter.instruction("str x2, [x10, #8]");
    emitter.instruction("add x9, x9, #1");
    emitter.instruction("str x9, [x0]");
    emitter.instruction("ret");
}

/// sort_int / rsort_int: insertion sort on integer array (in-place).
/// Input: x0 = array pointer
pub fn emit_sort_int(emitter: &mut Emitter, reverse: bool) {
    let label = if reverse { "__rt_rsort_int" } else { "__rt_sort_int" };
    let cmp_branch = if reverse { "b.ge" } else { "b.le" };

    emitter.blank();
    emitter.comment(&format!("--- runtime: {} ---", label));
    emitter.label(label);
    emitter.instruction("ldr x1, [x0]");
    emitter.instruction("add x2, x0, #24");
    emitter.instruction("mov x3, #1");

    let outer = format!("{}_outer", label);
    let inner = format!("{}_inner", label);
    let insert = format!("{}_insert", label);
    let done = format!("{}_done", label);

    emitter.label(&outer);
    emitter.instruction("cmp x3, x1");
    emitter.instruction(&format!("b.ge {}", done));
    emitter.instruction("ldr x4, [x2, x3, lsl #3]");
    emitter.instruction("sub x5, x3, #1");

    emitter.label(&inner);
    emitter.instruction("cmp x5, #0");
    emitter.instruction(&format!("b.lt {}", insert));
    emitter.instruction("ldr x6, [x2, x5, lsl #3]");
    emitter.instruction("cmp x6, x4");
    emitter.instruction(&format!("{} {}", cmp_branch, insert));
    emitter.instruction("add x7, x5, #1");
    emitter.instruction("str x6, [x2, x7, lsl #3]");
    emitter.instruction("sub x5, x5, #1");
    emitter.instruction(&format!("b {}", inner));

    emitter.label(&insert);
    emitter.instruction("add x7, x5, #1");
    emitter.instruction("str x4, [x2, x7, lsl #3]");
    emitter.instruction("add x3, x3, #1");
    emitter.instruction(&format!("b {}", outer));

    emitter.label(&done);
    emitter.instruction("ret");
}
