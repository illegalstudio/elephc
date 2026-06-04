//! Purpose:
//! Lowers high-level EIR iterator opcodes for the Phase 04 backend.
//! Starts with stack-resident iteration over indexed arrays.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - `IterStart` values reserve two spill words: source array pointer and current index.
//! - Current values are boxed into `Mixed` because Phase 03 foreach lowering stores loop variables as mixed.

use crate::codegen::platform::Arch;
use crate::codegen::{abi, emit_box_current_value_as_mixed};
use crate::ir::{Instruction, Op, ValueDef, ValueId};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};
use crate::codegen_ir::{CodegenIrError, Result};

const ITER_SOURCE_OFFSET_DELTA: usize = 0;
const ITER_INDEX_OFFSET_DELTA: usize = 8;

/// Lowers iterator initialization by storing source array pointer and cursor `-1`.
pub(super) fn lower_iter_start(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let source = expect_operand(inst, 0)?;
    require_indexed_array_source(&ctx.value_php_type(source)?, inst)?;
    let result = inst.result.ok_or_else(|| {
        CodegenIrError::invalid_module("iter_start missing result value".to_string())
    })?;
    let offset = ctx.value_frame_offset(result)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(source, result_reg)?;
    abi::store_at_offset(ctx.emitter, result_reg, offset - ITER_SOURCE_OFFSET_DELTA);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, -1);
    abi::store_at_offset(ctx.emitter, result_reg, offset - ITER_INDEX_OFFSET_DELTA);
    Ok(())
}

/// Lowers iterator advancement into a boolean result without moving past end.
pub(super) fn lower_iter_next(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let iterator = expect_operand(inst, 0)?;
    require_indexed_iterator(ctx, iterator, inst)?;
    let offset = ctx.value_frame_offset(iterator)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_iter_next_aarch64(ctx, offset),
        Arch::X86_64 => lower_iter_next_x86_64(ctx, offset),
    }
    store_if_result(ctx, inst)
}

/// Lowers the current indexed-array key by boxing the current cursor as `Mixed`.
pub(super) fn lower_iter_current_key(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let iterator = expect_operand(inst, 0)?;
    require_indexed_iterator(ctx, iterator, inst)?;
    let offset = ctx.value_frame_offset(iterator)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, result_reg, offset - ITER_INDEX_OFFSET_DELTA);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
    store_if_result(ctx, inst)
}

/// Lowers the current indexed-array element and boxes it into the foreach `Mixed` result.
pub(super) fn lower_iter_current_value(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let iterator = expect_operand(inst, 0)?;
    let elem_ty = indexed_iterator_element_type(ctx, iterator, inst)?;
    let offset = ctx.value_frame_offset(iterator)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => load_current_array_value_aarch64(ctx, offset, &elem_ty)?,
        Arch::X86_64 => load_current_array_value_x86_64(ctx, offset, &elem_ty)?,
    }
    emit_box_current_value_as_mixed(ctx.emitter, &elem_ty);
    store_if_result(ctx, inst)
}

/// Lowers iterator cleanup; indexed-array iterator state is stack-resident.
pub(super) fn lower_iter_end(_ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.result.is_some() {
        return Err(CodegenIrError::invalid_module(
            "iter_end must not produce a result".to_string(),
        ));
    }
    Ok(())
}

/// Emits AArch64 cursor advancement for a stack-resident indexed-array iterator.
fn lower_iter_next_aarch64(ctx: &mut FunctionContext<'_>, offset: usize) {
    let array_reg = abi::symbol_scratch_reg(ctx.emitter);
    let index_reg = abi::secondary_scratch_reg(ctx.emitter);
    let len_reg = abi::tertiary_scratch_reg(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    let done_label = ctx.next_label("iter_next_done");

    abi::load_at_offset(ctx.emitter, array_reg, offset - ITER_SOURCE_OFFSET_DELTA);
    abi::load_at_offset(ctx.emitter, index_reg, offset - ITER_INDEX_OFFSET_DELTA);
    ctx.emitter.instruction(&format!("add {}, {}, #1", index_reg, index_reg));  // advance to the candidate indexed-array offset
    abi::emit_load_from_address(ctx.emitter, len_reg, array_reg, 0);
    ctx.emitter.instruction(&format!("cmp {}, {}", index_reg, len_reg));        // compare the candidate offset against the array length
    ctx.emitter.instruction(&format!("cset {}, lt", result_reg));               // materialize whether another element is available
    ctx.emitter.instruction(&format!("b.ge {}", done_label));                   // leave the cursor unchanged once iteration reaches the end
    abi::store_at_offset(ctx.emitter, index_reg, offset - ITER_INDEX_OFFSET_DELTA);
    ctx.emitter.label(&done_label);
}

/// Emits x86_64 cursor advancement for a stack-resident indexed-array iterator.
fn lower_iter_next_x86_64(ctx: &mut FunctionContext<'_>, offset: usize) {
    let array_reg = abi::symbol_scratch_reg(ctx.emitter);
    let index_reg = abi::secondary_scratch_reg(ctx.emitter);
    let len_reg = abi::tertiary_scratch_reg(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    let done_label = ctx.next_label("iter_next_done");

    abi::load_at_offset(ctx.emitter, array_reg, offset - ITER_SOURCE_OFFSET_DELTA);
    abi::load_at_offset(ctx.emitter, index_reg, offset - ITER_INDEX_OFFSET_DELTA);
    ctx.emitter.instruction(&format!("add {}, 1", index_reg));                  // advance to the candidate indexed-array offset
    abi::emit_load_from_address(ctx.emitter, len_reg, array_reg, 0);
    ctx.emitter.instruction(&format!("cmp {}, {}", index_reg, len_reg));        // compare the candidate offset against the array length
    ctx.emitter.instruction("setl al");                                         // materialize whether another element is available in the low result byte
    ctx.emitter.instruction(&format!("movzx {}, al", result_reg));              // widen the availability flag into the integer result register
    ctx.emitter.instruction(&format!("jge {}", done_label));                    // leave the cursor unchanged once iteration reaches the end
    abi::store_at_offset(ctx.emitter, index_reg, offset - ITER_INDEX_OFFSET_DELTA);
    ctx.emitter.label(&done_label);
}

/// Loads the current indexed-array element into AArch64 result registers.
fn load_current_array_value_aarch64(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    elem_ty: &PhpType,
) -> Result<()> {
    let array_reg = abi::symbol_scratch_reg(ctx.emitter);
    let index_reg = abi::secondary_scratch_reg(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, array_reg, offset - ITER_SOURCE_OFFSET_DELTA);
    abi::load_at_offset(ctx.emitter, index_reg, offset - ITER_INDEX_OFFSET_DELTA);
    match elem_ty {
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
        }
        PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Mixed => {
            ctx.emitter.instruction(&format!("add {}, {}, #24", array_reg, array_reg)); // skip the indexed-array header to reach element payloads
            ctx.emitter.instruction(&format!("ldr {}, [{}, {}, lsl #3]", result_reg, array_reg, index_reg)); // load the selected pointer-sized indexed-array element
        }
        PhpType::Float => {
            ctx.emitter.instruction(&format!("add {}, {}, #24", array_reg, array_reg)); // skip the indexed-array header to reach float payloads
            ctx.emitter.instruction(&format!("ldr d0, [{}, {}, lsl #3]", array_reg, index_reg)); // load the selected indexed-array float element
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            ctx.emitter.instruction(&format!("lsl {}, {}, #4", index_reg, index_reg)); // scale the string-array offset by pointer-plus-length slot size
            ctx.emitter.instruction(&format!("add {}, {}, {}", array_reg, array_reg, index_reg)); // move to the selected string slot within the indexed array
            ctx.emitter.instruction(&format!("add {}, {}, #24", array_reg, array_reg)); // skip the indexed-array header before loading the string slot
            abi::emit_load_from_address(ctx.emitter, ptr_reg, array_reg, 0);
            abi::emit_load_from_address(ctx.emitter, len_reg, array_reg, 8);
        }
        other if other.is_refcounted() => {
            ctx.emitter.instruction(&format!("add {}, {}, #24", array_reg, array_reg)); // skip the indexed-array header to reach refcounted payloads
            ctx.emitter.instruction(&format!("ldr {}, [{}, {}, lsl #3]", result_reg, array_reg, index_reg)); // load the selected refcounted indexed-array element
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "indexed iterator value PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Loads the current indexed-array element into x86_64 result registers.
fn load_current_array_value_x86_64(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    elem_ty: &PhpType,
) -> Result<()> {
    let array_reg = abi::symbol_scratch_reg(ctx.emitter);
    let index_reg = abi::secondary_scratch_reg(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, array_reg, offset - ITER_SOURCE_OFFSET_DELTA);
    abi::load_at_offset(ctx.emitter, index_reg, offset - ITER_INDEX_OFFSET_DELTA);
    match elem_ty {
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
        }
        PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Mixed => {
            ctx.emitter.instruction(&format!("lea {}, [{} + 24]", array_reg, array_reg)); // skip the indexed-array header to reach element payloads
            ctx.emitter.instruction(&format!("mov {}, QWORD PTR [{} + {} * 8]", result_reg, array_reg, index_reg)); // load the selected pointer-sized indexed-array element
        }
        PhpType::Float => {
            ctx.emitter.instruction(&format!("lea {}, [{} + 24]", array_reg, array_reg)); // skip the indexed-array header to reach float payloads
            ctx.emitter.instruction(&format!("movsd xmm0, QWORD PTR [{} + {} * 8]", array_reg, index_reg)); // load the selected indexed-array float element
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            ctx.emitter.instruction(&format!("shl {}, 4", index_reg));          // scale the string-array offset by pointer-plus-length slot size
            ctx.emitter.instruction(&format!("add {}, {}", array_reg, index_reg)); // move to the selected string slot within the indexed array
            ctx.emitter.instruction(&format!("add {}, 24", array_reg));         // skip the indexed-array header before loading the string slot
            abi::emit_load_from_address(ctx.emitter, ptr_reg, array_reg, 0);
            abi::emit_load_from_address(ctx.emitter, len_reg, array_reg, 8);
        }
        other if other.is_refcounted() => {
            ctx.emitter.instruction(&format!("lea {}, [{} + 24]", array_reg, array_reg)); // skip the indexed-array header to reach refcounted payloads
            ctx.emitter.instruction(&format!("mov {}, QWORD PTR [{} + {} * 8]", result_reg, array_reg, index_reg)); // load the selected refcounted indexed-array element
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "indexed iterator value PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Returns the source array element type for an indexed-array iterator.
fn indexed_iterator_element_type(
    ctx: &FunctionContext<'_>,
    iterator: ValueId,
    inst: &Instruction,
) -> Result<PhpType> {
    match iterator_source_type(ctx, iterator, inst)? {
        PhpType::Array(elem) => Ok(elem.codegen_repr()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} over PHP type {:?}",
            inst.op.name(),
            other
        ))),
    }
}

/// Verifies that an iterator handle was created from an indexed array.
fn require_indexed_iterator(
    ctx: &FunctionContext<'_>,
    iterator: ValueId,
    inst: &Instruction,
) -> Result<()> {
    require_indexed_array_source(&iterator_source_type(ctx, iterator, inst)?, inst)
}

/// Returns the source PHP type referenced by an `IterStart` result value.
fn iterator_source_type(
    ctx: &FunctionContext<'_>,
    iterator: ValueId,
    inst: &Instruction,
) -> Result<PhpType> {
    let source = iterator_source_value(ctx, iterator, inst)?;
    ctx.value_php_type(source)
}

/// Returns the source operand for an iterator handle, rejecting malformed EIR.
fn iterator_source_value(
    ctx: &FunctionContext<'_>,
    iterator: ValueId,
    inst: &Instruction,
) -> Result<ValueId> {
    let value = ctx
        .function
        .value(iterator)
        .ok_or_else(|| CodegenIrError::missing_entry("value", iterator.as_raw()))?;
    let ValueDef::Instruction { inst: iter_start, .. } = value.def else {
        return Err(CodegenIrError::invalid_module(format!(
            "{} operand is not an iterator value",
            inst.op.name()
        )));
    };
    let iter_start = ctx
        .function
        .instruction(iter_start)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", iter_start.as_raw()))?;
    if iter_start.op != Op::IterStart {
        return Err(CodegenIrError::invalid_module(format!(
            "{} operand was produced by {} instead of iter_start",
            inst.op.name(),
            iter_start.op.name()
        )));
    }
    iter_start
        .operands
        .first()
        .copied()
        .ok_or_else(|| CodegenIrError::invalid_module("iter_start missing source operand".to_string()))
}

/// Verifies an iterator source uses the indexed-array storage layout handled here.
fn require_indexed_array_source(ty: &PhpType, inst: &Instruction) -> Result<()> {
    match ty {
        PhpType::Array(_) => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} over PHP type {:?}",
            inst.op.name(),
            other
        ))),
    }
}
