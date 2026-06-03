//! Purpose:
//! Lowers small indexed-array builtins for the EIR backend.
//! Delegates aggregate iteration and key-existence checks to existing runtime helpers.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - Aggregate helpers only accept indexed arrays with non-float scalar slots
//!   because they read 8-byte integer payloads directly.
//! - Indexed key existence reads only the array header, so element payload type is irrelevant.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::Instruction;
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::super::{expect_operand, store_if_result};

/// Lowers `array_sum()` over supported indexed-array payloads.
pub(super) fn lower_array_sum(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_indexed_array_aggregate(ctx, inst, "array_sum", "__rt_array_sum")
}

/// Lowers `array_product()` over supported indexed-array payloads.
pub(super) fn lower_array_product(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_indexed_array_aggregate(ctx, inst, "array_product", "__rt_array_product")
}

/// Lowers `array_key_exists()` for indexed arrays with integer-like keys.
pub(super) fn lower_array_key_exists(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "array_key_exists", 2)?;
    let key = expect_operand(inst, 0)?;
    let array = expect_operand(inst, 1)?;
    require_indexed_array_key_exists_types(
        ctx.value_php_type(key)?,
        ctx.value_php_type(array)?,
        "array_key_exists",
    )?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
            ctx.load_value_to_reg(key, "x1")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
            ctx.load_value_to_reg(key, "rsi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_key_exists");
    store_if_result(ctx, inst)
}

/// Lowers `in_array()` for indexed arrays with scalar or string payloads.
pub(super) fn lower_in_array(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "in_array", 2)?;
    let needle = expect_operand(inst, 0)?;
    let array = expect_operand(inst, 1)?;
    match supported_in_array_case(ctx.value_php_type(needle)?, ctx.value_php_type(array)?)? {
        InArrayCase::Empty => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        InArrayCase::Scalar => lower_in_array_scalar(ctx, needle, array)?,
        InArrayCase::String => lower_in_array_string(ctx, needle, array)?,
    }
    store_if_result(ctx, inst)
}

/// Loads an indexed array argument and calls the selected runtime aggregate helper.
fn lower_indexed_array_aggregate(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    helper: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 1)?;
    let array = expect_operand(inst, 0)?;
    require_supported_indexed_array(ctx.value_php_type(array)?, name)?;
    ctx.load_value_to_result(array)?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the indexed-array pointer as the runtime helper argument
    }
    abi::emit_call_label(ctx.emitter, helper);
    store_if_result(ctx, inst)
}

/// Verifies the aggregate can use the current raw integer-slot runtime helper.
fn require_supported_indexed_array(ty: PhpType, name: &str) -> Result<()> {
    match ty.codegen_repr() {
        PhpType::Array(elem) if matches!(*elem, PhpType::Int | PhpType::Bool | PhpType::Never) => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name,
            other
        ))),
    }
}

/// Describes which indexed-array `in_array()` lowering path applies.
enum InArrayCase {
    Empty,
    Scalar,
    String,
}

/// Verifies that an indexed-array `in_array()` call has a lowered Phase 04 payload shape.
fn supported_in_array_case(needle_ty: PhpType, array_ty: PhpType) -> Result<InArrayCase> {
    let needle_ty = needle_ty.codegen_repr();
    match array_ty.codegen_repr() {
        PhpType::Array(elem) => match elem.codegen_repr() {
            PhpType::Never | PhpType::Void => Ok(InArrayCase::Empty),
            PhpType::Int | PhpType::Bool if matches!(needle_ty, PhpType::Int | PhpType::Bool) => {
                Ok(InArrayCase::Scalar)
            }
            PhpType::Str if needle_ty == PhpType::Str => Ok(InArrayCase::String),
            elem_ty => Err(CodegenIrError::unsupported(format!(
                "in_array needle PHP type {:?} for indexed-array element PHP type {:?}",
                needle_ty,
                elem_ty
            ))),
        },
        other => Err(CodegenIrError::unsupported(format!(
            "in_array for PHP array type {:?}",
            other
        ))),
    }
}

/// Lowers integer-like indexed-array membership via the existing search helper.
fn lower_in_array_scalar(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
            ctx.load_value_to_reg(needle, "x1")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_search");
            ctx.emitter.instruction("cmp x0, #0");                              // check whether indexed-array search returned a non-negative match index
            ctx.emitter.instruction("cset x0, ge");                             // materialize in_array() as true for any found index
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
            ctx.load_value_to_reg(needle, "rsi")?;
            abi::emit_call_label(ctx.emitter, "__rt_array_search");
            ctx.emitter.instruction("cmp rax, 0");                              // check whether indexed-array search returned a non-negative match index
            ctx.emitter.instruction("setge al");                                // materialize in_array() as true for any found index
            ctx.emitter.instruction("movzx rax, al");                           // widen the membership flag into the integer result register
        }
    }
    Ok(())
}

/// Lowers string indexed-array membership with a linear scan and `__rt_str_eq`.
fn lower_in_array_string(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_in_array_string_aarch64(ctx, needle, array),
        Arch::X86_64 => lower_in_array_string_x86_64(ctx, needle, array),
    }
}

/// Emits the AArch64 string-array membership loop.
fn lower_in_array_string_aarch64(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_str_loop");
    let found_label = ctx.next_label("in_array_str_found");
    let end_label = ctx.next_label("in_array_str_end");
    let done_label = ctx.next_label("in_array_str_done");

    ctx.load_value_to_reg(array, "x10")?;
    ctx.emitter.instruction("ldr x9, [x10]");                                   // load indexed string-array length before scanning payload slots
    ctx.emitter.instruction("add x10, x10, #24");                               // point at the first indexed string-array payload slot
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
    abi::emit_call_label(ctx.emitter, "__rt_str_eq");
    abi::emit_pop_reg(ctx.emitter, "x12");
    abi::emit_pop_reg_pair(ctx.emitter, "x9", "x10");
    ctx.emitter.instruction(&format!("cbnz x0, {}", found_label));              // stop as soon as the searched string matches an element
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

/// Emits the x86_64 string-array membership loop.
fn lower_in_array_string_x86_64(
    ctx: &mut FunctionContext<'_>,
    needle: crate::ir::ValueId,
    array: crate::ir::ValueId,
) -> Result<()> {
    let loop_label = ctx.next_label("in_array_str_loop");
    let found_label = ctx.next_label("in_array_str_found");
    let end_label = ctx.next_label("in_array_str_end");
    let done_label = ctx.next_label("in_array_str_done");

    ctx.load_value_to_reg(array, "r10")?;
    ctx.emitter.instruction("mov r11, QWORD PTR [r10]");                        // load indexed string-array length before scanning payload slots
    ctx.emitter.instruction("lea r12, [r10 + 24]");                             // point at the first indexed string-array payload slot
    ctx.emitter.instruction("xor r13d, r13d");                                  // start the string membership scan at index zero
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp r13, r11");                                    // compare the scan index against indexed-array length
    ctx.emitter.instruction(&format!("jge {}", end_label));                     // finish with false after all string elements are scanned
    ctx.emitter.instruction("mov rcx, r13");                                    // copy the scan index before scaling it to a byte offset
    ctx.emitter.instruction("shl rcx, 4");                                      // scale the element index by the 16-byte string slot width
    ctx.emitter.instruction("mov rdi, QWORD PTR [r12 + rcx]");                  // load the current string element pointer for comparison
    ctx.emitter.instruction("mov rsi, QWORD PTR [r12 + rcx + 8]");              // load the current string element length for comparison
    abi::emit_push_reg_pair(ctx.emitter, "r11", "r12");
    abi::emit_push_reg(ctx.emitter, "r13");
    ctx.load_string_value_to_regs(needle, "rdx", "rcx")?;
    abi::emit_call_label(ctx.emitter, "__rt_str_eq");
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

/// Verifies indexed-array key existence can use the integer-key runtime helper.
fn require_indexed_array_key_exists_types(
    key_ty: PhpType,
    array_ty: PhpType,
    name: &str,
) -> Result<()> {
    match array_ty.codegen_repr() {
        PhpType::Array(_) => {}
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "{} for PHP array type {:?}",
                name,
                other
            )));
        }
    }
    match key_ty.codegen_repr() {
        PhpType::Int | PhpType::Bool => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} key PHP type {:?}",
            name,
            other
        ))),
    }
}
