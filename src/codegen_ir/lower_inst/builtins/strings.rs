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
