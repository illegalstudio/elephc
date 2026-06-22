//! Purpose:
//! Lowers PHP `isset()` in the EIR backend, including null checks, offset
//! existence probes, and scalar null-sentinel handling.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - Offset probes must inspect the source `StrCharAt`, `ArrayGet`, or `HashGet`
//!   producer because the ordinary read fallback erases missing-key information.

use crate::codegen::{abi, emit_box_current_value_as_mixed};
use crate::codegen::platform::Arch;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::{Instruction, Op, ValueDef, ValueId};
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::super::{expect_operand, predicates, store_if_result};

const RUNTIME_NULL_SENTINEL: i64 = 0x7fff_ffff_ffff_fffe;

/// Lowers `isset()` for values already evaluated by the EIR frontend.
pub(super) fn lower_isset(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_min_arg_count(inst, "isset", 1)?;
    let false_label = ctx.next_label("isset_false");
    let done_label = ctx.next_label("isset_done");
    for value in inst.operands.iter().copied() {
        emit_isset_missing_result(ctx, value)?;
        abi::emit_branch_if_int_result_nonzero(ctx.emitter, &false_label);
    }
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&false_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    ctx.emitter.label(&done_label);
    if inst.result_php_type.codegen_repr() == PhpType::Mixed {
        emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    }
    store_if_result(ctx, inst)
}

/// Emits 1 when an already-evaluated `isset` operand is null or missing.
fn emit_isset_missing_result(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    if let Some(inst) = source_instruction(ctx, value)? {
        match inst.op {
            Op::StrCharAt => return emit_isset_string_offset_missing_result(ctx, &inst),
            Op::ArrayGet => return emit_isset_array_offset_missing_result(ctx, &inst),
            Op::HashGet => return emit_isset_hash_offset_missing_result(ctx, &inst),
            _ => {}
        }
    }
    emit_loaded_isset_missing_result(ctx, value)
}

/// Emits 1 when a loaded scalar, boxed Mixed, or null-like value is absent for `isset`.
fn emit_loaded_isset_missing_result(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    match ctx.value_php_type(value)? {
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
            Ok(())
        }
        PhpType::Mixed | PhpType::Union(_) => predicates::emit_mixed_tag_eq(ctx, value, 8),
        PhpType::TaggedScalar => emit_tagged_scalar_value_is_null(ctx, value),
        PhpType::Int | PhpType::Bool if crate::codegen::sentinels::null_repr_is_tagged() => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            Ok(())
        }
        PhpType::Int | PhpType::Bool => emit_scalar_value_is_null_sentinel(ctx, value),
        _ => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            Ok(())
        }
    }
}

/// Emits 1 when a string-offset read came from an out-of-bounds offset.
fn emit_isset_string_offset_missing_result(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let string = expect_operand(inst, 0)?;
    let index = expect_operand(inst, 1)?;
    require_isset_integer_like_index(ctx, index, "string offset")?;
    let non_negative = ctx.next_label("isset_str_idx_pos");
    let missing = ctx.next_label("isset_str_idx_missing");
    let done = ctx.next_label("isset_str_idx_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_string_value_to_regs(string, "x1", "x2")?;
            ctx.load_value_to_reg(index, "x0")?;
            ctx.emitter.instruction("cmp x0, #0");                              // check whether the string offset is negative
            ctx.emitter.instruction(&format!("b.ge {}", non_negative));         // keep non-negative string offsets unchanged
            ctx.emitter.instruction("add x0, x2, x0");                          // convert negative offsets to length plus offset
            ctx.emitter.instruction("cmp x0, #0");                              // check whether the adjusted offset is still before the string
            ctx.emitter.instruction(&format!("b.lt {}", missing));              // negative offsets before the string are missing
            ctx.emitter.label(&non_negative);
            ctx.emitter.instruction("cmp x0, x2");                              // compare the string offset against the string length
            ctx.emitter.instruction(&format!("b.ge {}", missing));              // offsets at or beyond length are missing
            ctx.emitter.instruction("mov x0, #0");                              // in-bounds string offsets are present for isset
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the missing string-offset result
            ctx.emitter.label(&missing);
            ctx.emitter.instruction("mov x0, #1");                              // out-of-bounds string offsets are missing for isset
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            ctx.load_string_value_to_regs(string, "r8", "r9")?;
            ctx.load_value_to_reg(index, "rax")?;
            ctx.emitter.instruction("cmp rax, 0");                              // check whether the string offset is negative
            ctx.emitter.instruction(&format!("jge {}", non_negative));          // keep non-negative string offsets unchanged
            ctx.emitter.instruction("add rax, r9");                             // convert negative offsets to length plus offset
            ctx.emitter.instruction("cmp rax, 0");                              // check whether the adjusted offset is still before the string
            ctx.emitter.instruction(&format!("jl {}", missing));                // negative offsets before the string are missing
            ctx.emitter.label(&non_negative);
            ctx.emitter.instruction("cmp rax, r9");                             // compare the string offset against the string length
            ctx.emitter.instruction(&format!("jge {}", missing));               // offsets at or beyond length are missing
            ctx.emitter.instruction("xor eax, eax");                            // in-bounds string offsets are present for isset
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the missing string-offset result
            ctx.emitter.label(&missing);
            ctx.emitter.instruction("mov rax, 1");                              // out-of-bounds string offsets are missing for isset
            ctx.emitter.label(&done);
        }
    }
    Ok(())
}

/// Emits 1 when an indexed-array read came from a missing offset or null element.
fn emit_isset_array_offset_missing_result(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    let index = expect_operand(inst, 1)?;
    require_isset_integer_like_index(ctx, index, "array offset")?;
    let elem_ty = match ctx.value_php_type(array)? {
        PhpType::Array(elem_ty) => elem_ty.codegen_repr(),
        _ => return emit_loaded_isset_missing_result(ctx, expect_operand(inst, 0)?),
    };
    let missing = ctx.next_label("isset_array_idx_missing");
    let done = ctx.next_label("isset_array_idx_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_isset_array_offset_missing_aarch64(
            ctx,
            array,
            index,
            &elem_ty,
            &missing,
            &done,
        )?,
        Arch::X86_64 => emit_isset_array_offset_missing_x86_64(
            ctx,
            array,
            index,
            &elem_ty,
            &missing,
            &done,
        )?,
    }
    Ok(())
}

/// Emits 1 when an associative-array lookup missed or found a null value.
fn emit_isset_hash_offset_missing_result(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let hash = expect_operand(inst, 0)?;
    let key = expect_operand(inst, 1)?;
    let value_ty = match ctx.value_php_type(hash)? {
        PhpType::AssocArray { value, .. } => value.codegen_repr(),
        _ => return emit_loaded_isset_missing_result(ctx, expect_operand(inst, 0)?),
    };
    let missing = ctx.next_label("isset_hash_missing");
    let done = ctx.next_label("isset_hash_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_isset_hash_offset_missing_aarch64(
            ctx,
            hash,
            key,
            &value_ty,
            &missing,
            &done,
        )?,
        Arch::X86_64 => emit_isset_hash_offset_missing_x86_64(
            ctx,
            hash,
            key,
            &value_ty,
            &missing,
            &done,
        )?,
    }
    Ok(())
}

/// Emits AArch64 associative-array `isset` lookup and null checks.
fn emit_isset_hash_offset_missing_aarch64(
    ctx: &mut FunctionContext<'_>,
    hash: ValueId,
    key: ValueId,
    value_ty: &PhpType,
    missing: &str,
    done: &str,
) -> Result<()> {
    super::super::hashes::materialize_hash_key_aarch64(ctx, key)?;
    ctx.load_value_to_reg(hash, "x0")?;
    abi::emit_call_label(ctx.emitter, "__rt_hash_get");
    ctx.emitter.instruction(&format!("cbz x0, {}", missing));                   // missing hash keys make isset return false
    emit_isset_hash_found_null_check_aarch64(ctx, value_ty, missing)?;
    ctx.emitter.instruction("mov x0, #0");                                      // found non-null hash entries are present for isset
    ctx.emitter.instruction(&format!("b {}", done));                            // skip the missing hash-entry result
    ctx.emitter.label(missing);
    ctx.emitter.instruction("mov x0, #1");                                      // missing or null hash entries are absent for isset
    ctx.emitter.label(done);
    Ok(())
}

/// Emits x86_64 associative-array `isset` lookup and null checks.
fn emit_isset_hash_offset_missing_x86_64(
    ctx: &mut FunctionContext<'_>,
    hash: ValueId,
    key: ValueId,
    value_ty: &PhpType,
    missing: &str,
    done: &str,
) -> Result<()> {
    super::super::hashes::materialize_hash_key_x86_64(ctx, key)?;
    ctx.load_value_to_reg(hash, "rdi")?;
    abi::emit_call_label(ctx.emitter, "__rt_hash_get");
    ctx.emitter.instruction("test rax, rax");                                   // check whether the associative lookup found a matching key
    ctx.emitter.instruction(&format!("jz {}", missing));                        // missing hash keys make isset return false
    emit_isset_hash_found_null_check_x86_64(ctx, value_ty, missing)?;
    ctx.emitter.instruction("xor eax, eax");                                    // found non-null hash entries are present for isset
    ctx.emitter.instruction(&format!("jmp {}", done));                          // skip the missing hash-entry result
    ctx.emitter.label(missing);
    ctx.emitter.instruction("mov rax, 1");                                      // missing or null hash entries are absent for isset
    ctx.emitter.label(done);
    Ok(())
}

/// Branches on AArch64 when a found hash entry represents PHP null.
fn emit_isset_hash_found_null_check_aarch64(
    ctx: &mut FunctionContext<'_>,
    value_ty: &PhpType,
    missing: &str,
) -> Result<()> {
    if matches!(value_ty.codegen_repr(), PhpType::Mixed) {
        ctx.emitter.instruction("mov x0, x1");                                  // pass the boxed Mixed hash value to the unbox helper
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        ctx.emitter.instruction("cmp x0, #8");                                  // runtime tag 8 means the found hash value is PHP null
        ctx.emitter.instruction(&format!("b.eq {}", missing));                  // null hash values make isset return false
        return Ok(());
    }
    ctx.emitter.instruction("cmp x3, #8");                                      // runtime tag 8 means the found hash value is PHP null
    ctx.emitter.instruction(&format!("b.eq {}", missing));                      // null hash values make isset return false
    Ok(())
}

/// Branches on x86_64 when a found hash entry represents PHP null.
fn emit_isset_hash_found_null_check_x86_64(
    ctx: &mut FunctionContext<'_>,
    value_ty: &PhpType,
    missing: &str,
) -> Result<()> {
    if matches!(value_ty.codegen_repr(), PhpType::Mixed) {
        ctx.emitter.instruction("mov rax, rdi");                                // pass the boxed Mixed hash value to the unbox helper
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        ctx.emitter.instruction("cmp rax, 8");                                  // runtime tag 8 means the found hash value is PHP null
        ctx.emitter.instruction(&format!("je {}", missing));                    // null hash values make isset return false
        return Ok(());
    }
    ctx.emitter.instruction("cmp rcx, 8");                                      // runtime tag 8 means the found hash value is PHP null
    ctx.emitter.instruction(&format!("je {}", missing));                        // null hash values make isset return false
    Ok(())
}

/// Emits AArch64 indexed-array `isset` offset bounds and null checks.
fn emit_isset_array_offset_missing_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    index: ValueId,
    elem_ty: &PhpType,
    missing: &str,
    done: &str,
) -> Result<()> {
    let array_reg = abi::symbol_scratch_reg(ctx.emitter);
    let len_reg = abi::secondary_scratch_reg(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(index, result_reg)?;
    ctx.load_value_to_reg(array, array_reg)?;
    ctx.emitter.instruction(&format!("cmp {}, #0", result_reg));                // reject negative indexes as missing array elements
    ctx.emitter.instruction(&format!("b.lt {}", missing));                      // missing indexes make isset return false
    abi::emit_load_from_address(ctx.emitter, len_reg, array_reg, 0);
    ctx.emitter.instruction(&format!("cmp {}, {}", result_reg, len_reg));       // compare the requested index against the indexed-array length
    ctx.emitter.instruction(&format!("b.ge {}", missing));                      // out-of-bounds indexes make isset return false
    emit_isset_array_in_bounds_missing_aarch64(ctx, array_reg, result_reg, elem_ty)?;
    ctx.emitter.instruction(&format!("b {}", done));                            // skip the out-of-bounds isset result after an in-bounds probe
    ctx.emitter.label(missing);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 1);
    ctx.emitter.label(done);
    Ok(())
}

/// Emits x86_64 indexed-array `isset` offset bounds and null checks.
fn emit_isset_array_offset_missing_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    index: ValueId,
    elem_ty: &PhpType,
    missing: &str,
    done: &str,
) -> Result<()> {
    let array_reg = abi::symbol_scratch_reg(ctx.emitter);
    let len_reg = abi::secondary_scratch_reg(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(array, array_reg)?;
    ctx.load_value_to_reg(index, result_reg)?;
    ctx.emitter.instruction(&format!("cmp {}, 0", result_reg));                 // reject negative indexes as missing array elements
    ctx.emitter.instruction(&format!("jl {}", missing));                        // missing indexes make isset return false
    abi::emit_load_from_address(ctx.emitter, len_reg, array_reg, 0);
    ctx.emitter.instruction(&format!("cmp {}, {}", result_reg, len_reg));       // compare the requested index against the indexed-array length
    ctx.emitter.instruction(&format!("jge {}", missing));                       // out-of-bounds indexes make isset return false
    emit_isset_array_in_bounds_missing_x86_64(ctx, array_reg, result_reg, elem_ty)?;
    ctx.emitter.instruction(&format!("jmp {}", done));                          // skip the out-of-bounds isset result after an in-bounds probe
    ctx.emitter.label(missing);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 1);
    ctx.emitter.label(done);
    Ok(())
}

/// Emits AArch64 null handling for an in-bounds indexed-array `isset` probe.
fn emit_isset_array_in_bounds_missing_aarch64(
    ctx: &mut FunctionContext<'_>,
    array_reg: &str,
    index_reg: &str,
    elem_ty: &PhpType,
) -> Result<()> {
    match elem_ty.codegen_repr() {
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
        }
        PhpType::Mixed => {
            ctx.emitter.instruction(&format!("add {}, {}, #24", array_reg, array_reg)); // skip the indexed-array header to reach boxed Mixed elements
            ctx.emitter.instruction(&format!("ldr x0, [{}, {}, lsl #3]", array_reg, index_reg)); // load the boxed Mixed element pointer for null inspection
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction("cmp x0, #8");                              // runtime tag 8 means the indexed-array element is PHP null
            ctx.emitter.instruction("cset x0, eq");                             // return missing when the in-bounds Mixed element is null
        }
        _ => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
    }
    Ok(())
}

/// Emits x86_64 null handling for an in-bounds indexed-array `isset` probe.
fn emit_isset_array_in_bounds_missing_x86_64(
    ctx: &mut FunctionContext<'_>,
    array_reg: &str,
    index_reg: &str,
    elem_ty: &PhpType,
) -> Result<()> {
    match elem_ty.codegen_repr() {
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
        }
        PhpType::Mixed => {
            ctx.emitter.instruction(&format!("lea {}, [{} + 24]", array_reg, array_reg)); // skip the indexed-array header to reach boxed Mixed elements
            ctx.emitter.instruction(&format!("mov rax, QWORD PTR [{} + {} * 8]", array_reg, index_reg)); // load the boxed Mixed element pointer for null inspection
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction("cmp rax, 8");                              // runtime tag 8 means the indexed-array element is PHP null
            ctx.emitter.instruction("sete al");                                 // return missing when the in-bounds Mixed element is null
            ctx.emitter.instruction("movzx rax, al");                           // widen the Mixed null-check result into the integer result register
        }
        _ => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
    }
    Ok(())
}

/// Emits 1 when a scalar value equals the shared PHP null sentinel.
fn emit_scalar_value_is_null_sentinel(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    ctx.load_value_to_result(value)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x9", RUNTIME_NULL_SENTINEL);
            ctx.emitter.instruction("cmp x0, x9");                              // compare the scalar value against the shared null sentinel
            ctx.emitter.instruction("cset x0, eq");                             // return missing when the scalar value is the null sentinel
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "r10", RUNTIME_NULL_SENTINEL);
            ctx.emitter.instruction("cmp rax, r10");                            // compare the scalar value against the shared null sentinel
            ctx.emitter.instruction("sete al");                                 // set the low byte when the scalar value is the null sentinel
            ctx.emitter.instruction("movzx rax, al");                           // widen the null-sentinel predicate into the integer result register
        }
    }
    Ok(())
}

/// Emits 1 when a tagged scalar value carries PHP's null tag.
fn emit_tagged_scalar_value_is_null(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    ctx.load_value_to_result(value)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let cmp_inst = format!(
                "cmp x1, #{}",
                crate::codegen::sentinels::TAGGED_SCALAR_TAG_NULL
            );
            ctx.emitter.instruction(&cmp_inst);                                 // compare the tagged scalar tag against PHP null
            ctx.emitter.instruction("cset x0, eq");                             // report the operand as missing when it is null
        }
        Arch::X86_64 => {
            let cmp_inst = format!(
                "cmp rdx, {}",
                crate::codegen::sentinels::TAGGED_SCALAR_TAG_NULL
            );
            ctx.emitter.instruction(&cmp_inst);                                 // compare the tagged scalar tag against PHP null
            ctx.emitter.instruction("sete al");                                 // set the low byte when the tagged scalar is null
            ctx.emitter.instruction("movzx rax, al");                           // widen the null-check result into the integer result register
        }
    }
    Ok(())
}

/// Verifies that an `isset` offset probe receives an integer-like index.
fn require_isset_integer_like_index(
    ctx: &FunctionContext<'_>,
    index: ValueId,
    context: &str,
) -> Result<()> {
    match ctx.value_php_type(index)? {
        PhpType::Int | PhpType::Bool | PhpType::Callable => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "isset {} for PHP index type {:?}",
            context, other
        ))),
    }
}

/// Returns the instruction that produced an SSA value, when it has one.
fn source_instruction(ctx: &FunctionContext<'_>, value: ValueId) -> Result<Option<Instruction>> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    Ok(Some(inst_ref.clone()))
}
