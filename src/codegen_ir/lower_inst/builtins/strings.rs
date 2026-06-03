//! Purpose:
//! Lowers string-returning scalar builtins for the EIR backend.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - Runtime helpers keep owning returned string storage; this module only
//!   materializes target ABI arguments from EIR SSA slots.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};

/// Lowers a one-argument string builtin that directly delegates to a runtime helper.
pub(super) fn lower_unary_string_runtime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    load_single_string_arg(ctx, inst, name)?;
    abi::emit_call_label(ctx.emitter, runtime_label);
    store_if_result(ctx, inst)
}

/// Lowers `ucfirst()` by copying the string and uppercasing the first ASCII byte.
pub(super) fn lower_ucfirst(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    load_single_string_arg(ctx, inst, "ucfirst")?;
    abi::emit_call_label(ctx.emitter, "__rt_strcopy");
    emit_first_char_case_adjust(ctx, "ucfirst", 97, 122, FirstCharAdjust::Uppercase);
    store_if_result(ctx, inst)
}

/// Lowers `lcfirst()` by copying the string and lowercasing the first ASCII byte.
pub(super) fn lower_lcfirst(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    load_single_string_arg(ctx, inst, "lcfirst")?;
    abi::emit_call_label(ctx.emitter, "__rt_strcopy");
    emit_first_char_case_adjust(ctx, "lcfirst", 65, 90, FirstCharAdjust::Lowercase);
    store_if_result(ctx, inst)
}

/// Lowers `trim()`/`ltrim()`/`rtrim()`/`chop()` for default and explicit masks.
pub(super) fn lower_trim_like(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    default_runtime_label: &str,
    mask_runtime_label: &str,
) -> Result<()> {
    if inst.operands.is_empty() || inst.operands.len() > 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expected 1 or 2 args, got {}",
            name,
            inst.operands.len()
        )));
    }
    let source = expect_operand(inst, 0)?;
    let ptr_reg = string_ptr_reg(ctx);
    let len_reg = string_len_reg(ctx);
    ctx.load_string_value_to_regs(source, ptr_reg, len_reg)?;
    if inst.operands.len() == 1 {
        abi::emit_call_label(ctx.emitter, default_runtime_label);
    } else {
        lower_trim_mask_arg(ctx, inst, name)?;
        abi::emit_call_label(ctx.emitter, mask_runtime_label);
    }
    store_if_result(ctx, inst)
}

/// Lowers a two-argument string builtin that directly delegates to a runtime helper.
pub(super) fn lower_binary_string_runtime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    load_binary_string_args(ctx, inst, name)?;
    abi::emit_call_label(ctx.emitter, runtime_label);
    store_if_result(ctx, inst)
}

/// Lowers `hash(algo, data)` through the shared runtime digest dispatcher.
pub(super) fn lower_hash(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() != 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "hash expected 2 args, got {}",
            inst.operands.len()
        )));
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_hash_aarch64(ctx, inst)?,
        Arch::X86_64 => lower_hash_x86_64(ctx, inst)?,
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash");
    store_if_result(ctx, inst)
}

/// Lowers `str_contains()` through `strpos()` and converts found positions to bool.
pub(super) fn lower_str_contains(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    load_binary_string_args(ctx, inst, "str_contains")?;
    abi::emit_call_label(ctx.emitter, "__rt_strpos");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // check whether strpos() found the needle at any non-negative position
            ctx.emitter.instruction("cset x0, ge");                             // normalize the signed strpos() result into a PHP boolean
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 0");                              // check whether strpos() found the needle at any non-negative position
            ctx.emitter.instruction("setge al");                                // normalize the signed strpos() result into the low boolean byte
            ctx.emitter.instruction("movzx eax, al");                           // widen the normalized boolean byte into the integer result register
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `strpos()`/`strrpos()` and boxes position-or-false results as Mixed.
pub(super) fn lower_string_position(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    load_binary_string_args(ctx, inst, name)?;
    abi::emit_call_label(ctx.emitter, runtime_label);
    box_search_result(ctx, name);
    store_if_result(ctx, inst)
}

/// Lowers `substr(string, offset, length?)` with target-local pointer arithmetic.
pub(super) fn lower_substr(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() < 2 || inst.operands.len() > 3 {
        return Err(CodegenIrError::invalid_module(format!(
            "substr expected 2 or 3 args, got {}",
            inst.operands.len()
        )));
    }
    let neg_done = ctx.next_label("substr_neg_done");
    let len_done = ctx.next_label("substr_len_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_substr_aarch64(ctx, inst, &neg_done, &len_done)?,
        Arch::X86_64 => lower_substr_x86_64(ctx, inst, &neg_done, &len_done)?,
    }
    store_if_result(ctx, inst)
}

/// Lowers `str_repeat(string, times)` through the shared runtime helper.
pub(super) fn lower_str_repeat(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() != 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "str_repeat expected 2 args, got {}",
            inst.operands.len()
        )));
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_str_repeat_aarch64(ctx, inst)?,
        Arch::X86_64 => lower_str_repeat_x86_64(ctx, inst)?,
    }
    abi::emit_call_label(ctx.emitter, "__rt_str_repeat");
    store_if_result(ctx, inst)
}

/// Lowers `strstr(haystack, needle)` by searching and returning the matching suffix.
pub(super) fn lower_strstr(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() != 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "strstr expected 2 args, got {}",
            inst.operands.len()
        )));
    }
    let found_label = ctx.next_label("strstr_found");
    let end_label = ctx.next_label("strstr_end");
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_strstr_aarch64(ctx, inst, &found_label, &end_label)?,
        Arch::X86_64 => lower_strstr_x86_64(ctx, inst, &found_label, &end_label)?,
    }
    ctx.emitter.label(&end_label);
    store_if_result(ctx, inst)
}

/// Lowers `wordwrap(string, width?, break?, cut?)` through the shared runtime helper.
pub(super) fn lower_wordwrap(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.is_empty() || inst.operands.len() > 4 {
        return Err(CodegenIrError::invalid_module(format!(
            "wordwrap expected 1 to 4 args, got {}",
            inst.operands.len()
        )));
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_wordwrap_aarch64(ctx, inst)?,
        Arch::X86_64 => lower_wordwrap_x86_64(ctx, inst)?,
    }
    abi::emit_call_label(ctx.emitter, "__rt_wordwrap");
    store_if_result(ctx, inst)
}

/// Lowers `str_pad(string, length, pad_string?, pad_type?)` through the shared runtime helper.
pub(super) fn lower_str_pad(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() < 2 || inst.operands.len() > 4 {
        return Err(CodegenIrError::invalid_module(format!(
            "str_pad expected 2 to 4 args, got {}",
            inst.operands.len()
        )));
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_str_pad_aarch64(ctx, inst)?,
        Arch::X86_64 => lower_str_pad_x86_64(ctx, inst)?,
    }
    abi::emit_call_label(ctx.emitter, "__rt_str_pad");
    store_if_result(ctx, inst)
}

/// Lowers `ord()` by returning the first byte of a string or zero for empty input.
pub(super) fn lower_ord(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    load_single_string_arg(ctx, inst, "ord")?;
    let empty_label = ctx.next_label("ord_empty");
    let done_label = ctx.next_label("ord_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x2, {}", empty_label));       // return zero when ord() receives an empty string
            ctx.emitter.instruction("ldrb w0, [x1]");                           // load the first byte as an unsigned integer
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the empty-string fallback after loading the first byte
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rdx, rdx");                           // return zero when ord() receives an empty string
            ctx.emitter.instruction(&format!("jz {}", empty_label));            // branch to the empty-string fallback when the length is zero
            ctx.emitter.instruction("movzx eax, BYTE PTR [rax]");               // load the first byte as an unsigned integer
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the empty-string fallback after loading the first byte
        }
    }
    ctx.emitter.label(&empty_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers `chr()` by converting an integer code point into a one-byte string.
pub(super) fn lower_chr(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::invalid_module(format!(
            "chr expected 1 arg, got {}",
            inst.operands.len()
        )));
    }
    let value = expect_operand(inst, 0)?;
    load_as_int(ctx, value, "chr")?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the character code to the x86_64 runtime helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_chr");
    store_if_result(ctx, inst)
}

/// Lowers `number_format()` by arranging its runtime helper arguments.
pub(super) fn lower_number_format(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.is_empty() || inst.operands.len() > 4 {
        return Err(CodegenIrError::invalid_module(format!(
            "number_format expected 1 to 4 args, got {}",
            inst.operands.len()
        )));
    }

    let number = expect_operand(inst, 0)?;
    load_as_float(ctx, number, "number_format")?;
    abi::emit_push_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));

    push_decimal_count(ctx, inst)?;
    push_separator_byte(ctx, inst, 2, 46, false, "decimal separator")?;
    push_separator_byte(ctx, inst, 3, 44, true, "thousands separator")?;
    pop_number_format_args(ctx);
    abi::emit_call_label(ctx.emitter, "__rt_number_format");
    store_if_result(ctx, inst)
}

/// Describes how the first-byte ASCII case helper mutates matched characters.
enum FirstCharAdjust {
    Uppercase,
    Lowercase,
}

/// Returns the target register holding string-result pointers.
fn string_ptr_reg(ctx: &FunctionContext<'_>) -> &'static str {
    match ctx.emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rax",
    }
}

/// Returns the target register holding string-result lengths.
fn string_len_reg(ctx: &FunctionContext<'_>) -> &'static str {
    match ctx.emitter.target.arch {
        Arch::AArch64 => "x2",
        Arch::X86_64 => "rdx",
    }
}

/// Loads the sole argument for a string-transform builtin into string result registers.
fn load_single_string_arg(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expected 1 arg, got {}",
            name,
            inst.operands.len()
        )));
    }
    let value = expect_operand(inst, 0)?;
    match ctx.load_value_to_result(value)?.codegen_repr() {
        PhpType::Str => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name, other
        ))),
    }
}

/// Preserves the trim source string while loading the explicit character mask.
fn lower_trim_mask_arg(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    let mask = expect_operand(inst, 1)?;
    if ctx.value_php_type(mask)? != PhpType::Str {
        return Err(CodegenIrError::unsupported(format!(
            "{} mask for PHP type {:?}",
            name,
            ctx.value_php_type(mask)?
        )));
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x1, [sp, #-16]!");                     // preserve the source string pointer while loading the trim mask
            ctx.emitter.instruction("str x2, [sp, #-16]!");                     // preserve the source string length while loading the trim mask
            ctx.load_string_value_to_regs(mask, "x1", "x2")?;
            ctx.emitter.instruction("mov x3, x1");                              // pass the trim-mask pointer as the secondary string argument
            ctx.emitter.instruction("mov x4, x2");                              // pass the trim-mask length as the secondary string argument
            ctx.emitter.instruction("ldr x2, [sp], #16");                       // restore the source string length after loading the mask
            ctx.emitter.instruction("ldr x1, [sp], #16");                       // restore the source string pointer after loading the mask
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            ctx.load_string_value_to_regs(mask, "rax", "rdx")?;
            ctx.emitter.instruction("mov rdi, rax");                            // pass the trim-mask pointer as the secondary string argument
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the trim-mask length as the secondary string argument
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
    }
    Ok(())
}

/// Materializes two string operands into the runtime helper's target ABI registers.
fn load_binary_string_args(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    if inst.operands.len() != 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expected 2 args, got {}",
            name,
            inst.operands.len()
        )));
    }
    let first = expect_string_operand(ctx, inst, 0, name)?;
    let second = expect_string_operand(ctx, inst, 1, name)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_string_value_to_regs(first, "x1", "x2")?;
            ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                 // preserve the first string pointer and length while loading the second
            ctx.load_string_value_to_regs(second, "x1", "x2")?;
            ctx.emitter.instruction("mov x3, x1");                              // pass the second string pointer as the secondary string argument
            ctx.emitter.instruction("mov x4, x2");                              // pass the second string length as the secondary string argument
            ctx.emitter.instruction("ldp x1, x2, [sp], #16");                   // restore the first string pointer and length into primary argument registers
        }
        Arch::X86_64 => {
            ctx.load_string_value_to_regs(first, "rax", "rdx")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            ctx.load_string_value_to_regs(second, "rax", "rdx")?;
            ctx.emitter.instruction("mov rcx, rdx");                            // pass the second string length as the fourth SysV string argument
            ctx.emitter.instruction("mov rdx, rax");                            // pass the second string pointer as the third SysV string argument
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
        }
    }
    Ok(())
}

/// Returns a string operand after validating the EIR builtin call shape.
fn expect_string_operand(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
    index: usize,
    name: &str,
) -> Result<ValueId> {
    let value = expect_operand(inst, index)?;
    let ty = ctx.value_php_type(value)?;
    if ty == PhpType::Str {
        return Ok(value);
    }
    Err(CodegenIrError::unsupported(format!(
        "{} arg {} for PHP type {:?}",
        name,
        index + 1,
        ty
    )))
}

/// Emits the AArch64 inline substring pointer/length calculation.
fn lower_substr_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    neg_done: &str,
    len_done: &str,
) -> Result<()> {
    load_substr_string_and_offset_aarch64(ctx, inst)?;
    if inst.operands.len() >= 3 {
        let length = expect_operand(inst, 2)?;
        load_as_int(ctx, length, "substr length")?;
        ctx.emitter.instruction("mov x3, x0");                                  // move the explicit substring length into the clamp register
    } else {
        ctx.emitter.instruction("mov x3, #-1");                                 // use -1 as the sentinel for an omitted substring length
    }
    ctx.emitter.instruction("ldr x0, [sp], #16");                               // restore the substring offset after optional length materialization
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the source string pointer and length
    ctx.emitter.instruction("cmp x0, #0");                                      // check whether the requested offset is negative
    ctx.emitter.instruction(&format!("b.ge {}", neg_done));                     // skip tail-relative offset adjustment for non-negative offsets
    ctx.emitter.instruction("add x0, x2, x0");                                  // convert the negative offset into a tail-relative byte index
    ctx.emitter.instruction("cmp x0, #0");                                      // check whether the tail-relative offset still points before the string
    ctx.emitter.instruction("csel x0, xzr, x0, lt");                            // clamp underflowing offsets back to the start of the string
    ctx.emitter.label(neg_done);
    ctx.emitter.instruction("cmp x0, x2");                                      // compare the final offset against the full source-string length
    ctx.emitter.instruction("csel x0, x2, x0, gt");                             // clamp offsets past the end to the source-string length
    ctx.emitter.instruction("add x1, x1, x0");                                  // advance the result pointer to the selected substring start
    ctx.emitter.instruction("sub x2, x2, x0");                                  // compute the remaining byte length after the selected offset
    ctx.emitter.instruction("cmn x3, #1");                                      // check whether the optional length argument was omitted
    ctx.emitter.instruction(&format!("b.eq {}", len_done));                     // keep the full remaining tail when no explicit length was provided
    ctx.emitter.instruction("cmp x3, #0");                                      // check whether the requested substring length is negative
    ctx.emitter.instruction("csel x3, xzr, x3, lt");                            // clamp negative requested lengths to zero
    ctx.emitter.instruction("cmp x3, x2");                                      // compare requested length against the remaining tail length
    ctx.emitter.instruction("csel x2, x3, x2, lt");                             // shrink the result length when the requested length is shorter
    ctx.emitter.label(len_done);
    Ok(())
}

/// Loads the source string and offset for AArch64 `substr()` lowering.
fn load_substr_string_and_offset_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let source = expect_string_operand(ctx, inst, 0, "substr")?;
    let offset = expect_operand(inst, 1)?;
    ctx.load_string_value_to_regs(source, "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the source string while materializing numeric arguments
    load_as_int(ctx, offset, "substr offset")?;
    ctx.emitter.instruction("str x0, [sp, #-16]!");                             // preserve the substring offset while materializing the optional length
    Ok(())
}

/// Emits the x86_64 inline substring pointer/length calculation.
fn lower_substr_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    neg_done: &str,
    len_done: &str,
) -> Result<()> {
    load_substr_string_and_offset_x86_64(ctx, inst)?;
    if inst.operands.len() >= 3 {
        let length = expect_operand(inst, 2)?;
        load_as_int(ctx, length, "substr length")?;
        ctx.emitter.instruction("mov rcx, rax");                                // move the explicit substring length into the clamp register
    } else {
        abi::emit_load_int_immediate(ctx.emitter, "rcx", -1);
    }
    abi::emit_pop_reg(ctx.emitter, "rax");
    abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
    ctx.emitter.instruction("cmp rax, 0");                                      // check whether the requested offset is negative
    ctx.emitter.instruction(&format!("jge {}", neg_done));                      // skip tail-relative offset adjustment for non-negative offsets
    ctx.emitter.instruction("add rax, rsi");                                    // convert the negative offset into a tail-relative byte index
    ctx.emitter.instruction("cmp rax, 0");                                      // check whether the tail-relative offset still points before the string
    ctx.emitter.instruction("mov r8, 0");                                       // materialize zero for offset and length clamping
    ctx.emitter.instruction("cmovl rax, r8");                                   // clamp underflowing offsets back to the start of the string
    ctx.emitter.label(neg_done);
    ctx.emitter.instruction("cmp rax, rsi");                                    // compare the final offset against the full source-string length
    ctx.emitter.instruction("cmovg rax, rsi");                                  // clamp offsets past the end to the source-string length
    ctx.emitter.instruction("add rdi, rax");                                    // advance the result pointer to the selected substring start
    ctx.emitter.instruction("sub rsi, rax");                                    // compute the remaining byte length after the selected offset
    ctx.emitter.instruction("cmp rcx, -1");                                     // check whether the optional length argument was omitted
    ctx.emitter.instruction(&format!("je {}", len_done));                       // keep the full remaining tail when no explicit length was provided
    ctx.emitter.instruction("cmp rcx, 0");                                      // check whether the requested substring length is negative
    ctx.emitter.instruction("mov r8, 0");                                       // materialize zero for negative length clamping
    ctx.emitter.instruction("cmovl rcx, r8");                                   // clamp negative requested lengths to zero
    ctx.emitter.instruction("cmp rcx, rsi");                                    // compare requested length against the remaining tail length
    ctx.emitter.instruction("cmovl rsi, rcx");                                  // shrink the result length when the requested length is shorter
    ctx.emitter.label(len_done);
    ctx.emitter.instruction("mov rax, rdi");                                    // return the selected substring pointer in the string result register
    ctx.emitter.instruction("mov rdx, rsi");                                    // return the selected substring length in the string result register
    Ok(())
}

/// Loads the source string and offset for x86_64 `substr()` lowering.
fn load_substr_string_and_offset_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let source = expect_string_operand(ctx, inst, 0, "substr")?;
    let offset = expect_operand(inst, 1)?;
    ctx.load_string_value_to_regs(source, "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_as_int(ctx, offset, "substr offset")?;
    abi::emit_push_reg(ctx.emitter, "rax");
    Ok(())
}

/// Materializes AArch64 `str_repeat()` runtime arguments.
fn lower_str_repeat_aarch64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let source = expect_string_operand(ctx, inst, 0, "str_repeat")?;
    let times = expect_operand(inst, 1)?;
    ctx.load_string_value_to_regs(source, "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the source string while materializing the repeat count
    load_as_int(ctx, times, "str_repeat times")?;
    ctx.emitter.instruction("mov x3, x0");                                      // pass the repeat count as the third string-helper argument
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the source string into runtime argument registers
    Ok(())
}

/// Materializes x86_64 `str_repeat()` runtime arguments.
fn lower_str_repeat_x86_64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let source = expect_string_operand(ctx, inst, 0, "str_repeat")?;
    let times = expect_operand(inst, 1)?;
    ctx.load_string_value_to_regs(source, "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_as_int(ctx, times, "str_repeat times")?;
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the repeat count as the extra x86_64 runtime argument
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    Ok(())
}

/// Emits AArch64 `strstr()` search and suffix reconstruction.
fn lower_strstr_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    found_label: &str,
    end_label: &str,
) -> Result<()> {
    let haystack = expect_string_operand(ctx, inst, 0, "strstr")?;
    let needle = expect_string_operand(ctx, inst, 1, "strstr")?;
    ctx.load_string_value_to_regs(haystack, "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the haystack while materializing the needle string
    ctx.load_string_value_to_regs(needle, "x1", "x2")?;
    ctx.emitter.instruction("mov x3, x1");                                      // pass the needle pointer as the secondary string argument
    ctx.emitter.instruction("mov x4, x2");                                      // pass the needle length as the secondary string argument
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the haystack into primary string argument registers
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the haystack while strpos() returns the match offset
    abi::emit_call_label(ctx.emitter, "__rt_strpos");
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the haystack for suffix reconstruction
    ctx.emitter.instruction("cmp x0, #0");                                      // check whether strpos() returned a valid match offset
    ctx.emitter.instruction(&format!("b.ge {}", found_label));                  // build the matching suffix when the needle was found
    ctx.emitter.instruction("mov x1, #0");                                      // return a null pointer for the empty not-found string
    ctx.emitter.instruction("mov x2, #0");                                      // return zero length for the empty not-found string
    ctx.emitter.instruction(&format!("b {}", end_label));                       // skip suffix pointer adjustment for a miss
    ctx.emitter.label(found_label);
    ctx.emitter.instruction("add x1, x1, x0");                                  // advance the haystack pointer to the matching suffix
    ctx.emitter.instruction("sub x2, x2, x0");                                  // shrink the haystack length to the matching suffix length
    Ok(())
}

/// Emits x86_64 `strstr()` search and suffix reconstruction.
fn lower_strstr_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    found_label: &str,
    end_label: &str,
) -> Result<()> {
    let haystack = expect_string_operand(ctx, inst, 0, "strstr")?;
    let needle = expect_string_operand(ctx, inst, 1, "strstr")?;
    ctx.load_string_value_to_regs(haystack, "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    ctx.load_string_value_to_regs(needle, "rax", "rdx")?;
    ctx.emitter.instruction("mov r8, rax");                                     // preserve the needle pointer while restoring the haystack
    ctx.emitter.instruction("mov r9, rdx");                                     // preserve the needle length while restoring the haystack
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the haystack pointer as the first SysV string argument
    ctx.emitter.instruction("mov rsi, rdx");                                    // pass the haystack length as the second SysV string argument
    ctx.emitter.instruction("mov rdx, r8");                                     // pass the needle pointer as the third SysV string argument
    ctx.emitter.instruction("mov rcx, r9");                                     // pass the needle length as the fourth SysV string argument
    abi::emit_call_label(ctx.emitter, "__rt_strpos");
    ctx.emitter.instruction("mov r8, rax");                                     // preserve the signed match offset while restoring the haystack
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    ctx.emitter.instruction("cmp r8, 0");                                       // check whether strpos() returned a valid match offset
    ctx.emitter.instruction(&format!("jge {}", found_label));                   // build the matching suffix when the needle was found
    ctx.emitter.instruction("xor eax, eax");                                    // return a null pointer for the empty not-found string
    ctx.emitter.instruction("xor edx, edx");                                    // return zero length for the empty not-found string
    ctx.emitter.instruction(&format!("jmp {}", end_label));                     // skip suffix pointer adjustment for a miss
    ctx.emitter.label(found_label);
    ctx.emitter.instruction("add rax, r8");                                     // advance the haystack pointer to the matching suffix
    ctx.emitter.instruction("sub rdx, r8");                                     // shrink the haystack length to the matching suffix length
    Ok(())
}

/// Materializes AArch64 `hash()` runtime arguments.
fn lower_hash_aarch64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let algorithm = expect_string_operand(ctx, inst, 0, "hash")?;
    let data = expect_string_operand(ctx, inst, 1, "hash")?;
    ctx.load_string_value_to_regs(algorithm, "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the algorithm string while materializing the data string
    ctx.load_string_value_to_regs(data, "x1", "x2")?;
    ctx.emitter.instruction("mov x3, x1");                                      // pass the data string pointer as the secondary hash argument
    ctx.emitter.instruction("mov x4, x2");                                      // pass the data string length as the secondary hash argument
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the algorithm string into primary hash argument registers
    Ok(())
}

/// Materializes x86_64 `hash()` runtime arguments.
fn lower_hash_x86_64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let algorithm = expect_string_operand(ctx, inst, 0, "hash")?;
    let data = expect_string_operand(ctx, inst, 1, "hash")?;
    ctx.load_string_value_to_regs(algorithm, "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    ctx.load_string_value_to_regs(data, "rax", "rdx")?;
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the data string pointer as the secondary hash argument
    ctx.emitter.instruction("mov rsi, rdx");                                    // pass the data string length as the secondary hash argument
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    Ok(())
}

/// Materializes AArch64 `str_pad()` runtime arguments.
fn lower_str_pad_aarch64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let input = expect_string_operand(ctx, inst, 0, "str_pad")?;
    let target_length = expect_operand(inst, 1)?;
    ctx.load_string_value_to_regs(input, "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the input string while materializing length and pad arguments
    load_as_int(ctx, target_length, "str_pad length")?;
    abi::emit_push_reg(ctx.emitter, "x0");
    materialize_str_pad_pad_string_aarch64(ctx, inst)?;
    materialize_str_pad_type_aarch64(ctx, inst)?;
    ctx.emitter.instruction("ldp x3, x4, [sp], #16");                           // restore the pad string into secondary runtime argument registers
    abi::emit_pop_reg(ctx.emitter, "x5");
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the input string into primary runtime argument registers
    Ok(())
}

/// Materializes the AArch64 `str_pad()` pad-string argument.
fn materialize_str_pad_pad_string_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() >= 3 {
        let pad_string = expect_string_operand(ctx, inst, 2, "str_pad")?;
        ctx.load_string_value_to_regs(pad_string, "x1", "x2")?;
    } else {
        let (label, len) = ctx.data.add_string(b" ");
        abi::emit_symbol_address(ctx.emitter, "x1", &label);
        abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
    }
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the pad string while materializing the optional pad type
    Ok(())
}

/// Materializes the AArch64 `str_pad()` pad-type argument.
fn materialize_str_pad_type_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() >= 4 {
        let pad_type = expect_operand(inst, 3)?;
        load_as_int(ctx, pad_type, "str_pad pad_type")?;
        ctx.emitter.instruction("mov x7, x0");                                  // pass the requested STR_PAD mode to the runtime helper
    } else {
        ctx.emitter.instruction("mov x7, #1");                                  // default to STR_PAD_RIGHT when pad_type is omitted
    }
    Ok(())
}

/// Materializes x86_64 `str_pad()` runtime arguments.
fn lower_str_pad_x86_64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let input = expect_string_operand(ctx, inst, 0, "str_pad")?;
    let target_length = expect_operand(inst, 1)?;
    ctx.load_string_value_to_regs(input, "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_as_int(ctx, target_length, "str_pad length")?;
    abi::emit_push_reg(ctx.emitter, "rax");
    materialize_str_pad_pad_string_x86_64(ctx, inst)?;
    materialize_str_pad_type_x86_64(ctx, inst)?;
    abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
    abi::emit_pop_reg(ctx.emitter, "rcx");
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    Ok(())
}

/// Materializes the x86_64 `str_pad()` pad-string argument.
fn materialize_str_pad_pad_string_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() >= 3 {
        let pad_string = expect_string_operand(ctx, inst, 2, "str_pad")?;
        ctx.load_string_value_to_regs(pad_string, "rax", "rdx")?;
    } else {
        let (label, len) = ctx.data.add_string(b" ");
        abi::emit_symbol_address(ctx.emitter, "rax", &label);
        abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
    }
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    Ok(())
}

/// Materializes the x86_64 `str_pad()` pad-type argument.
fn materialize_str_pad_type_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() >= 4 {
        let pad_type = expect_operand(inst, 3)?;
        load_as_int(ctx, pad_type, "str_pad pad_type")?;
        ctx.emitter.instruction("mov r8, rax");                                 // pass the requested STR_PAD mode to the runtime helper
    } else {
        ctx.emitter.instruction("mov r8, 1");                                   // default to STR_PAD_RIGHT when pad_type is omitted
    }
    Ok(())
}

/// Materializes AArch64 `wordwrap()` runtime arguments.
fn lower_wordwrap_aarch64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let input = expect_string_operand(ctx, inst, 0, "wordwrap")?;
    ctx.load_string_value_to_regs(input, "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the input string while materializing width and break arguments
    materialize_wordwrap_width_aarch64(ctx, inst)?;
    materialize_wordwrap_break_aarch64(ctx, inst)?;
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the input string into primary runtime argument registers
    Ok(())
}

/// Materializes the AArch64 wordwrap width argument.
fn materialize_wordwrap_width_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() >= 2 {
        let width = expect_operand(inst, 1)?;
        load_as_int(ctx, width, "wordwrap width")?;
        ctx.emitter.instruction("mov x3, x0");                                  // pass the requested wrap width to the runtime helper
    } else {
        ctx.emitter.instruction("mov x3, #75");                                 // use PHP's default wrap width when omitted
    }
    Ok(())
}

/// Materializes the AArch64 wordwrap break-string argument.
fn materialize_wordwrap_break_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() >= 3 {
        let break_string = expect_string_operand(ctx, inst, 2, "wordwrap")?;
        ctx.load_string_value_to_regs(break_string, "x1", "x2")?;
        ctx.emitter.instruction("mov x4, x1");                                  // pass the break-string pointer to the runtime helper
        ctx.emitter.instruction("mov x5, x2");                                  // pass the break-string length to the runtime helper
    } else {
        let (label, len) = ctx.data.add_string(b"\n");
        abi::emit_symbol_address(ctx.emitter, "x4", &label);
        abi::emit_load_int_immediate(ctx.emitter, "x5", len as i64);
    }
    Ok(())
}

/// Materializes x86_64 `wordwrap()` runtime arguments.
fn lower_wordwrap_x86_64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let input = expect_string_operand(ctx, inst, 0, "wordwrap")?;
    ctx.load_string_value_to_regs(input, "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    materialize_wordwrap_width_x86_64(ctx, inst)?;
    materialize_wordwrap_break_x86_64(ctx, inst)?;
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    Ok(())
}

/// Materializes the x86_64 wordwrap width argument.
fn materialize_wordwrap_width_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() >= 2 {
        let width = expect_operand(inst, 1)?;
        load_as_int(ctx, width, "wordwrap width")?;
        ctx.emitter.instruction("mov rdi, rax");                                // pass the requested wrap width to the runtime helper
    } else {
        ctx.emitter.instruction("mov rdi, 75");                                 // use PHP's default wrap width when omitted
    }
    Ok(())
}

/// Materializes the x86_64 wordwrap break-string argument.
fn materialize_wordwrap_break_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() >= 3 {
        let break_string = expect_string_operand(ctx, inst, 2, "wordwrap")?;
        ctx.load_string_value_to_regs(break_string, "rax", "rdx")?;
        ctx.emitter.instruction("mov rcx, rax");                                // pass the break-string pointer to the runtime helper
        ctx.emitter.instruction("mov r8, rdx");                                 // pass the break-string length to the runtime helper
    } else {
        let (label, len) = ctx.data.add_string(b"\n");
        abi::emit_symbol_address(ctx.emitter, "rcx", &label);
        abi::emit_load_int_immediate(ctx.emitter, "r8", len as i64);
    }
    Ok(())
}

/// Boxes a raw string-search position result into the Mixed pointer representation.
fn box_search_result(ctx: &mut FunctionContext<'_>, label_prefix: &str) {
    let found_label = ctx.next_label(&format!("{}_found", label_prefix));
    let end_label = ctx.next_label(&format!("{}_done", label_prefix));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // distinguish a valid non-negative match offset from the not-found sentinel
            ctx.emitter.instruction(&format!("b.ge {}", found_label));          // box a found offset as an integer result
            ctx.emitter.instruction("mov x1, #0");                              // use zero as the false payload for the mixed bool box
            ctx.emitter.instruction("mov x2, #0");                              // clear the unused high payload word for bool mixed boxes
            ctx.emitter.instruction("mov x0, #3");                              // select runtime tag 3 for a boolean false mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("b {}", end_label));               // skip integer boxing after producing the false result
            ctx.emitter.label(&found_label);
            ctx.emitter.instruction("mov x1, x0");                              // move the found offset into the mixed helper payload register
            ctx.emitter.instruction("mov x2, #0");                              // clear the unused high payload word for integer mixed boxes
            ctx.emitter.instruction("mov x0, #0");                              // select runtime tag 0 for an integer mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&end_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 0");                              // distinguish a valid non-negative match offset from the not-found sentinel
            ctx.emitter.instruction(&format!("jge {}", found_label));           // box a found offset as an integer result
            ctx.emitter.instruction("xor edi, edi");                            // use zero as the false payload for the mixed bool box
            ctx.emitter.instruction("xor esi, esi");                            // clear the unused high payload word for bool mixed boxes
            ctx.emitter.instruction("mov eax, 3");                              // select runtime tag 3 for a boolean false mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("jmp {}", end_label));             // skip integer boxing after producing the false result
            ctx.emitter.label(&found_label);
            ctx.emitter.instruction("mov rdi, rax");                            // move the found offset into the mixed helper payload register
            ctx.emitter.instruction("xor esi, esi");                            // clear the unused high payload word for integer mixed boxes
            ctx.emitter.instruction("xor eax, eax");                            // select runtime tag 0 for an integer mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&end_label);
        }
    }
}

/// Emits target-aware first-byte ASCII case adjustment for `ucfirst()` and `lcfirst()`.
fn emit_first_char_case_adjust(
    ctx: &mut FunctionContext<'_>,
    label_prefix: &str,
    lower_bound: u8,
    upper_bound: u8,
    adjust: FirstCharAdjust,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let done = ctx.next_label(&format!("{}_done", label_prefix));
            ctx.emitter.instruction(&format!("cbz x2, {}", done));              // leave empty strings unchanged because there is no first byte
            ctx.emitter.instruction("ldrb w9, [x1]");                           // load the first byte of the copied string for ASCII case checks
            ctx.emitter.instruction(&format!("cmp w9, #{}", lower_bound));      // compare the first byte against the lower ASCII case bound
            ctx.emitter.instruction(&format!("b.lt {}", done));                 // leave bytes below the case range unchanged
            ctx.emitter.instruction(&format!("cmp w9, #{}", upper_bound));      // compare the first byte against the upper ASCII case bound
            ctx.emitter.instruction(&format!("b.gt {}", done));                 // leave bytes above the case range unchanged
            match adjust {
                FirstCharAdjust::Uppercase => {
                    ctx.emitter.instruction("sub w9, w9, #32");                 // convert lowercase ASCII to uppercase
                }
                FirstCharAdjust::Lowercase => {
                    ctx.emitter.instruction("add w9, w9, #32");                 // convert uppercase ASCII to lowercase
                }
            }
            ctx.emitter.instruction("strb w9, [x1]");                           // store the adjusted first byte into the copied string
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            let done = ctx.next_label(&format!("{}_done", label_prefix));
            ctx.emitter.instruction("test rdx, rdx");                           // leave empty strings unchanged because there is no first byte
            ctx.emitter.instruction(&format!("jz {}", done));                   // skip first-byte mutation for empty strings
            ctx.emitter.instruction("movzx ecx, BYTE PTR [rax]");               // load the first byte of the copied string for ASCII case checks
            ctx.emitter.instruction(&format!("cmp cl, {}", lower_bound));       // compare the first byte against the lower ASCII case bound
            ctx.emitter.instruction(&format!("jb {}", done));                   // leave bytes below the case range unchanged
            ctx.emitter.instruction(&format!("cmp cl, {}", upper_bound));       // compare the first byte against the upper ASCII case bound
            ctx.emitter.instruction(&format!("ja {}", done));                   // leave bytes above the case range unchanged
            match adjust {
                FirstCharAdjust::Uppercase => {
                    ctx.emitter.instruction("sub cl, 32");                      // convert lowercase ASCII to uppercase
                }
                FirstCharAdjust::Lowercase => {
                    ctx.emitter.instruction("add cl, 32");                      // convert uppercase ASCII to lowercase
                }
            }
            ctx.emitter.instruction("mov BYTE PTR [rax], cl");                  // store the adjusted first byte into the copied string
            ctx.emitter.label(&done);
        }
    }
}

/// Pushes the explicit or default decimal-count argument.
fn push_decimal_count(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() >= 2 {
        let decimals = expect_operand(inst, 1)?;
        load_as_int(ctx, decimals, "number_format decimals")?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    }
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    Ok(())
}

/// Pushes a one-byte separator argument, using `default_byte` when it is omitted.
fn push_separator_byte(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    operand_index: usize,
    default_byte: i64,
    empty_string_means_zero: bool,
    name: &str,
) -> Result<()> {
    if inst.operands.len() > operand_index {
        let value = expect_operand(inst, operand_index)?;
        load_separator_byte(ctx, value, empty_string_means_zero, name)?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), default_byte);
    }
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    Ok(())
}

/// Loads the first byte of a separator string into the integer result register.
fn load_separator_byte(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    empty_string_means_zero: bool,
    name: &str,
) -> Result<()> {
    if ctx.value_php_type(value)? != PhpType::Str {
        return Err(CodegenIrError::unsupported(format!(
            "number_format {} for non-string operand",
            name
        )));
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_string_value_to_regs(value, "x1", "x2")?;
            if empty_string_means_zero {
                emit_aarch64_empty_separator_guard(ctx);
            } else {
                ctx.emitter.instruction("ldrb w0, [x1]");                       // load the first byte of the separator string
            }
        }
        Arch::X86_64 => {
            ctx.load_string_value_to_regs(value, "rax", "rdx")?;
            if empty_string_means_zero {
                emit_x86_64_empty_separator_guard(ctx);
            } else {
                ctx.emitter.instruction("movzx eax, BYTE PTR [rax]");           // load the first byte of the separator string
            }
        }
    }
    Ok(())
}

/// Emits the AArch64 empty-string fallback for the optional thousands separator.
fn emit_aarch64_empty_separator_guard(ctx: &mut FunctionContext<'_>) {
    let use_zero = ctx.next_label("nf_sep_zero");
    let done = ctx.next_label("nf_sep_done");
    ctx.emitter.instruction(&format!("cbz x2, {}", use_zero));                  // use the no-separator sentinel when the separator string is empty
    ctx.emitter.instruction("ldrb w0, [x1]");                                   // load the first byte of the non-empty separator string
    ctx.emitter.instruction(&format!("b {}", done));                            // skip the empty-string separator fallback
    ctx.emitter.label(&use_zero);
    abi::emit_load_int_immediate(ctx.emitter, "x0", 0);
    ctx.emitter.label(&done);
}

/// Emits the x86_64 empty-string fallback for the optional thousands separator.
fn emit_x86_64_empty_separator_guard(ctx: &mut FunctionContext<'_>) {
    let use_zero = ctx.next_label("nf_sep_zero");
    let done = ctx.next_label("nf_sep_done");
    ctx.emitter.instruction("test rdx, rdx");                                   // check whether the separator string is empty
    ctx.emitter.instruction(&format!("jz {}", use_zero));                       // use the no-separator sentinel for an empty separator
    ctx.emitter.instruction("movzx eax, BYTE PTR [rax]");                       // load the first byte of the non-empty separator string
    ctx.emitter.instruction(&format!("jmp {}", done));                          // skip the empty-string separator fallback
    ctx.emitter.label(&use_zero);
    abi::emit_load_int_immediate(ctx.emitter, "rax", 0);
    ctx.emitter.label(&done);
}

/// Pops the staged arguments into the runtime helper's target ABI registers.
fn pop_number_format_args(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x3");
            abi::emit_pop_reg(ctx.emitter, "x2");
            abi::emit_pop_reg(ctx.emitter, "x1");
            abi::emit_pop_float_reg(ctx.emitter, "d0");
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "rdx");
            abi::emit_pop_reg(ctx.emitter, "rsi");
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_pop_float_reg(ctx.emitter, "xmm0");
        }
    }
}

/// Loads a concrete scalar value as a floating-point runtime argument.
fn load_as_float(ctx: &mut FunctionContext<'_>, value: ValueId, name: &str) -> Result<()> {
    match ctx.load_value_to_result(value)?.codegen_repr() {
        PhpType::Float => Ok(()),
        PhpType::Int | PhpType::Bool => {
            abi::emit_int_result_to_float_result(ctx.emitter);
            Ok(())
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            abi::emit_int_result_to_float_result(ctx.emitter);
            Ok(())
        }
        PhpType::Str => {
            abi::emit_call_label(ctx.emitter, "__rt_str_to_number");
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name, other
        ))),
    }
}

/// Loads a concrete scalar value as an integer runtime argument.
fn load_as_int(ctx: &mut FunctionContext<'_>, value: ValueId, name: &str) -> Result<()> {
    match ctx.load_value_to_result(value)?.codegen_repr() {
        PhpType::Int | PhpType::Bool => Ok(()),
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            Ok(())
        }
        PhpType::Float => {
            abi::emit_float_result_to_int_result(ctx.emitter);
            Ok(())
        }
        PhpType::Str => {
            abi::emit_call_label(ctx.emitter, "__rt_str_to_int");
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name, other
        ))),
    }
}
