//! Purpose:
//! Lowers string constants, scalar-to-string conversions, and string
//! concatenation EIR opcodes for the Phase 04 backend.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - PHP string coercion treats `false` and `null` as empty strings, while
//!   integer true and ordinary ints use the existing `__rt_itoa` helper.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::ir::Instruction;
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{
    expect_data, expect_operand, require_float, require_integer_like, require_string,
    store_if_result,
};
use crate::codegen_ir::{CodegenIrError, Result};

const CALLED_CLASS_ID_PARAM: &str = "__elephc_called_class_id";

/// Lowers a string constant by materializing its data-section pointer and byte length.
pub(super) fn lower_const_str(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let data_id = expect_data(inst)?;
    let (label, len) = ctx.intern_string_data(data_id)?;
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
    store_if_result(ctx, inst)
}

/// Lowers a `::class` constant by materializing the interned class-name string.
pub(super) fn lower_const_class_name(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let data_id = expect_data(inst)?;
    let class_name = ctx
        .module
        .data
        .class_names
        .get(data_id.as_raw() as usize)
        .ok_or_else(|| CodegenIrError::missing_entry("class data", data_id.as_raw()))?
        .clone();
    if class_name == "static" {
        lower_late_static_class_name(ctx)?;
        return store_if_result(ctx, inst);
    }
    let (label, len) = ctx.intern_class_name_data(data_id)?;
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
    store_if_result(ctx, inst)
}

/// Lowers `static::class` by resolving the forwarded called-class id at runtime.
fn lower_late_static_class_name(ctx: &mut FunctionContext<'_>) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_late_static_class_name_arm64(ctx),
        Arch::X86_64 => lower_late_static_class_name_x86_64(ctx),
    }
}

/// Emits the AArch64 class-id table lookup for `static::class`.
fn lower_late_static_class_name_arm64(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let missing = ctx.next_label("static_class_missing");
    let done = ctx.next_label("static_class_done");
    emit_late_static_class_id_to_reg(ctx, "x12")?;
    abi::emit_load_symbol_to_reg(ctx.emitter, "x10", "_class_name_count", 0);
    ctx.emitter.instruction("cmp x12, x10");                                    // reject called-class ids outside the emitted class-name table
    ctx.emitter.instruction(&format!("b.hs {}", missing));                      // use an empty class name when metadata is unavailable
    abi::emit_symbol_address(ctx.emitter, "x11", "_class_name_entries");
    ctx.emitter.instruction("lsl x12, x12, #4");                                // convert class id to a 16-byte class-name table offset
    ctx.emitter.instruction("add x11, x11, x12");                               // select the class-name table row for the called class
    ctx.emitter.instruction("ldr x1, [x11]");                                   // load the called class name pointer into the string result
    ctx.emitter.instruction("ldr x2, [x11, #8]");                               // load the called class name length into the string result
    ctx.emitter.instruction(&format!("b {}", done));                            // skip the missing-metadata fallback
    ctx.emitter.label(&missing);
    abi::emit_symbol_address(ctx.emitter, "x1", "_class_name_missing");
    ctx.emitter.instruction("mov x2, #0");                                      // missing metadata stringifies to an empty class name
    ctx.emitter.label(&done);
    Ok(())
}

/// Emits the x86_64 class-id table lookup for `static::class`.
fn lower_late_static_class_name_x86_64(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let missing = ctx.next_label("static_class_missing");
    let done = ctx.next_label("static_class_done");
    emit_late_static_class_id_to_reg(ctx, "r8")?;
    abi::emit_load_symbol_to_reg(ctx.emitter, "r9", "_class_name_count", 0);
    ctx.emitter.instruction("cmp r8, r9");                                      // reject called-class ids outside the emitted class-name table
    ctx.emitter.instruction(&format!("jae {}", missing));                       // use an empty class name when metadata is unavailable
    abi::emit_symbol_address(ctx.emitter, "r10", "_class_name_entries");
    ctx.emitter.instruction("shl r8, 4");                                       // convert class id to a 16-byte class-name table offset
    ctx.emitter.instruction("add r10, r8");                                     // select the class-name table row for the called class
    ctx.emitter.instruction("mov rax, QWORD PTR [r10]");                        // load the called class name pointer into the string result
    ctx.emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                    // load the called class name length into the string result
    ctx.emitter.instruction(&format!("jmp {}", done));                          // skip the missing-metadata fallback
    ctx.emitter.label(&missing);
    abi::emit_symbol_address(ctx.emitter, "rax", "_class_name_missing");
    ctx.emitter.instruction("mov rdx, 0");                                      // missing metadata stringifies to an empty class name
    ctx.emitter.label(&done);
    Ok(())
}

/// Loads the late-static class id from the hidden static frame slot or `$this`.
fn emit_late_static_class_id_to_reg(ctx: &mut FunctionContext<'_>, reg: &str) -> Result<()> {
    if let Some(slot) = ctx.local_slot_by_name(CALLED_CLASS_ID_PARAM) {
        let offset = ctx.local_offset(slot)?;
        abi::load_at_offset(ctx.emitter, reg, offset);
        return Ok(());
    }
    if let Some(slot) = ctx.local_slot_by_name("this") {
        let offset = ctx.local_offset(slot)?;
        abi::load_at_offset(ctx.emitter, reg, offset);
        abi::emit_load_from_address(ctx.emitter, reg, reg, 0);
        return Ok(());
    }
    Err(CodegenIrError::unsupported(
        "static::class without a forwarded called-class id",
    ))
}

/// Lowers a string concatenation by loading both string pairs into `__rt_concat`'s ABI.
pub(super) fn lower_str_concat(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    require_string(ctx.value_php_type(lhs)?, inst)?;
    require_string(ctx.value_php_type(rhs)?, inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_string_value_to_regs(lhs, "x1", "x2")?;
            ctx.load_string_value_to_regs(rhs, "x3", "x4")?;
        }
        Arch::X86_64 => {
            ctx.load_string_value_to_regs(lhs, "rax", "rdx")?;
            ctx.load_string_value_to_regs(rhs, "rdi", "rsi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_concat");
    store_if_result(ctx, inst)
}

/// Lowers a string length opcode by returning the string-pair length word.
pub(super) fn lower_str_len(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    require_string(ctx.load_value_to_result(value)?, inst)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let len_reg = abi::string_result_regs(ctx.emitter).1;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov {}, {}", result_reg, len_reg)); // return the byte length of the loaded PHP string
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov {}, {}", result_reg, len_reg)); // return the byte length of the loaded PHP string
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers string indexing to a one-byte string or an empty string when out of bounds.
pub(super) fn lower_str_char_at(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let string = expect_operand(inst, 0)?;
    let index = expect_operand(inst, 1)?;
    require_string(ctx.value_php_type(string)?, inst)?;
    let non_negative = ctx.next_label("str_idx_pos");
    let oob = ctx.next_label("str_idx_oob");
    let end = ctx.next_label("str_idx_end");

    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_string_value_to_regs(string, "x1", "x2")?;
            require_integer_like(ctx.load_value_to_reg(index, "x0")?, inst)?;
            ctx.emitter.instruction("cmp x0, #0");                              // check whether the requested string offset is negative
            ctx.emitter.instruction(&format!("b.ge {}", non_negative));         // keep non-negative string offsets unchanged
            ctx.emitter.instruction("add x0, x2, x0");                          // convert negative string offsets to length plus offset
            ctx.emitter.instruction("cmp x0, #0");                              // check whether the adjusted offset still precedes the string
            ctx.emitter.instruction(&format!("b.lt {}", oob));                  // out-of-range negative offsets return an empty string
            ctx.emitter.label(&non_negative);
            ctx.emitter.instruction("cmp x0, x2");                              // compare the requested offset against the string length
            ctx.emitter.instruction(&format!("b.ge {}", oob));                  // offsets at or beyond length return an empty string
            ctx.emitter.instruction("add x1, x1, x0");                          // point the string result at the selected byte
            ctx.emitter.instruction("mov x2, #1");                              // in-bounds string indexing returns one byte
            ctx.emitter.instruction(&format!("b {}", end));                     // skip the out-of-bounds empty-string result
            ctx.emitter.label(&oob);
            ctx.emitter.instruction("mov x2, #0");                              // out-of-bounds string indexing returns an empty string
            ctx.emitter.label(&end);
        }
        Arch::X86_64 => {
            ctx.load_string_value_to_regs(string, "r8", "r9")?;
            require_integer_like(ctx.load_value_to_reg(index, "rax")?, inst)?;
            ctx.emitter.instruction("cmp rax, 0");                              // check whether the requested string offset is negative
            ctx.emitter.instruction(&format!("jge {}", non_negative));          // keep non-negative string offsets unchanged
            ctx.emitter.instruction("add rax, r9");                             // convert negative string offsets to length plus offset
            ctx.emitter.instruction("cmp rax, 0");                              // check whether the adjusted offset still precedes the string
            ctx.emitter.instruction(&format!("jl {}", oob));                    // out-of-range negative offsets return an empty string
            ctx.emitter.label(&non_negative);
            ctx.emitter.instruction("cmp rax, r9");                             // compare the requested offset against the string length
            ctx.emitter.instruction(&format!("jge {}", oob));                   // offsets at or beyond length return an empty string
            ctx.emitter.instruction("add r8, rax");                             // point the string result at the selected byte
            ctx.emitter.instruction("mov rax, r8");                             // publish the selected byte pointer as the string result pointer
            ctx.emitter.instruction("mov rdx, 1");                              // in-bounds string indexing returns one byte
            ctx.emitter.instruction(&format!("jmp {}", end));                   // skip the out-of-bounds empty-string result
            ctx.emitter.label(&oob);
            ctx.emitter.instruction("mov rax, r8");                             // preserve a valid source pointer for the empty string result
            ctx.emitter.instruction("mov rdx, 0");                              // out-of-bounds string indexing returns an empty string
            ctx.emitter.label(&end);
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers string persistence by copying the string into runtime-owned storage.
pub(super) fn lower_str_persist(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    require_string(ctx.load_value_to_result(value)?, inst)?;
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    store_if_result(ctx, inst)
}

/// Lowers a float-to-string conversion through the existing runtime formatter.
pub(super) fn lower_float_to_string(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    require_float(ctx.load_value_to_result(value)?, inst)?;
    abi::emit_call_label(ctx.emitter, "__rt_ftoa");
    store_if_result(ctx, inst)
}

/// Lowers an integer-like-to-string conversion, including PHP bool/null string rules.
pub(super) fn lower_int_like_to_string(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    match ctx.load_value_to_result(value)? {
        PhpType::Bool => {
            lower_loaded_bool_to_string(ctx)?;
            store_if_result(ctx, inst)
        }
        PhpType::Int => {
            abi::emit_call_label(ctx.emitter, "__rt_itoa");
            store_if_result(ctx, inst)
        }
        PhpType::Void | PhpType::Never => {
            let len_reg = abi::string_result_regs(ctx.emitter).1;
            abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
            store_if_result(ctx, inst)
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            inst.op.name(),
            other
        ))),
    }
}

/// Converts the loaded boolean result to PHP string ABI registers.
fn lower_loaded_bool_to_string(ctx: &mut FunctionContext<'_>) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let false_label = ctx.next_label("bool_to_str_false");
            let done_label = ctx.next_label("bool_to_str_done");
            ctx.emitter.instruction(&format!("cbz x0, {}", false_label));       // false stringifies to an empty string
            abi::emit_call_label(ctx.emitter, "__rt_itoa");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the empty-string fallback after true conversion
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x2, #0");                              // false has zero string length
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            let false_label = ctx.next_label("bool_to_str_false");
            let done_label = ctx.next_label("bool_to_str_done");
            ctx.emitter.instruction("test rax, rax");                           // test whether the boolean payload is false
            ctx.emitter.instruction(&format!("je {}", false_label));            // false stringifies to an empty string
            abi::emit_call_label(ctx.emitter, "__rt_itoa");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the empty-string fallback after true conversion
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov rdx, 0");                              // false has zero string length
            ctx.emitter.label(&done_label);
        }
    }
    Ok(())
}
