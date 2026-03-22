use super::emit::Emitter;

pub fn emit_runtime(emitter: &mut Emitter) {
    emit_itoa(emitter);
    emit_concat(emitter);
    emit_atoi(emitter);
    emit_argv(emitter);
    emit_heap_alloc(emitter);
    emit_array_new(emitter);
    emit_array_push_int(emitter);
    emit_array_push_str(emitter);
    emit_sort_int(emitter, false);
    emit_sort_int(emitter, true);
}

/// Returns BSS directives needed by runtime routines.
pub fn emit_runtime_data() -> String {
    let mut out = String::new();
    out.push_str(".comm _concat_buf, 65536, 3\n");
    out.push_str(".comm _concat_off, 8, 3\n");
    out.push_str(".comm _global_argc, 8, 3\n");
    out.push_str(".comm _global_argv, 8, 3\n");
    out.push_str(".comm _heap_buf, 1048576, 3\n");
    out.push_str(".comm _heap_off, 8, 3\n");
    out
}

/// itoa: convert signed 64-bit integer to decimal string.
/// Input:  x0 = integer value
/// Output: x1 = pointer to string, x2 = length
/// Writes into _concat_buf (persists across calls).
fn emit_itoa(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: itoa ---");
    emitter.comment("Input: x0 = integer value");
    emitter.comment("Output: x1 = pointer to string, x2 = length");
    emitter.label("__rt_itoa");
    emitter.instruction("sub sp, sp, #16");
    emitter.instruction("stp x29, x30, [sp]");
    emitter.instruction("mov x29, sp");

    // Allocate 21 bytes from concat buffer
    emitter.instruction("adrp x6, _concat_off@PAGE");
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");
    emitter.instruction("ldr x8, [x6]");
    emitter.instruction("adrp x7, _concat_buf@PAGE");
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");
    emitter.instruction("add x9, x7, x8");
    emitter.instruction("add x9, x9, #20"); // point to end of 21-byte region

    emitter.instruction("mov x10, #0"); // digit count
    emitter.instruction("mov x11, #0"); // is_negative

    // Handle negative
    emitter.instruction("cmp x0, #0");
    emitter.instruction("b.ge __rt_itoa_positive");
    emitter.instruction("mov x11, #1");
    emitter.instruction("neg x0, x0");

    emitter.label("__rt_itoa_positive");
    emitter.instruction("cbnz x0, __rt_itoa_loop");
    // Zero
    emitter.instruction("mov w12, #48");
    emitter.instruction("strb w12, [x9]");
    emitter.instruction("sub x9, x9, #1");
    emitter.instruction("mov x10, #1");
    emitter.instruction("b __rt_itoa_done");

    // Digit extraction loop
    emitter.label("__rt_itoa_loop");
    emitter.instruction("cbz x0, __rt_itoa_sign");
    emitter.instruction("mov x12, #10");
    emitter.instruction("udiv x13, x0, x12");
    emitter.instruction("msub x14, x13, x12, x0");
    emitter.instruction("add x14, x14, #48");
    emitter.instruction("strb w14, [x9]");
    emitter.instruction("sub x9, x9, #1");
    emitter.instruction("add x10, x10, #1");
    emitter.instruction("mov x0, x13");
    emitter.instruction("b __rt_itoa_loop");

    // Sign
    emitter.label("__rt_itoa_sign");
    emitter.instruction("cbz x11, __rt_itoa_done");
    emitter.instruction("mov w12, #45");
    emitter.instruction("strb w12, [x9]");
    emitter.instruction("sub x9, x9, #1");
    emitter.instruction("add x10, x10, #1");

    emitter.label("__rt_itoa_done");
    // Advance concat offset by 21
    emitter.instruction("add x8, x8, #21");
    emitter.instruction("str x8, [x6]");

    // Result
    emitter.instruction("add x1, x9, #1");
    emitter.instruction("mov x2, x10");
    emitter.instruction("ldp x29, x30, [sp]");
    emitter.instruction("add sp, sp, #16");
    emitter.instruction("ret");
}

/// concat: concatenate two strings into the concat buffer.
/// Input:  x1=left_ptr, x2=left_len, x3=right_ptr, x4=right_len
/// Output: x1=result_ptr, x2=result_len
fn emit_concat(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: concat ---");
    emitter.comment("Input: x1=left_ptr, x2=left_len, x3=right_ptr, x4=right_len");
    emitter.comment("Output: x1=result_ptr, x2=result_len");
    emitter.label("__rt_concat");
    emitter.instruction("sub sp, sp, #64");
    emitter.instruction("stp x29, x30, [sp, #48]");
    emitter.instruction("add x29, sp, #48");

    // Save inputs
    emitter.instruction("stp x1, x2, [sp, #0]");
    emitter.instruction("stp x3, x4, [sp, #16]");

    // Total length
    emitter.instruction("add x5, x2, x4");
    emitter.instruction("str x5, [sp, #32]");

    // Get buffer destination
    emitter.instruction("adrp x6, _concat_off@PAGE");
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");
    emitter.instruction("ldr x8, [x6]");
    emitter.instruction("adrp x7, _concat_buf@PAGE");
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");
    emitter.instruction("add x9, x7, x8");
    emitter.instruction("str x9, [sp, #40]");

    // Copy left
    emitter.instruction("ldp x1, x2, [sp, #0]");
    emitter.instruction("mov x10, x9");
    emitter.label("__rt_concat_cl");
    emitter.instruction("cbz x2, __rt_concat_cr_setup");
    emitter.instruction("ldrb w11, [x1], #1");
    emitter.instruction("strb w11, [x10], #1");
    emitter.instruction("sub x2, x2, #1");
    emitter.instruction("b __rt_concat_cl");

    // Copy right
    emitter.label("__rt_concat_cr_setup");
    emitter.instruction("ldp x3, x4, [sp, #16]");
    emitter.label("__rt_concat_cr");
    emitter.instruction("cbz x4, __rt_concat_done");
    emitter.instruction("ldrb w11, [x3], #1");
    emitter.instruction("strb w11, [x10], #1");
    emitter.instruction("sub x4, x4, #1");
    emitter.instruction("b __rt_concat_cr");

    // Update offset
    emitter.label("__rt_concat_done");
    emitter.instruction("ldr x5, [sp, #32]");
    emitter.instruction("adrp x6, _concat_off@PAGE");
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");
    emitter.instruction("ldr x8, [x6]");
    emitter.instruction("add x8, x8, x5");
    emitter.instruction("str x8, [x6]");

    // Return
    emitter.instruction("ldr x1, [sp, #40]");
    emitter.instruction("ldr x2, [sp, #32]");
    emitter.instruction("ldp x29, x30, [sp, #48]");
    emitter.instruction("add sp, sp, #64");
    emitter.instruction("ret");
}

/// atoi: parse a string to a signed 64-bit integer.
/// Input:  x1 = string pointer, x2 = string length
/// Output: x0 = integer value
fn emit_atoi(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: atoi ---");
    emitter.label("__rt_atoi");
    emitter.instruction("mov x0, #0"); // result
    emitter.instruction("mov x3, #0"); // is_negative
    emitter.instruction("cbz x2, __rt_atoi_done"); // empty string → 0

    // Check for leading '-'
    emitter.instruction("ldrb w4, [x1]");
    emitter.instruction("cmp w4, #45"); // '-'
    emitter.instruction("b.ne __rt_atoi_loop");
    emitter.instruction("mov x3, #1");
    emitter.instruction("add x1, x1, #1");
    emitter.instruction("sub x2, x2, #1");

    emitter.label("__rt_atoi_loop");
    emitter.instruction("cbz x2, __rt_atoi_sign");
    emitter.instruction("ldrb w4, [x1], #1");
    emitter.instruction("sub w4, w4, #48"); // ASCII to digit
    // If not a digit (< 0 or > 9), stop
    emitter.instruction("cmp w4, #9");
    emitter.instruction("b.hi __rt_atoi_sign");
    emitter.instruction("mov x5, #10");
    emitter.instruction("mul x0, x0, x5");
    emitter.instruction("add x0, x0, x4");
    emitter.instruction("sub x2, x2, #1");
    emitter.instruction("b __rt_atoi_loop");

    emitter.label("__rt_atoi_sign");
    emitter.instruction("cbz x3, __rt_atoi_done");
    emitter.instruction("neg x0, x0");

    emitter.label("__rt_atoi_done");
    emitter.instruction("ret");
}

/// argv: get command-line argument by index.
/// Input:  x0 = argument index
/// Output: x1 = string pointer, x2 = string length
fn emit_argv(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: argv ---");
    emitter.label("__rt_argv");
    // Load global argv pointer
    emitter.instruction("adrp x9, _global_argv@PAGE");
    emitter.instruction("add x9, x9, _global_argv@PAGEOFF");
    emitter.instruction("ldr x9, [x9]"); // x9 = argv array
    // Index into argv: x1 = argv[x0]
    emitter.instruction("ldr x1, [x9, x0, lsl #3]"); // x1 = pointer to C string

    // Compute string length (scan for \0)
    emitter.instruction("mov x2, #0");
    emitter.label("__rt_argv_len");
    emitter.instruction("ldrb w3, [x1, x2]");
    emitter.instruction("cbz w3, __rt_argv_done");
    emitter.instruction("add x2, x2, #1");
    emitter.instruction("b __rt_argv_len");

    emitter.label("__rt_argv_done");
    emitter.instruction("ret");
}

/// heap_alloc: bump allocator.
/// Input: x0 = bytes needed
/// Output: x0 = pointer to allocated memory
fn emit_heap_alloc(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: heap_alloc ---");
    emitter.label("__rt_heap_alloc");
    emitter.instruction("adrp x9, _heap_off@PAGE");
    emitter.instruction("add x9, x9, _heap_off@PAGEOFF");
    emitter.instruction("ldr x10, [x9]"); // current offset
    emitter.instruction("adrp x11, _heap_buf@PAGE");
    emitter.instruction("add x11, x11, _heap_buf@PAGEOFF");
    emitter.instruction("add x12, x11, x10"); // result pointer
    emitter.instruction("add x10, x10, x0"); // bump offset
    emitter.instruction("str x10, [x9]"); // store new offset
    emitter.instruction("mov x0, x12"); // return pointer
    emitter.instruction("ret");
}

/// array_new: create a new array on the heap.
/// Input: x0 = capacity, x1 = element size (8 or 16)
/// Output: x0 = pointer to array header
/// Layout: [length:8][capacity:8][elem_size:8][elements...]
fn emit_array_new(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_new ---");
    emitter.label("__rt_array_new");
    emitter.instruction("sub sp, sp, #32");
    emitter.instruction("stp x29, x30, [sp, #16]");
    emitter.instruction("add x29, sp, #16");
    // Save capacity and elem_size
    emitter.instruction("str x0, [sp, #0]"); // capacity
    emitter.instruction("str x1, [sp, #8]"); // elem_size
    // Allocate: 24 (header) + capacity * elem_size
    emitter.instruction("mul x2, x0, x1");
    emitter.instruction("add x0, x2, #24");
    emitter.instruction("bl __rt_heap_alloc");
    // x0 = allocated pointer, init header
    emitter.instruction("str xzr, [x0]"); // length = 0
    emitter.instruction("ldr x9, [sp, #0]");
    emitter.instruction("str x9, [x0, #8]"); // capacity
    emitter.instruction("ldr x9, [sp, #8]");
    emitter.instruction("str x9, [x0, #16]"); // elem_size
    emitter.instruction("ldp x29, x30, [sp, #16]");
    emitter.instruction("add sp, sp, #32");
    emitter.instruction("ret");
}

/// array_push_int: push an integer element to an array.
/// Input: x0 = array pointer, x1 = value
fn emit_array_push_int(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_push_int ---");
    emitter.label("__rt_array_push_int");
    emitter.instruction("ldr x9, [x0]"); // length
    // Store at header + 24 + length * 8
    emitter.instruction("add x10, x0, #24");
    emitter.instruction("str x1, [x10, x9, lsl #3]");
    // Increment length
    emitter.instruction("add x9, x9, #1");
    emitter.instruction("str x9, [x0]");
    emitter.instruction("ret");
}

/// sort_int / rsort_int: insertion sort on integer array (in-place).
/// Input: x0 = array pointer
fn emit_sort_int(emitter: &mut Emitter, reverse: bool) {
    let label = if reverse { "__rt_rsort_int" } else { "__rt_sort_int" };
    let cmp_branch = if reverse { "b.ge" } else { "b.le" };

    emitter.blank();
    emitter.comment(&format!("--- runtime: {} ---", label));
    emitter.label(label);
    // Insertion sort: for i=1..len: key=arr[i], j=i-1, while j>=0 && arr[j]>key: arr[j+1]=arr[j], j--; arr[j+1]=key
    emitter.instruction("ldr x1, [x0]"); // length
    emitter.instruction("add x2, x0, #24"); // elements base
    emitter.instruction("mov x3, #1"); // i = 1

    let outer = format!("{}_outer", label);
    let inner = format!("{}_inner", label);
    let _shift = format!("{}_shift", label);
    let insert = format!("{}_insert", label);
    let done = format!("{}_done", label);

    emitter.label(&outer);
    emitter.instruction("cmp x3, x1");
    emitter.instruction(&format!("b.ge {}", done));
    // key = arr[i]
    emitter.instruction("ldr x4, [x2, x3, lsl #3]"); // key
    emitter.instruction("sub x5, x3, #1"); // j = i - 1

    emitter.label(&inner);
    // if j < 0, insert
    emitter.instruction("cmp x5, #0");
    emitter.instruction(&format!("b.lt {}", insert));
    // if arr[j] <= key (sort) or arr[j] >= key (rsort), insert
    emitter.instruction("ldr x6, [x2, x5, lsl #3]");
    emitter.instruction("cmp x6, x4");
    emitter.instruction(&format!("{} {}", cmp_branch, insert));
    // shift: arr[j+1] = arr[j]
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

/// array_push_str: push a string element (ptr+len) to an array.
/// Input: x0 = array pointer, x1 = str ptr, x2 = str len
fn emit_array_push_str(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_push_str ---");
    emitter.label("__rt_array_push_str");
    emitter.instruction("ldr x9, [x0]"); // length
    // Offset = 24 + length * 16
    emitter.instruction("lsl x10, x9, #4"); // length * 16
    emitter.instruction("add x10, x0, x10");
    emitter.instruction("add x10, x10, #24");
    emitter.instruction("str x1, [x10]"); // ptr
    emitter.instruction("str x2, [x10, #8]"); // len
    // Increment length
    emitter.instruction("add x9, x9, #1");
    emitter.instruction("str x9, [x0]");
    emitter.instruction("ret");
}
