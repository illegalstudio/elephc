//! Purpose:
//! Boolean, string, and Mixed-array membership loops.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays`.
//!
//! Key details:
//! - Preserves callback ABI, target parity, array storage, and ownership contracts.

use super::*;

/// Scans a bool array against a precomputed AArch64 boolean needle register.
pub(super) fn lower_in_array_bool_array_with_preloaded_needle_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    needle_reg: &str,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_bool_loop");
    let found_label = ctx.next_label("in_array_bool_found");
    let end_label = ctx.next_label("in_array_bool_end");
    let done_label = ctx.next_label("in_array_bool_done");

    ctx.load_value_to_reg(array, "x10")?;
    elephc_memoir::values::packed_slots::emit_load_length(ctx.emitter, "x9", "x10"); // load indexed bool-array length before scanning payload slots
    elephc_memoir::values::packed_slots::emit_packed_payload_aarch64(ctx.emitter, "x10", "x10");
    ctx.emitter.instruction("mov x12, #0");                                     // start the bool membership scan at index zero
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp x12, x9");                                     // compare the scan index against indexed-array length
    ctx.emitter.instruction(&format!("b.ge {}", end_label));                    // finish with false after all bool elements are scanned
    ctx.emitter.instruction("ldr x13, [x10, x12, lsl #3]");                     // load the current bool element
    ctx.emitter.instruction(&format!("cmp x13, {}", needle_reg));               // compare element bool against needle bool
    ctx.emitter.instruction(&format!("b.eq {}", found_label));                  // stop as soon as a loosely equal bool is found
    ctx.emitter.instruction("add x12, x12, #1");                                // advance to the next indexed bool element
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // continue scanning remaining bool payload slots
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov x0, #1");                                      // return true after finding a matching bool
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the not-found result after a match
    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("mov x0, #0");                                      // return false when no bool element matches
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Scans a bool array against a precomputed x86_64 boolean needle register.
pub(super) fn lower_in_array_bool_array_with_preloaded_needle_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    needle_reg: &str,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_bool_loop");
    let found_label = ctx.next_label("in_array_bool_found");
    let end_label = ctx.next_label("in_array_bool_end");
    let done_label = ctx.next_label("in_array_bool_done");

    ctx.load_value_to_reg(array, "r11")?;
    elephc_memoir::values::packed_slots::emit_load_length(ctx.emitter, "r12", "r11"); // load indexed bool-array length before scanning payload slots
    elephc_memoir::values::packed_slots::emit_packed_payload_x86_64(ctx.emitter, "r11", "r11");
    ctx.emitter.instruction("xor r13d, r13d");                                  // start the bool membership scan at index zero
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp r13, r12");                                    // compare the scan index against indexed-array length
    ctx.emitter.instruction(&format!("jge {}", end_label));                     // finish with false after all bool elements are scanned
    ctx.emitter.instruction("mov rax, QWORD PTR [r11 + r13*8]");                // load the current bool element
    ctx.emitter.instruction(&format!("cmp rax, {}", needle_reg));               // compare element bool against needle bool
    ctx.emitter.instruction(&format!("je {}", found_label));                    // stop as soon as a loosely equal bool is found
    ctx.emitter.instruction("add r13, 1");                                      // advance to the next indexed bool element
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // continue scanning remaining bool payload slots
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov rax, 1");                                      // return true after finding a matching bool
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the not-found result after a match
    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("xor eax, eax");                                    // return false when no bool element matches
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Lowers string indexed-array membership with a linear scan and the selected string equality helper.
pub(super) fn lower_in_array_string(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
    eq_helper: &str,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_in_array_string_aarch64(ctx, needle, array, eq_helper),
        Arch::X86_64 => lower_in_array_string_x86_64(ctx, needle, array, eq_helper),
    }
}

/// Emits the AArch64 string-array membership loop.
pub(super) fn lower_in_array_string_aarch64(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
    eq_helper: &str,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_str_loop");
    let found_label = ctx.next_label("in_array_str_found");
    let end_label = ctx.next_label("in_array_str_end");
    let done_label = ctx.next_label("in_array_str_done");

    ctx.load_value_to_reg(array, "x10")?;
    elephc_memoir::values::packed_slots::emit_load_length(ctx.emitter, "x9", "x10"); // load indexed string-array length before scanning payload slots
    elephc_memoir::values::packed_slots::emit_packed_payload_aarch64(ctx.emitter, "x10", "x10");
    ctx.emitter.instruction("mov x12, #0");                                     // start the string membership scan at index zero
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp x12, x9");                                     // compare the scan index against indexed-array length
    ctx.emitter.instruction(&format!("b.ge {}", end_label));                    // finish with false after all string elements are scanned
    ctx.emitter.instruction("lsl x13, x12, #4");                                // scale the element index by the 16-byte string slot width
    ctx.emitter.instruction("ldr x1, [x10, x13]");                              // load the current string element pointer for comparison
    ctx.emitter.instruction("add x14, x13, #8");                                // compute the current string element length-slot offset
    ctx.emitter.instruction("ldr x2, [x10, x14]");                              // load the current string element length for comparison
    abi::emit_push_reg_pair(ctx.emitter, "x9", "x10");
    abi::emit_push_reg(ctx.emitter, "x12");
    ctx.load_string_value_to_regs(needle, "x3", "x4")?;
    abi::emit_call_label(ctx.emitter, eq_helper);
    abi::emit_pop_reg(ctx.emitter, "x12");
    abi::emit_pop_reg_pair(ctx.emitter, "x9", "x10");
    ctx.emitter
        .instruction(&format!("cbnz x0, {}", found_label)); // stop as soon as the searched string matches an element
    ctx.emitter.instruction("add x12, x12, #1");                                // advance to the next indexed string element
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // continue scanning remaining string payload slots
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov x0, #1");                                      // return true after finding the searched string
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the not-found result after a match
    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("mov x0, #0");                                      // return false when no indexed string element matches
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Lowers a string-needle membership scan over an indexed `array<Mixed>`.
///
/// Each 8-byte slot holds a boxed Mixed cell, so every cell is unboxed and the string-tagged ones
/// are compared with the selected string equality helper, mirroring the concrete string-array path.
pub(super) fn lower_in_array_mixed_string(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
    eq_helper: &str,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_in_array_mixed_string_aarch64(ctx, needle, array, eq_helper),
        Arch::X86_64 => lower_in_array_mixed_string_x86_64(ctx, needle, array, eq_helper),
    }
}

/// Lowers an integer-needle membership scan over boxed-Mixed indexed-array slots.
///
/// The runtime helper dispatches on each cell's tag so loose mode preserves PHP
/// numeric-string, float, bool, and null comparison rules while strict mode
/// accepts only an integer-tagged cell with the same payload.
pub(super) fn lower_in_array_mixed_int(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
    strict: bool,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
            ctx.load_value_to_reg(needle, "x1")?;
            abi::emit_load_int_immediate(ctx.emitter, "x2", i64::from(strict));
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
            ctx.load_value_to_reg(needle, "rsi")?;
            abi::emit_load_int_immediate(ctx.emitter, "rdx", i64::from(strict));
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_in_array_mixed_int");
    Ok(())
}

/// Emits the AArch64 boxed-Mixed-array string membership loop.
pub(super) fn lower_in_array_mixed_string_aarch64(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
    eq_helper: &str,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_mix_loop");
    let not_string_label = ctx.next_label("in_array_mix_not_string");
    let have_flag_label = ctx.next_label("in_array_mix_have_flag");
    let found_label = ctx.next_label("in_array_mix_found");
    let end_label = ctx.next_label("in_array_mix_end");
    let done_label = ctx.next_label("in_array_mix_done");

    ctx.load_value_to_reg(array, "x10")?;
    elephc_memoir::values::packed_slots::emit_load_length(ctx.emitter, "x9", "x10"); // load array<Mixed> length before scanning boxed slots
    elephc_memoir::values::packed_slots::emit_packed_payload_aarch64(ctx.emitter, "x10", "x10");
    ctx.emitter.instruction("mov x12, #0");                                     // start the membership scan at index zero
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp x12, x9");                                     // compare the scan index against the array length
    ctx.emitter.instruction(&format!("b.ge {}", end_label));                    // finish with false once every cell is scanned
    ctx.emitter.instruction("ldr x0, [x10, x12, lsl #3]");                      // load the current boxed Mixed cell pointer from its 8-byte slot
    abi::emit_push_reg_pair(ctx.emitter, "x9", "x10");
    abi::emit_push_reg(ctx.emitter, "x12");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox"); // unbox the cell → x0=tag, x1=string ptr, x2=string len
    ctx.emitter.instruction("cmp x0, #1");                                      // is this cell a string value (runtime tag 1)?
    ctx.emitter
        .instruction(&format!("b.ne {}", not_string_label)); // non-string cells can never equal a string needle
    ctx.load_string_value_to_regs(needle, "x3", "x4")?;
    abi::emit_call_label(ctx.emitter, eq_helper); // compare the unboxed string element (x1/x2) against the needle (x3/x4)
    ctx.emitter.instruction(&format!("b {}", have_flag_label));                 // carry the str-eq result into the shared match-flag join
    ctx.emitter.label(&not_string_label);
    ctx.emitter.instruction("mov x0, #0");                                      // a non-string cell yields a not-matched flag
    ctx.emitter.label(&have_flag_label);
    abi::emit_pop_reg(ctx.emitter, "x12");
    abi::emit_pop_reg_pair(ctx.emitter, "x9", "x10");
    ctx.emitter
        .instruction(&format!("cbnz x0, {}", found_label)); // stop as soon as a cell matches the needle
    ctx.emitter.instruction("add x12, x12, #1");                                // advance to the next boxed Mixed cell
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // continue scanning the remaining cells
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov x0, #1");                                      // return true after finding a matching cell
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the not-found result after a match
    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("mov x0, #0");                                      // return false when no cell matches the needle
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits the x86_64 boxed-Mixed-array string membership loop.
pub(super) fn lower_in_array_mixed_string_x86_64(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
    eq_helper: &str,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_mix_loop");
    let not_string_label = ctx.next_label("in_array_mix_not_string");
    let have_flag_label = ctx.next_label("in_array_mix_have_flag");
    let found_label = ctx.next_label("in_array_mix_found");
    let end_label = ctx.next_label("in_array_mix_end");
    let done_label = ctx.next_label("in_array_mix_done");

    ctx.load_value_to_reg(array, "r10")?;
    elephc_memoir::values::packed_slots::emit_load_length(ctx.emitter, "r11", "r10"); // load array<Mixed> length before scanning boxed slots
    elephc_memoir::values::packed_slots::emit_packed_payload_x86_64(ctx.emitter, "r12", "r10");
    ctx.emitter.instruction("xor r13d, r13d");                                  // start the membership scan at index zero
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp r13, r11");                                    // compare the scan index against the array length
    ctx.emitter.instruction(&format!("jge {}", end_label));                     // finish with false once every cell is scanned
    ctx.emitter.instruction("mov rax, QWORD PTR [r12 + r13*8]");                // load the boxed Mixed cell pointer into rax (the unbox input register)
    abi::emit_push_reg_pair(ctx.emitter, "r11", "r12");
    abi::emit_push_reg(ctx.emitter, "r13");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox"); // unbox the cell → rax=tag, rdi=string ptr, rdx=string len
    ctx.emitter.instruction("cmp rax, 1");                                      // is this cell a string value (runtime tag 1)?
    ctx.emitter
        .instruction(&format!("jne {}", not_string_label)); // non-string cells can never equal a string needle
    ctx.emitter.instruction("mov rsi, rdx");                                    // move the unboxed string length into the comparison argument
    ctx.load_string_value_to_regs(needle, "rdx", "rcx")?;
    abi::emit_call_label(ctx.emitter, eq_helper); // compare the unboxed string element (rdi/rsi) against the needle (rdx/rcx)
    ctx.emitter.instruction(&format!("jmp {}", have_flag_label));               // carry the str-eq result into the shared match-flag join
    ctx.emitter.label(&not_string_label);
    ctx.emitter.instruction("xor eax, eax");                                    // a non-string cell yields a not-matched flag
    ctx.emitter.label(&have_flag_label);
    abi::emit_pop_reg(ctx.emitter, "r13");
    abi::emit_pop_reg_pair(ctx.emitter, "r11", "r12");
    ctx.emitter.instruction("test rax, rax");                                   // did the current cell match the needle?
    ctx.emitter.instruction(&format!("jne {}", found_label));                   // stop as soon as a cell matches the needle
    ctx.emitter.instruction("add r13, 1");                                      // advance to the next boxed Mixed cell
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // continue scanning the remaining cells
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov rax, 1");                                      // return true after finding a matching cell
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the not-found result after a match
    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("xor eax, eax");                                    // return false when no cell matches the needle
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits the x86_64 string-array membership loop.
pub(super) fn lower_in_array_string_x86_64(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
    eq_helper: &str,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_str_loop");
    let found_label = ctx.next_label("in_array_str_found");
    let end_label = ctx.next_label("in_array_str_end");
    let done_label = ctx.next_label("in_array_str_done");

    ctx.load_value_to_reg(array, "r10")?;
    elephc_memoir::values::packed_slots::emit_load_length(ctx.emitter, "r11", "r10"); // load indexed string-array length before scanning payload slots
    elephc_memoir::values::packed_slots::emit_packed_payload_x86_64(ctx.emitter, "r12", "r10");
    ctx.emitter.instruction("xor r13d, r13d");                                  // start the string membership scan at index zero
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp r13, r11");                                    // compare the scan index against indexed-array length
    ctx.emitter.instruction(&format!("jge {}", end_label));                     // finish with false after all string elements are scanned
    ctx.emitter.instruction("mov rcx, r13");                                    // copy the scan index before scaling it to a byte offset
    ctx.emitter.instruction("shl rcx, 4");                                      // scale the element index by the 16-byte string slot width
    ctx.emitter.instruction("mov rdi, QWORD PTR [r12 + rcx]");                  // load the current string element pointer for comparison
    ctx.emitter
        .instruction("mov rsi, QWORD PTR [r12 + rcx + 8]"); // load the current string element length for comparison
    abi::emit_push_reg_pair(ctx.emitter, "r11", "r12");
    abi::emit_push_reg(ctx.emitter, "r13");
    ctx.load_string_value_to_regs(needle, "rdx", "rcx")?;
    abi::emit_call_label(ctx.emitter, eq_helper);
    abi::emit_pop_reg(ctx.emitter, "r13");
    abi::emit_pop_reg_pair(ctx.emitter, "r11", "r12");
    ctx.emitter.instruction("test rax, rax");                                   // check whether the current string element matched the needle
    ctx.emitter.instruction(&format!("jne {}", found_label));                   // stop as soon as the searched string matches an element
    ctx.emitter.instruction("add r13, 1");                                      // advance to the next indexed string element
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // continue scanning remaining string payload slots
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov rax, 1");                                      // return true after finding the searched string
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the not-found result after a match
    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("xor eax, eax");                                    // return false when no indexed string element matches
    ctx.emitter.label(&done_label);
    Ok(())
}
