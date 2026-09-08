//! Purpose:
//! Mixed-needle and Mixed-element membership lowering for PHP `in_array()`.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays::lower_in_array_with_mode()`.
//!
//! Key details:
//! - Preserves strict tag-aware identity and PHP loose string/integer scans on every target.

use super::*;

/// Scans a concrete string array after unboxing a boxed-Mixed needle.
pub(super) fn lower_in_array_mixed_needle_string_array(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
    eq_helper: &str,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            lower_in_array_mixed_needle_string_array_aarch64(ctx, needle, array, eq_helper)
        }
        Arch::X86_64 => {
            lower_in_array_mixed_needle_string_array_x86_64(ctx, needle, array, eq_helper)
        }
    }
}

/// Emits the AArch64 runtime-string branch for a Mixed needle and concrete string array.
fn lower_in_array_mixed_needle_string_array_aarch64(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
    eq_helper: &str,
) -> Result<()> {
    let no_match_label = ctx.next_label("in_array_mixed_string_needle_no_match");
    let loop_label = ctx.next_label("in_array_mixed_string_needle_loop");
    let found_label = ctx.next_label("in_array_mixed_string_needle_found");
    let end_label = ctx.next_label("in_array_mixed_string_needle_end");
    let cleanup_label = ctx.next_label("in_array_mixed_string_needle_cleanup");
    let done_label = ctx.next_label("in_array_mixed_string_needle_done");

    ctx.load_value_to_reg(needle, "x0")?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("cmp x0, #1");                                      // only a runtime string can match concrete string slots on this path
    ctx.emitter.instruction(&format!("b.ne {}", no_match_label));
    abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");                         // preserve the unboxed needle pointer and length
    ctx.load_value_to_reg(array, "x10")?;
    ctx.emitter.instruction("ldr x9, [x10]");                                   // load indexed string-array length
    ctx.emitter.instruction("add x10, x10, #24");                              // point at the first 16-byte string slot
    ctx.emitter.instruction("mov x12, #0");
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp x12, x9");
    ctx.emitter.instruction(&format!("b.ge {}", end_label));
    ctx.emitter.instruction("lsl x13, x12, #4");
    ctx.emitter.instruction("ldr x1, [x10, x13]");                              // comparison left pointer = current array element
    ctx.emitter.instruction("add x14, x13, #8");
    ctx.emitter.instruction("ldr x2, [x10, x14]");                              // comparison left length = current array element length
    abi::emit_push_reg_pair(ctx.emitter, "x9", "x10");
    abi::emit_push_reg(ctx.emitter, "x12");
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x3", 32);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x4", 40);
    abi::emit_call_label(ctx.emitter, eq_helper);
    abi::emit_pop_reg(ctx.emitter, "x12");
    abi::emit_pop_reg_pair(ctx.emitter, "x9", "x10");
    ctx.emitter.instruction(&format!("cbnz x0, {}", found_label));
    ctx.emitter.instruction("add x12, x12, #1");
    ctx.emitter.instruction(&format!("b {}", loop_label));
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov x0, #1");
    ctx.emitter.instruction(&format!("b {}", cleanup_label));
    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("mov x0, #0");
    ctx.emitter.label(&cleanup_label);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    ctx.emitter.instruction(&format!("b {}", done_label));
    ctx.emitter.label(&no_match_label);
    ctx.emitter.instruction("mov x0, #0");
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits the x86_64 runtime-string branch for a Mixed needle and concrete string array.
fn lower_in_array_mixed_needle_string_array_x86_64(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
    eq_helper: &str,
) -> Result<()> {
    let no_match_label = ctx.next_label("in_array_mixed_string_needle_no_match");
    let loop_label = ctx.next_label("in_array_mixed_string_needle_loop");
    let found_label = ctx.next_label("in_array_mixed_string_needle_found");
    let end_label = ctx.next_label("in_array_mixed_string_needle_end");
    let cleanup_label = ctx.next_label("in_array_mixed_string_needle_cleanup");
    let done_label = ctx.next_label("in_array_mixed_string_needle_done");

    ctx.load_value_to_reg(needle, "rax")?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("cmp rax, 1");                                      // only a runtime string can match concrete string slots on this path
    ctx.emitter.instruction(&format!("jne {}", no_match_label));
    abi::emit_push_reg_pair(ctx.emitter, "rdi", "rdx");                       // preserve the unboxed needle pointer and length
    ctx.load_value_to_reg(array, "r10")?;
    ctx.emitter.instruction("mov r11, QWORD PTR [r10]");                       // load indexed string-array length
    ctx.emitter.instruction("lea r12, [r10 + 24]");                            // point at the first 16-byte string slot
    ctx.emitter.instruction("xor r13d, r13d");
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp r13, r11");
    ctx.emitter.instruction(&format!("jge {}", end_label));
    ctx.emitter.instruction("mov rcx, r13");
    ctx.emitter.instruction("shl rcx, 4");
    ctx.emitter.instruction("mov rdi, QWORD PTR [r12 + rcx]");                 // comparison left pointer = current array element
    ctx.emitter.instruction("mov rsi, QWORD PTR [r12 + rcx + 8]");             // comparison left length = current array element length
    abi::emit_push_reg_pair(ctx.emitter, "r11", "r12");
    abi::emit_push_reg(ctx.emitter, "r13");
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rdx", 32);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rcx", 40);
    abi::emit_call_label(ctx.emitter, eq_helper);
    abi::emit_pop_reg(ctx.emitter, "r13");
    abi::emit_pop_reg_pair(ctx.emitter, "r11", "r12");
    ctx.emitter.instruction("test rax, rax");
    ctx.emitter.instruction(&format!("jnz {}", found_label));
    ctx.emitter.instruction("add r13, 1");
    ctx.emitter.instruction(&format!("jmp {}", loop_label));
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov rax, 1");
    ctx.emitter.instruction(&format!("jmp {}", cleanup_label));
    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("xor eax, eax");
    ctx.emitter.label(&cleanup_label);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    ctx.emitter.instruction(&format!("jmp {}", done_label));
    ctx.emitter.label(&no_match_label);
    ctx.emitter.instruction("xor eax, eax");
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Compares a boxed-Mixed needle with boxed-Mixed indexed-array elements.
pub(super) fn lower_in_array_mixed_mixed(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
    mode: InArrayMode,
) -> Result<()> {
    if matches!(mode, InArrayMode::Strict) {
        return lower_in_array_mixed_mixed_strict(ctx, needle, array);
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_in_array_mixed_mixed_loose_aarch64(ctx, needle, array),
        Arch::X86_64 => lower_in_array_mixed_mixed_loose_x86_64(ctx, needle, array),
    }
}

/// Emits a strict boxed-Mixed membership scan on either supported architecture.
fn lower_in_array_mixed_mixed_strict(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_mixed_strict_loop");
    let found_label = ctx.next_label("in_array_mixed_strict_found");
    let end_label = ctx.next_label("in_array_mixed_strict_end");
    let done_label = ctx.next_label("in_array_mixed_strict_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x10")?;
            ctx.load_value_to_reg(needle, "x11")?;
            abi::emit_push_reg_pair(ctx.emitter, "x10", "x11");
            ctx.emitter.instruction("mov x12, #0");                              // start the boxed membership scan at index zero
            ctx.emitter.label(&loop_label);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x10", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x11", 8);
            ctx.emitter.instruction("ldr x9, [x10]");                           // load the array<Mixed> logical length
            ctx.emitter.instruction("cmp x12, x9");                             // have all boxed slots been compared?
            ctx.emitter.instruction(&format!("b.ge {}", end_label));            // no match remains after the final slot
            ctx.emitter.instruction("add x13, x10, #24");                      // skip the indexed-array length/capacity/data header
            ctx.emitter.instruction("ldr x1, [x13, x12, lsl #3]");             // pass the current boxed element as the right operand
            ctx.emitter.instruction("mov x0, x11");                             // pass the boxed needle as the left operand
            abi::emit_push_reg(ctx.emitter, "x12");
            abi::emit_call_label(ctx.emitter, "__rt_mixed_strict_eq");
            abi::emit_pop_reg(ctx.emitter, "x12");
            ctx.emitter.instruction(&format!("cbnz x0, {}", found_label));      // stop at the first identical runtime value
            ctx.emitter.instruction("add x12, x12, #1");                       // advance to the next boxed slot
            ctx.emitter.instruction(&format!("b {}", loop_label));              // continue the strict membership scan
            ctx.emitter.label(&found_label);
            ctx.emitter.instruction("mov x0, #1");                              // report a strict membership match
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the miss result
            ctx.emitter.label(&end_label);
            ctx.emitter.instruction("mov x0, #0");                              // report that no boxed value matched
            ctx.emitter.label(&done_label);
            abi::emit_pop_reg_pair(ctx.emitter, "x10", "x11");
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "r10")?;
            ctx.load_value_to_reg(needle, "r11")?;
            abi::emit_push_reg_pair(ctx.emitter, "r10", "r11");
            ctx.emitter.instruction("xor r13d, r13d");                          // start the boxed membership scan at index zero
            ctx.emitter.label(&loop_label);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r11", 8);
            ctx.emitter.instruction("cmp r13, QWORD PTR [r10]");                // have all boxed slots been compared?
            ctx.emitter.instruction(&format!("jge {}", end_label));             // no match remains after the final slot
            ctx.emitter.instruction("mov rsi, QWORD PTR [r10 + r13*8 + 24]");   // pass the current boxed element as the right operand
            ctx.emitter.instruction("mov rdi, r11");                            // pass the boxed needle as the left operand
            abi::emit_push_reg(ctx.emitter, "r13");
            abi::emit_call_label(ctx.emitter, "__rt_mixed_strict_eq");
            abi::emit_pop_reg(ctx.emitter, "r13");
            ctx.emitter.instruction("test rax, rax");                           // did the current boxed value match?
            ctx.emitter.instruction(&format!("jnz {}", found_label));           // stop at the first identical runtime value
            ctx.emitter.instruction("add r13, 1");                              // advance to the next boxed slot
            ctx.emitter.instruction(&format!("jmp {}", loop_label));            // continue the strict membership scan
            ctx.emitter.label(&found_label);
            ctx.emitter.instruction("mov rax, 1");                              // report a strict membership match
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the miss result
            ctx.emitter.label(&end_label);
            ctx.emitter.instruction("xor eax, eax");                            // report that no boxed value matched
            ctx.emitter.label(&done_label);
            abi::emit_pop_reg_pair(ctx.emitter, "r10", "r11");
        }
    }
    Ok(())
}

/// Emits loose boxed-Mixed membership for runtime string and integer needles on AArch64.
fn lower_in_array_mixed_mixed_loose_aarch64(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
) -> Result<()> {
    let string_label = ctx.next_label("in_array_mixed_needle_string");
    let int_label = ctx.next_label("in_array_mixed_needle_int");
    let fallback_label = ctx.next_label("in_array_mixed_needle_fallback");
    let loop_label = ctx.next_label("in_array_mixed_needle_string_loop");
    let next_label = ctx.next_label("in_array_mixed_needle_string_next");
    let found_label = ctx.next_label("in_array_mixed_needle_string_found");
    let end_label = ctx.next_label("in_array_mixed_needle_string_end");
    let done_label = ctx.next_label("in_array_mixed_needle_done");

    ctx.load_value_to_reg(needle, "x0")?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("cmp x0, #1");                                      // runtime tag 1 selects PHP string loose equality
    ctx.emitter.instruction(&format!("b.eq {}", string_label));
    ctx.emitter.instruction("cmp x0, #0");                                      // runtime tag 0 selects PHP integer loose equality
    ctx.emitter.instruction(&format!("b.eq {}", int_label));
    ctx.emitter.instruction(&format!("b {}", fallback_label));                  // pointer-like values retain tag-aware identity fallback

    ctx.emitter.label(&int_label);
    ctx.load_value_to_reg(array, "x0")?;
    ctx.emitter.instruction("mov x2, #0");                                      // request loose comparison from the mixed-int helper
    abi::emit_call_label(ctx.emitter, "__rt_in_array_mixed_int");
    ctx.emitter.instruction(&format!("b {}", done_label));

    ctx.emitter.label(&string_label);
    abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");                         // preserve the unboxed needle pointer and length
    ctx.load_value_to_reg(array, "x10")?;
    abi::emit_push_reg(ctx.emitter, "x10");
    ctx.emitter.instruction("mov x12, #0");                                     // start scanning boxed array elements
    ctx.emitter.label(&loop_label);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x10", 0);
    ctx.emitter.instruction("ldr x9, [x10]");                                   // load the array<Mixed> logical length
    ctx.emitter.instruction("cmp x12, x9");
    ctx.emitter.instruction(&format!("b.ge {}", end_label));
    ctx.emitter.instruction("add x13, x10, #24");                              // skip the indexed-array length/capacity/data header
    ctx.emitter.instruction("ldr x0, [x13, x12, lsl #3]");                     // unbox the current array element
    abi::emit_push_reg(ctx.emitter, "x12");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("cmp x0, #1");                                      // non-string elements do not match this string needle path
    ctx.emitter.instruction(&format!("b.ne {}", next_label));
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x3", 32);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x4", 40);
    abi::emit_call_label(ctx.emitter, "__rt_str_loose_eq");
    ctx.emitter.instruction(&format!("cbnz x0, {}", found_label));
    ctx.emitter.label(&next_label);
    abi::emit_pop_reg(ctx.emitter, "x12");
    ctx.emitter.instruction("add x12, x12, #1");
    ctx.emitter.instruction(&format!("b {}", loop_label));
    ctx.emitter.label(&found_label);
    abi::emit_pop_reg(ctx.emitter, "x12");
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    ctx.emitter.instruction("mov x0, #1");
    ctx.emitter.instruction(&format!("b {}", done_label));
    ctx.emitter.label(&end_label);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    ctx.emitter.instruction("mov x0, #0");                                      // report that no boxed string matched
    ctx.emitter.instruction(&format!("b {}", done_label));

    ctx.emitter.label(&fallback_label);
    lower_in_array_mixed_mixed_strict(ctx, needle, array)?;
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits loose boxed-Mixed membership for runtime string and integer needles on x86_64.
fn lower_in_array_mixed_mixed_loose_x86_64(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
) -> Result<()> {
    let string_label = ctx.next_label("in_array_mixed_needle_string");
    let int_label = ctx.next_label("in_array_mixed_needle_int");
    let fallback_label = ctx.next_label("in_array_mixed_needle_fallback");
    let loop_label = ctx.next_label("in_array_mixed_needle_string_loop");
    let next_label = ctx.next_label("in_array_mixed_needle_string_next");
    let found_label = ctx.next_label("in_array_mixed_needle_string_found");
    let end_label = ctx.next_label("in_array_mixed_needle_string_end");
    let done_label = ctx.next_label("in_array_mixed_needle_done");

    ctx.load_value_to_reg(needle, "rax")?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("cmp rax, 1");                                      // runtime tag 1 selects PHP string loose equality
    ctx.emitter.instruction(&format!("je {}", string_label));
    ctx.emitter.instruction("cmp rax, 0");                                      // runtime tag 0 selects PHP integer loose equality
    ctx.emitter.instruction(&format!("je {}", int_label));
    ctx.emitter.instruction(&format!("jmp {}", fallback_label));                // pointer-like values retain tag-aware identity fallback

    ctx.emitter.label(&int_label);
    ctx.emitter.instruction("mov rsi, rdi");                                    // pass the unboxed integer needle payload
    ctx.load_value_to_reg(array, "rdi")?;
    ctx.emitter.instruction("xor edx, edx");                                    // request loose comparison from the mixed-int helper
    abi::emit_call_label(ctx.emitter, "__rt_in_array_mixed_int");
    ctx.emitter.instruction(&format!("jmp {}", done_label));

    ctx.emitter.label(&string_label);
    abi::emit_push_reg_pair(ctx.emitter, "rdi", "rdx");                       // preserve the unboxed needle pointer and length
    ctx.load_value_to_reg(array, "r10")?;
    abi::emit_push_reg(ctx.emitter, "r10");
    ctx.emitter.instruction("xor r13d, r13d");                                  // start scanning boxed array elements
    ctx.emitter.label(&loop_label);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", 0);
    ctx.emitter.instruction("cmp r13, QWORD PTR [r10]");
    ctx.emitter.instruction(&format!("jge {}", end_label));
    ctx.emitter.instruction("mov rax, QWORD PTR [r10 + r13*8 + 24]");           // unbox the current array element
    abi::emit_push_reg(ctx.emitter, "r13");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("cmp rax, 1");                                      // non-string elements do not match this string needle path
    ctx.emitter.instruction(&format!("jne {}", next_label));
    ctx.emitter.instruction("mov rsi, rdx");                                    // comparison left length = current element length
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rdx", 32);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rcx", 40);
    abi::emit_call_label(ctx.emitter, "__rt_str_loose_eq");
    ctx.emitter.instruction("test rax, rax");
    ctx.emitter.instruction(&format!("jnz {}", found_label));
    ctx.emitter.label(&next_label);
    abi::emit_pop_reg(ctx.emitter, "r13");
    ctx.emitter.instruction("add r13, 1");
    ctx.emitter.instruction(&format!("jmp {}", loop_label));
    ctx.emitter.label(&found_label);
    abi::emit_pop_reg(ctx.emitter, "r13");
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    ctx.emitter.instruction("mov rax, 1");
    ctx.emitter.instruction(&format!("jmp {}", done_label));
    ctx.emitter.label(&end_label);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    ctx.emitter.instruction("xor eax, eax");                                    // report that no boxed string matched
    ctx.emitter.instruction(&format!("jmp {}", done_label));

    ctx.emitter.label(&fallback_label);
    lower_in_array_mixed_mixed_strict(ctx, needle, array)?;
    ctx.emitter.label(&done_label);
    Ok(())
}
