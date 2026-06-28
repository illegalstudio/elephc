//! Purpose:
//! Lowers predicate EIR opcodes such as null checks, PHP truthiness, and Mixed tag tests.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - PHP string truthiness is special: only `""` and `"0"` are false.
//! - Mixed predicates unbox the runtime cell before comparing the concrete tag.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};
use crate::codegen_ir::{CodegenIrError, Result};

/// Lowers scalar PHP truthiness into a concrete boolean integer result.
pub(super) fn lower_is_truthy(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    match ctx.raw_value_php_type(value)? {
        PhpType::Bool | PhpType::Int | PhpType::Pointer(_) => {
            ctx.load_value_to_result(value)?;
            emit_int_result_nonzero_bool(ctx);
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            emit_float_result_nonzero_bool(ctx);
        }
        PhpType::Str => emit_string_truthiness(ctx, value)?,
        PhpType::TaggedScalar => emit_tagged_scalar_truthiness(ctx, value)?,
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => {
            emit_array_truthiness(ctx, value)?;
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool");
        }
        PhpType::Resource(_) => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "{} for PHP type {:?}",
                inst.op.name(),
                other
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// Emits PHP truthiness for a tagged scalar: null is false, otherwise integer truthiness.
fn emit_tagged_scalar_truthiness(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let null_label = ctx.next_label("tagged_truthy_null");
    let done_label = ctx.next_label("tagged_truthy_done");
    ctx.load_value_to_result(value)?;
    crate::codegen::sentinels::emit_branch_if_tagged_scalar_null(ctx.emitter, &null_label);
    emit_int_result_nonzero_bool(ctx);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&null_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits PHP array truthiness by checking the runtime container length header.
pub(super) fn emit_array_truthiness(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    ctx.load_value_to_result(value)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_from_address(ctx.emitter, result_reg, result_reg, 0);
    emit_int_result_nonzero_bool(ctx);
    Ok(())
}

/// Lowers static and boxed Mixed null checks into a concrete boolean integer result.
pub(super) fn lower_is_null(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    emit_is_null_result(ctx, value)?;
    store_if_result(ctx, inst)
}

/// Emits an `is_null` boolean result for static nulls and boxed Mixed payloads.
pub(super) fn emit_is_null_result(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    match ctx.value_php_type(value)? {
        PhpType::Mixed | PhpType::Union(_) => emit_mixed_tag_eq(ctx, value, 8),
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
            Ok(())
        }
        PhpType::TaggedScalar => {
            ctx.load_value_to_result(value)?;
            emit_tagged_scalar_null_bool(ctx);
            Ok(())
        }
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            if crate::codegen::sentinels::null_repr_is_tagged() {
                abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
                return Ok(());
            }
            ctx.load_value_to_result(value)?;
            emit_int_result_null_sentinel_bool(ctx);
            Ok(())
        }
        _ => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            Ok(())
        }
    }
}

/// Compares the loaded tagged-scalar tag register against PHP's null tag.
fn emit_tagged_scalar_null_bool(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let cmp_inst = format!(
                "cmp x1, #{}",
                crate::codegen::sentinels::TAGGED_SCALAR_TAG_NULL
            );
            ctx.emitter.instruction(&cmp_inst);                                 // compare the tagged scalar tag against PHP null
            ctx.emitter.instruction("cset x0, eq");                             // materialize true when the tagged scalar is null
        }
        Arch::X86_64 => {
            let cmp_inst = format!(
                "cmp rdx, {}",
                crate::codegen::sentinels::TAGGED_SCALAR_TAG_NULL
            );
            ctx.emitter.instruction(&cmp_inst);                                 // compare the tagged scalar tag against PHP null
            ctx.emitter.instruction("sete al");                                 // materialize true when the tagged scalar is null
            ctx.emitter.instruction("movzx rax, al");                           // widen the boolean byte into the integer result register
        }
    }
}

/// Compares the loaded integer result against elephc's scalar null sentinel.
fn emit_int_result_null_sentinel_bool(ctx: &mut FunctionContext<'_>) {
    let sentinel_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, sentinel_reg, 0x7fff_ffff_ffff_fffe);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp x0, {}", sentinel_reg));      // compare the scalar value with the runtime null sentinel
            ctx.emitter.instruction("cset x0, eq");                             // materialize true when the scalar carries the null sentinel
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp rax, {}", sentinel_reg));     // compare the scalar value with the runtime null sentinel
            ctx.emitter.instruction("sete al");                                 // materialize true when the scalar carries the null sentinel
            ctx.emitter.instruction("movzx rax, al");                           // widen the boolean byte into the integer result register
        }
    }
}

/// Emits a boolean result for whether a boxed Mixed value has the given runtime tag.
pub(super) fn emit_mixed_tag_eq(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    tag: u8,
) -> Result<()> {
    let ty = ctx.load_value_to_result(value)?;
    if !matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        return Err(CodegenIrError::unsupported(format!(
            "mixed tag predicate for PHP type {:?}",
            ty
        )));
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp x0, #{}", tag));              // compare the unboxed Mixed runtime tag against the expected tag
            ctx.emitter.instruction("cset x0, eq");                             // materialize the Mixed tag predicate as boolean 1 on match
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp rax, {}", tag));              // compare the unboxed Mixed runtime tag against the expected tag
            ctx.emitter.instruction("sete al");                                 // materialize the Mixed tag predicate in the low byte
            ctx.emitter.instruction("movzx rax, al");                           // widen the Mixed tag predicate byte into the integer result register
        }
    }
    Ok(())
}

/// Emits a boolean result that is 1 when a boxed Mixed value's unboxed runtime tag is one
/// of `true_tags`, else 0. Backs the `is_array`/`is_object`/`is_scalar` kind predicates over
/// boxed Mixed/Union cells, where the concrete kind is only known at runtime.
pub(super) fn emit_mixed_tag_membership(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    true_tags: &[u8],
) -> Result<()> {
    let ty = ctx.load_value_to_result(value)?;
    if !matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        return Err(CodegenIrError::unsupported(format!(
            "mixed kind predicate for PHP type {:?}",
            ty
        )));
    }
    let true_label = ctx.next_label("mixed_kind_true");
    let done_label = ctx.next_label("mixed_kind_done");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    for &tag in true_tags {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cmp x0, #{}", tag));          // compare the unboxed Mixed runtime tag against an accepted kind
                ctx.emitter.instruction(&format!("b.eq {}", true_label));       // accept the value when its runtime tag matches this kind
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("cmp rax, {}", tag));          // compare the unboxed Mixed runtime tag against an accepted kind
                ctx.emitter.instruction(&format!("je {}", true_label));         // accept the value when its runtime tag matches this kind
            }
        }
    }
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&true_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits an integer nonzero check into the canonical integer result register.
pub(super) fn emit_int_result_nonzero_bool(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // compare the integer truthiness operand against zero
            ctx.emitter.instruction("cset x0, ne");                             // materialize nonzero integer truthiness as boolean 1
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // compare the integer truthiness operand against zero
            ctx.emitter.instruction("setne al");                                // materialize nonzero integer truthiness in the low byte
            ctx.emitter.instruction("movzx rax, al");                           // widen the truthiness byte into the integer result register
        }
    }
}

/// Emits a float nonzero check into the canonical integer result register.
pub(super) fn emit_float_result_nonzero_bool(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fcmp d0, #0.0");                           // compare the float truthiness operand against zero
            ctx.emitter.instruction("cset x0, ne");                             // materialize nonzero float truthiness as boolean 1
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("xorpd xmm1, xmm1");                        // materialize a zero float register for comparison
            ctx.emitter.instruction("ucomisd xmm0, xmm1");                      // compare the float truthiness operand against zero
            ctx.emitter.instruction("setne al");                                // materialize nonzero float truthiness in the low byte
            ctx.emitter.instruction("movzx rax, al");                           // widen the truthiness byte into the integer result register
        }
    }
}

/// Emits PHP string truthiness, where `""` and `"0"` are false.
pub(super) fn emit_string_truthiness(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let true_label = ctx.next_label("str_truthy_true");
    let done_label = ctx.next_label("str_truthy_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_string_value_to_regs(value, "x1", "x2")?;
            ctx.emitter.instruction("mov x0, #0");                              // default string truthiness to false
            ctx.emitter.instruction(&format!("cbz x2, {}", done_label));        // empty strings are false
            ctx.emitter.instruction("cmp x2, #1");                              // detect the one-byte special case for string "0"
            ctx.emitter.instruction(&format!("b.ne {}", true_label));           // multi-byte non-empty strings are truthy
            ctx.emitter.instruction("ldrb w10, [x1]");                          // load the only byte for the PHP string "0" check
            ctx.emitter.instruction("cmp w10, #48");                            // compare the byte against ASCII '0'
            ctx.emitter.instruction("cset x0, ne");                             // one-byte strings are truthy unless that byte is '0'
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the generic truthy case
            ctx.emitter.label(&true_label);
            ctx.emitter.instruction("mov x0, #1");                              // mark non-empty non-"0" strings as truthy
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.load_string_value_to_regs(value, "r10", "r11")?;
            ctx.emitter.instruction("mov rax, 0");                              // default string truthiness to false
            ctx.emitter.instruction("test r11, r11");                           // test whether the string length is zero
            ctx.emitter.instruction(&format!("je {}", done_label));             // empty strings are false
            ctx.emitter.instruction("cmp r11, 1");                              // detect the one-byte special case for string "0"
            ctx.emitter.instruction(&format!("jne {}", true_label));            // multi-byte non-empty strings are truthy
            ctx.emitter.instruction("movzx ecx, BYTE PTR [r10]");               // load the only byte for the PHP string "0" check
            ctx.emitter.instruction("cmp ecx, 48");                             // compare the byte against ASCII '0'
            ctx.emitter.instruction("setne al");                                // one-byte strings are truthy unless that byte is '0'
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the generic truthy case
            ctx.emitter.label(&true_label);
            ctx.emitter.instruction("mov rax, 1");                              // mark non-empty non-"0" strings as truthy
            ctx.emitter.label(&done_label);
        }
    }
    Ok(())
}
