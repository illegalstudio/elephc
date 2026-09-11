//! Purpose:
//! Cross-type string, integer, and bool membership lowering.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays`.
//!
//! Key details:
//! - Preserves callback ABI, target parity, array storage, and ownership contracts.

use super::*;

/// Lowers loose int-needle membership in a string array via PHP numeric-string parsing.
pub(super) fn lower_in_array_int_needle_string_array(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_in_array_int_needle_string_array_aarch64(ctx, needle, array),
        Arch::X86_64 => lower_in_array_int_needle_string_array_x86_64(ctx, needle, array),
    }
}

/// Emits the AArch64 int-needle vs string-array loose membership loop.
pub(super) fn lower_in_array_int_needle_string_array_aarch64(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_int_str_loop");
    let not_numeric_label = ctx.next_label("in_array_int_str_not_numeric");
    let found_label = ctx.next_label("in_array_int_str_found");
    let end_label = ctx.next_label("in_array_int_str_end");
    let done_label = ctx.next_label("in_array_int_str_done");

    ctx.load_value_to_reg(needle, "x11")?;
    ctx.emitter.instruction("scvtf d1, x11");                                   // promote the integer needle for PHP numeric-string comparison
    ctx.load_value_to_reg(array, "x10")?;
    ctx.emitter.instruction("ldr x9, [x10]");                                   // load indexed string-array length before scanning payload slots
    ctx.emitter.instruction("add x10, x10, #24");                               // point at the first indexed string payload slot
    ctx.emitter.instruction("mov x12, #0");                                     // start the numeric-string membership scan at index zero
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp x12, x9");                                     // compare the scan index against indexed-array length
    ctx.emitter.instruction(&format!("b.ge {}", end_label));                    // finish with false after all string elements are scanned
    ctx.emitter.instruction("lsl x13, x12, #4");                                // scale the element index by the 16-byte string slot width
    ctx.emitter.instruction("ldr x1, [x10, x13]");                              // load the current string element pointer for parsing
    ctx.emitter.instruction("add x14, x13, #8");                                // compute the current string element length-slot offset
    ctx.emitter.instruction("ldr x2, [x10, x14]");                              // load the current string element length for parsing
    abi::emit_push_reg_pair(ctx.emitter, "x9", "x10");
    abi::emit_push_reg(ctx.emitter, "x12");
    abi::emit_push_float_reg(ctx.emitter, "d1");
    abi::emit_call_label(ctx.emitter, "__rt_str_to_number");
    abi::emit_pop_float_reg(ctx.emitter, "d1");
    abi::emit_pop_reg(ctx.emitter, "x12");
    abi::emit_pop_reg_pair(ctx.emitter, "x9", "x10");
    ctx.emitter.instruction("cmp x0, #0");                                      // non-numeric string elements cannot equal an int needle
    ctx.emitter
        .instruction(&format!("b.eq {}", not_numeric_label));
    ctx.emitter.instruction("fcmp d1, d0");                                     // compare integer needle with parsed string element number
    ctx.emitter.instruction(&format!("b.eq {}", found_label));                  // stop when the numeric values match
    ctx.emitter.label(&not_numeric_label);
    ctx.emitter.instruction("add x12, x12, #1");                                // advance to the next indexed string element
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // continue scanning remaining string payload slots
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov x0, #1");                                      // return true after finding a loose numeric match
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the not-found result after a match
    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("mov x0, #0");                                      // return false when no string element matches
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits the x86_64 int-needle vs string-array loose membership loop.
pub(super) fn lower_in_array_int_needle_string_array_x86_64(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_int_str_loop");
    let not_numeric_label = ctx.next_label("in_array_int_str_not_numeric");
    let found_label = ctx.next_label("in_array_int_str_found");
    let end_label = ctx.next_label("in_array_int_str_end");
    let done_label = ctx.next_label("in_array_int_str_done");

    ctx.load_value_to_reg(needle, "r10")?;
    ctx.emitter.instruction("cvtsi2sd xmm1, r10");                              // promote the integer needle for PHP numeric-string comparison
    ctx.load_value_to_reg(array, "r11")?;
    ctx.emitter.instruction("mov r12, QWORD PTR [r11]");                        // load indexed string-array length before scanning payload slots
    ctx.emitter.instruction("lea r11, [r11 + 24]");                             // point at the first indexed string payload slot
    ctx.emitter.instruction("xor r13d, r13d");                                  // start the numeric-string membership scan at index zero
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp r13, r12");                                    // compare the scan index against indexed-array length
    ctx.emitter.instruction(&format!("jge {}", end_label));                     // finish with false after all string elements are scanned
    ctx.emitter.instruction("mov rcx, r13");                                    // copy the scan index before scaling it to a byte offset
    ctx.emitter.instruction("shl rcx, 4");                                      // scale the element index by the 16-byte string slot width
    ctx.emitter.instruction("mov rax, QWORD PTR [r11 + rcx]");                  // load the current string element pointer for parsing
    ctx.emitter
        .instruction("mov rdx, QWORD PTR [r11 + rcx + 8]"); // load the current string element length for parsing
    abi::emit_push_reg_pair(ctx.emitter, "r11", "r12");
    abi::emit_push_reg(ctx.emitter, "r13");
    abi::emit_push_float_reg(ctx.emitter, "xmm1");
    abi::emit_call_label(ctx.emitter, "__rt_str_to_number");
    abi::emit_pop_float_reg(ctx.emitter, "xmm1");
    abi::emit_pop_reg(ctx.emitter, "r13");
    abi::emit_pop_reg_pair(ctx.emitter, "r11", "r12");
    ctx.emitter.instruction("test rax, rax");                                   // non-numeric string elements cannot equal an int needle
    ctx.emitter
        .instruction(&format!("je {}", not_numeric_label));
    ctx.emitter.instruction("ucomisd xmm1, xmm0");                              // compare integer needle with parsed string element number
    ctx.emitter
        .instruction(&format!("jp {}", not_numeric_label)); // unordered parsed values are never equal
    ctx.emitter.instruction(&format!("je {}", found_label));                    // stop when the numeric values match
    ctx.emitter.label(&not_numeric_label);
    ctx.emitter.instruction("add r13, 1");                                      // advance to the next indexed string element
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // continue scanning remaining string payload slots
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov rax, 1");                                      // return true after finding a loose numeric match
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the not-found result after a match
    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("xor eax, eax");                                    // return false when no string element matches
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Lowers loose string-needle membership in a bool array using PHP string truthiness.
pub(super) fn lower_in_array_string_needle_bool_array(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_in_array_string_needle_bool_array_aarch64(ctx, needle, array),
        Arch::X86_64 => lower_in_array_string_needle_bool_array_x86_64(ctx, needle, array),
    }
}

/// Emits the AArch64 string-needle vs bool-array loose membership loop.
pub(super) fn lower_in_array_string_needle_bool_array_aarch64(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
) -> Result<()> {
    ctx.load_string_value_to_regs(needle, "x1", "x2")?;
    emit_string_regs_truthiness_to_reg(ctx, "x1", "x2", "x11");
    lower_in_array_bool_array_with_preloaded_needle_aarch64(ctx, array, "x11")
}

/// Emits the x86_64 string-needle vs bool-array loose membership loop.
pub(super) fn lower_in_array_string_needle_bool_array_x86_64(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
) -> Result<()> {
    ctx.load_string_value_to_regs(needle, "rax", "rdx")?;
    emit_string_regs_truthiness_to_reg(ctx, "rax", "rdx", "r10");
    lower_in_array_bool_array_with_preloaded_needle_x86_64(ctx, array, "r10")
}

/// Lowers loose bool-needle membership in a string array using PHP string truthiness.
pub(super) fn lower_in_array_bool_needle_string_array(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_in_array_bool_needle_string_array_aarch64(ctx, needle, array),
        Arch::X86_64 => lower_in_array_bool_needle_string_array_x86_64(ctx, needle, array),
    }
}

/// Emits the AArch64 bool-needle vs string-array loose membership loop.
pub(super) fn lower_in_array_bool_needle_string_array_aarch64(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_bool_str_loop");
    let found_label = ctx.next_label("in_array_bool_str_found");
    let end_label = ctx.next_label("in_array_bool_str_end");
    let done_label = ctx.next_label("in_array_bool_str_done");

    ctx.load_value_to_reg(needle, "x11")?;
    emit_reg_nonzero_bool(ctx, "x11");
    ctx.load_value_to_reg(array, "x10")?;
    ctx.emitter.instruction("ldr x9, [x10]");                                   // load indexed string-array length before scanning payload slots
    ctx.emitter.instruction("add x10, x10, #24");                               // point at the first indexed string payload slot
    ctx.emitter.instruction("mov x12, #0");                                     // start the string truthiness scan at index zero
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp x12, x9");                                     // compare the scan index against indexed-array length
    ctx.emitter.instruction(&format!("b.ge {}", end_label));                    // finish with false after all string elements are scanned
    ctx.emitter.instruction("lsl x13, x12, #4");                                // scale the element index by the 16-byte string slot width
    ctx.emitter.instruction("ldr x1, [x10, x13]");                              // load the current string element pointer
    ctx.emitter.instruction("add x14, x13, #8");                                // compute the current string element length-slot offset
    ctx.emitter.instruction("ldr x2, [x10, x14]");                              // load the current string element length
    emit_string_regs_truthiness_to_reg(ctx, "x1", "x2", "x13");
    ctx.emitter.instruction("cmp x13, x11");                                    // compare element truthiness against needle truthiness
    ctx.emitter.instruction(&format!("b.eq {}", found_label));                  // stop as soon as a loosely equal string is found
    ctx.emitter.instruction("add x12, x12, #1");                                // advance to the next indexed string element
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // continue scanning remaining string payload slots
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov x0, #1");                                      // return true after finding a loose truthiness match
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the not-found result after a match
    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("mov x0, #0");                                      // return false when no string element matches
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits the x86_64 bool-needle vs string-array loose membership loop.
pub(super) fn lower_in_array_bool_needle_string_array_x86_64(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_bool_str_loop");
    let found_label = ctx.next_label("in_array_bool_str_found");
    let end_label = ctx.next_label("in_array_bool_str_end");
    let done_label = ctx.next_label("in_array_bool_str_done");

    ctx.load_value_to_reg(needle, "r10")?;
    emit_reg_nonzero_bool(ctx, "r10");
    ctx.load_value_to_reg(array, "r11")?;
    ctx.emitter.instruction("mov r12, QWORD PTR [r11]");                        // load indexed string-array length before scanning payload slots
    ctx.emitter.instruction("lea r11, [r11 + 24]");                             // point at the first indexed string payload slot
    ctx.emitter.instruction("xor r13d, r13d");                                  // start the string truthiness scan at index zero
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp r13, r12");                                    // compare the scan index against indexed-array length
    ctx.emitter.instruction(&format!("jge {}", end_label));                     // finish with false after all string elements are scanned
    ctx.emitter.instruction("mov rcx, r13");                                    // copy the scan index before scaling it to a byte offset
    ctx.emitter.instruction("shl rcx, 4");                                      // scale the element index by the 16-byte string slot width
    ctx.emitter.instruction("mov rax, QWORD PTR [r11 + rcx]");                  // load the current string element pointer
    ctx.emitter
        .instruction("mov rdx, QWORD PTR [r11 + rcx + 8]"); // load the current string element length
    emit_string_regs_truthiness_to_reg(ctx, "rax", "rdx", "r15");
    ctx.emitter.instruction("cmp r15, r10");                                    // compare element truthiness against needle truthiness
    ctx.emitter.instruction(&format!("je {}", found_label));                    // stop as soon as a loosely equal string is found
    ctx.emitter.instruction("add r13, 1");                                      // advance to the next indexed string element
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // continue scanning remaining string payload slots
    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov rax, 1");                                      // return true after finding a loose truthiness match
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the not-found result after a match
    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("xor eax, eax");                                    // return false when no string element matches
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Branches to `label` when the boolean value operand is true.
pub(super) fn branch_if_bool_value_true(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    label: &str,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(value, "x9")?;
            ctx.emitter.instruction("cmp x9, #0");                              // test the runtime strict flag
            ctx.emitter.instruction(&format!("b.ne {}", label));                // non-zero strict flag selects `===` membership
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(value, "r10")?;
            ctx.emitter.instruction("test r10, r10");                           // test the runtime strict flag
            ctx.emitter.instruction(&format!("jne {}", label));                 // non-zero strict flag selects `===` membership
        }
    }
    Ok(())
}

/// Rewrites an integer register to PHP bool truthiness, where zero is false.
pub(super) fn emit_reg_nonzero_bool(ctx: &mut FunctionContext<'_>, reg: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #0", reg));               // compare scalar value against zero for truthiness
            ctx.emitter.instruction(&format!("cset {}, ne", reg));              // materialize nonzero truthiness in the same register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {}, {}", reg, reg));         // compare scalar value against zero for truthiness
            ctx.emitter.instruction("setne al");                                // materialize nonzero truthiness in the low byte
            ctx.emitter.instruction(&format!("movzx {}, al", reg));             // widen truthiness into the requested register
        }
    }
}

/// Materializes PHP string truthiness for a pointer/length pair into `out_reg`.
pub(super) fn emit_string_regs_truthiness_to_reg(
    ctx: &mut FunctionContext<'_>,
    ptr_reg: &str,
    len_reg: &str,
    out_reg: &str,
) {
    let falsy_label = ctx.next_label("in_array_str_falsy");
    let truthy_label = ctx.next_label("in_array_str_truthy");
    let done_label = ctx.next_label("in_array_str_truth_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cbz {}, {}", len_reg, falsy_label)); // empty strings are falsy
            ctx.emitter.instruction(&format!("cmp {}, #1", len_reg));           // check whether this can be the special string "0"
            ctx.emitter.instruction(&format!("b.ne {}", truthy_label));         // non-empty strings longer than one byte are truthy
            ctx.emitter.instruction(&format!("ldrb w15, [{}]", ptr_reg));       // load the only string byte for the PHP "0" exception
            ctx.emitter.instruction("cmp w15, #48");                            // compare the byte with ASCII '0'
            ctx.emitter.instruction(&format!("b.eq {}", falsy_label));          // the exact string "0" is falsy
            ctx.emitter.label(&truthy_label);
            ctx.emitter.instruction(&format!("mov {}, #1", out_reg));           // materialize string truthiness as true
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the falsy path
            ctx.emitter.label(&falsy_label);
            ctx.emitter.instruction(&format!("mov {}, #0", out_reg));           // materialize string truthiness as false
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("test {}, {}", len_reg, len_reg)); // empty strings are falsy
            ctx.emitter.instruction(&format!("je {}", falsy_label));            // branch to the falsy path for empty strings
            ctx.emitter.instruction(&format!("cmp {}, 1", len_reg));            // check whether this can be the special string "0"
            ctx.emitter.instruction(&format!("jne {}", truthy_label));          // non-empty strings longer than one byte are truthy
            ctx.emitter
                .instruction(&format!("movzx r9d, BYTE PTR [{}]", ptr_reg)); // load the only string byte for the PHP "0" exception
            ctx.emitter.instruction("cmp r9d, 48");                             // compare the byte with ASCII '0'
            ctx.emitter.instruction(&format!("je {}", falsy_label));            // the exact string "0" is falsy
            ctx.emitter.label(&truthy_label);
            ctx.emitter.instruction(&format!("mov {}, 1", out_reg));            // materialize string truthiness as true
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the falsy path
            ctx.emitter.label(&falsy_label);
            ctx.emitter
                .instruction(&format!("xor {}, {}", out_reg, out_reg)); // materialize string truthiness as false
            ctx.emitter.label(&done_label);
        }
    }
}

