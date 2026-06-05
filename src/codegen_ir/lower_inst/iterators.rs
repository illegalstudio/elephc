//! Purpose:
//! Lowers high-level EIR iterator opcodes for the Phase 04 backend.
//! Handles stack-resident iteration over indexed and associative arrays.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - `IterStart` values reserve a fixed stack state for source, cursor, and current hash payload.
//! - Current values are boxed into `Mixed` because Phase 03 foreach lowering stores loop variables as mixed.

use crate::codegen::platform::Arch;
use crate::codegen::{abi, emit_box_current_value_as_mixed, emit_box_runtime_payload_as_mixed};
use crate::ir::{Instruction, Op, ValueDef, ValueId};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};
use crate::codegen_ir::{CodegenIrError, Result};

const ITER_SOURCE_OFFSET_DELTA: usize = 0;
const ITER_CURSOR_OFFSET_DELTA: usize = 8;
const ITER_KEY_LO_OFFSET_DELTA: usize = 16;
const ITER_KEY_HI_OFFSET_DELTA: usize = 24;
const ITER_VALUE_LO_OFFSET_DELTA: usize = 32;
const ITER_VALUE_HI_OFFSET_DELTA: usize = 40;
const ITER_VALUE_TAG_OFFSET_DELTA: usize = 48;

enum IteratorSourceKind {
    Indexed { elem: PhpType },
    Hash,
}

/// Lowers iterator initialization by storing the source pointer and initial cursor.
pub(super) fn lower_iter_start(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let source = expect_operand(inst, 0)?;
    let source_kind = iterator_source_kind_from_type(&ctx.value_php_type(source)?, inst)?;
    let result = inst.result.ok_or_else(|| {
        CodegenIrError::invalid_module("iter_start missing result value".to_string())
    })?;
    let offset = ctx.value_frame_offset(result)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(source, result_reg)?;
    abi::store_at_offset(ctx.emitter, result_reg, offset - ITER_SOURCE_OFFSET_DELTA);
    let initial_cursor = match source_kind {
        IteratorSourceKind::Indexed { .. } => -1,
        IteratorSourceKind::Hash => 0,
    };
    abi::emit_load_int_immediate(ctx.emitter, result_reg, initial_cursor);
    abi::store_at_offset(ctx.emitter, result_reg, offset - ITER_CURSOR_OFFSET_DELTA);
    Ok(())
}

/// Lowers iterator advancement into a boolean result without moving past end.
pub(super) fn lower_iter_next(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let iterator = expect_operand(inst, 0)?;
    let offset = ctx.value_frame_offset(iterator)?;
    match iterator_source_kind(ctx, iterator, inst)? {
        IteratorSourceKind::Indexed { .. } => match ctx.emitter.target.arch {
            Arch::AArch64 => lower_indexed_iter_next_aarch64(ctx, offset),
            Arch::X86_64 => lower_indexed_iter_next_x86_64(ctx, offset),
        },
        IteratorSourceKind::Hash => match ctx.emitter.target.arch {
            Arch::AArch64 => lower_hash_iter_next_aarch64(ctx, offset),
            Arch::X86_64 => lower_hash_iter_next_x86_64(ctx, offset),
        },
    }
    store_if_result(ctx, inst)
}

/// Lowers the current iterator key by boxing it as a `Mixed` value.
pub(super) fn lower_iter_current_key(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let iterator = expect_operand(inst, 0)?;
    let offset = ctx.value_frame_offset(iterator)?;
    match iterator_source_kind(ctx, iterator, inst)? {
        IteratorSourceKind::Indexed { .. } => {
            let result_reg = abi::int_result_reg(ctx.emitter);
            abi::load_at_offset(ctx.emitter, result_reg, offset - ITER_CURSOR_OFFSET_DELTA);
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
        }
        IteratorSourceKind::Hash => match ctx.emitter.target.arch {
            Arch::AArch64 => load_current_hash_key_as_mixed_aarch64(ctx, offset),
            Arch::X86_64 => load_current_hash_key_as_mixed_x86_64(ctx, offset),
        },
    }
    store_if_result(ctx, inst)
}

/// Lowers the current iterator value and boxes it into the foreach `Mixed` result.
pub(super) fn lower_iter_current_value(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let iterator = expect_operand(inst, 0)?;
    let offset = ctx.value_frame_offset(iterator)?;
    match iterator_source_kind(ctx, iterator, inst)? {
        IteratorSourceKind::Indexed { elem } => {
            match ctx.emitter.target.arch {
                Arch::AArch64 => load_current_array_value_aarch64(ctx, offset, &elem)?,
                Arch::X86_64 => load_current_array_value_x86_64(ctx, offset, &elem)?,
            }
            emit_box_current_value_as_mixed(ctx.emitter, &elem);
        }
        IteratorSourceKind::Hash => match ctx.emitter.target.arch {
            Arch::AArch64 => load_current_hash_value_as_mixed_aarch64(ctx, offset),
            Arch::X86_64 => load_current_hash_value_as_mixed_x86_64(ctx, offset),
        },
    }
    store_if_result(ctx, inst)
}

/// Lowers iterator cleanup; Phase 04 array iterator state is stack-resident.
pub(super) fn lower_iter_end(_ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.result.is_some() {
        return Err(CodegenIrError::invalid_module(
            "iter_end must not produce a result".to_string(),
        ));
    }
    Ok(())
}

/// Emits AArch64 cursor advancement for a stack-resident indexed-array iterator.
fn lower_indexed_iter_next_aarch64(ctx: &mut FunctionContext<'_>, offset: usize) {
    let array_reg = "x12";
    let index_reg = abi::secondary_scratch_reg(ctx.emitter);
    let len_reg = abi::tertiary_scratch_reg(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    let done_label = ctx.next_label("iter_next_done");

    abi::load_at_offset_scratch(ctx.emitter, array_reg, offset - ITER_SOURCE_OFFSET_DELTA, "x9");
    abi::load_at_offset(ctx.emitter, index_reg, offset - ITER_CURSOR_OFFSET_DELTA);
    ctx.emitter.instruction(&format!("add {}, {}, #1", index_reg, index_reg));  // advance to the candidate indexed-array offset
    abi::emit_load_from_address(ctx.emitter, len_reg, array_reg, 0);
    ctx.emitter.instruction(&format!("cmp {}, {}", index_reg, len_reg));        // compare the candidate offset against the array length
    ctx.emitter.instruction(&format!("cset {}, lt", result_reg));               // materialize whether another element is available
    ctx.emitter.instruction(&format!("b.ge {}", done_label));                   // leave the cursor unchanged once iteration reaches the end
    abi::store_at_offset(ctx.emitter, index_reg, offset - ITER_CURSOR_OFFSET_DELTA);
    ctx.emitter.label(&done_label);
}

/// Emits x86_64 cursor advancement for a stack-resident indexed-array iterator.
fn lower_indexed_iter_next_x86_64(ctx: &mut FunctionContext<'_>, offset: usize) {
    let array_reg = abi::symbol_scratch_reg(ctx.emitter);
    let index_reg = abi::secondary_scratch_reg(ctx.emitter);
    let len_reg = abi::tertiary_scratch_reg(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    let done_label = ctx.next_label("iter_next_done");

    abi::load_at_offset(ctx.emitter, array_reg, offset - ITER_SOURCE_OFFSET_DELTA);
    abi::load_at_offset(ctx.emitter, index_reg, offset - ITER_CURSOR_OFFSET_DELTA);
    ctx.emitter.instruction(&format!("add {}, 1", index_reg));                  // advance to the candidate indexed-array offset
    abi::emit_load_from_address(ctx.emitter, len_reg, array_reg, 0);
    ctx.emitter.instruction(&format!("cmp {}, {}", index_reg, len_reg));        // compare the candidate offset against the array length
    ctx.emitter.instruction("setl al");                                         // materialize whether another element is available in the low result byte
    ctx.emitter.instruction(&format!("movzx {}, al", result_reg));              // widen the availability flag into the integer result register
    ctx.emitter.instruction(&format!("jge {}", done_label));                    // leave the cursor unchanged once iteration reaches the end
    abi::store_at_offset(ctx.emitter, index_reg, offset - ITER_CURSOR_OFFSET_DELTA);
    ctx.emitter.label(&done_label);
}

/// Emits AArch64 advancement for a stack-resident associative-array iterator.
fn lower_hash_iter_next_aarch64(ctx: &mut FunctionContext<'_>, offset: usize) {
    abi::load_at_offset(ctx.emitter, "x0", offset - ITER_SOURCE_OFFSET_DELTA);
    abi::load_at_offset(ctx.emitter, "x1", offset - ITER_CURSOR_OFFSET_DELTA);
    abi::emit_call_label(ctx.emitter, "__rt_hash_iter_next");
    ctx.emitter.instruction("cmn x0, #1");                                      // check whether the hash iterator returned the done sentinel
    abi::store_at_offset(ctx.emitter, "x0", offset - ITER_CURSOR_OFFSET_DELTA);
    abi::store_at_offset(ctx.emitter, "x1", offset - ITER_KEY_LO_OFFSET_DELTA);
    abi::store_at_offset(ctx.emitter, "x2", offset - ITER_KEY_HI_OFFSET_DELTA);
    abi::store_at_offset(ctx.emitter, "x3", offset - ITER_VALUE_LO_OFFSET_DELTA);
    abi::store_at_offset(ctx.emitter, "x4", offset - ITER_VALUE_HI_OFFSET_DELTA);
    abi::store_at_offset(ctx.emitter, "x5", offset - ITER_VALUE_TAG_OFFSET_DELTA);
    ctx.emitter.instruction("cset x0, ne");                                     // materialize whether the associative iterator has a current entry
}

/// Emits x86_64 advancement for a stack-resident associative-array iterator.
fn lower_hash_iter_next_x86_64(ctx: &mut FunctionContext<'_>, offset: usize) {
    abi::load_at_offset(ctx.emitter, "rdi", offset - ITER_SOURCE_OFFSET_DELTA);
    abi::load_at_offset(ctx.emitter, "rsi", offset - ITER_CURSOR_OFFSET_DELTA);
    abi::emit_call_label(ctx.emitter, "__rt_hash_iter_next");
    ctx.emitter.instruction("cmp rax, -1");                                     // check whether the hash iterator returned the done sentinel
    abi::store_at_offset(ctx.emitter, "rax", offset - ITER_CURSOR_OFFSET_DELTA);
    abi::store_at_offset(ctx.emitter, "rdi", offset - ITER_KEY_LO_OFFSET_DELTA);
    abi::store_at_offset(ctx.emitter, "rdx", offset - ITER_KEY_HI_OFFSET_DELTA);
    abi::store_at_offset(ctx.emitter, "rcx", offset - ITER_VALUE_LO_OFFSET_DELTA);
    abi::store_at_offset(ctx.emitter, "r8", offset - ITER_VALUE_HI_OFFSET_DELTA);
    abi::store_at_offset(ctx.emitter, "r9", offset - ITER_VALUE_TAG_OFFSET_DELTA);
    ctx.emitter.instruction("setne al");                                        // materialize whether the associative iterator has a current entry
    ctx.emitter.instruction("movzx rax, al");                                   // widen the availability flag into the integer result register
}

/// Boxes the current AArch64 hash key saved by `IterNext` into a `Mixed` cell.
fn load_current_hash_key_as_mixed_aarch64(ctx: &mut FunctionContext<'_>, offset: usize) {
    let key_string = ctx.next_label("iter_hash_key_string");
    let key_done = ctx.next_label("iter_hash_key_done");
    abi::load_at_offset(ctx.emitter, "x1", offset - ITER_KEY_LO_OFFSET_DELTA);
    abi::load_at_offset(ctx.emitter, "x2", offset - ITER_KEY_HI_OFFSET_DELTA);
    ctx.emitter.instruction("cmn x2, #1");                                      // check whether this normalized hash key is integer-backed
    ctx.emitter.instruction(&format!("b.ne {}", key_string));                   // branch to string-key boxing when key_hi is not the integer sentinel
    ctx.emitter.instruction("mov x0, #0");                                      // runtime tag 0 = integer mixed key
    ctx.emitter.instruction("mov x2, xzr");                                     // integer mixed payloads do not use a high word
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.instruction(&format!("b {}", key_done));                        // skip string-key boxing after producing the integer key box
    ctx.emitter.label(&key_string);
    ctx.emitter.instruction("mov x0, #1");                                      // runtime tag 1 = string mixed key
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.label(&key_done);
}

/// Boxes the current x86_64 hash key saved by `IterNext` into a `Mixed` cell.
fn load_current_hash_key_as_mixed_x86_64(ctx: &mut FunctionContext<'_>, offset: usize) {
    let key_string = ctx.next_label("iter_hash_key_string");
    let key_done = ctx.next_label("iter_hash_key_done");
    abi::load_at_offset(ctx.emitter, "rdi", offset - ITER_KEY_LO_OFFSET_DELTA);
    abi::load_at_offset(ctx.emitter, "rdx", offset - ITER_KEY_HI_OFFSET_DELTA);
    ctx.emitter.instruction("cmp rdx, -1");                                     // check whether this normalized hash key is integer-backed
    ctx.emitter.instruction(&format!("jne {}", key_string));                    // branch to string-key boxing when key_hi is not the integer sentinel
    ctx.emitter.instruction("xor esi, esi");                                    // integer mixed payloads do not use a high word
    ctx.emitter.instruction("mov eax, 0");                                      // runtime tag 0 = integer mixed key
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.instruction(&format!("jmp {}", key_done));                      // skip string-key boxing after producing the integer key box
    ctx.emitter.label(&key_string);
    ctx.emitter.instruction("mov rsi, rdx");                                    // move the string key length into the mixed helper high-word register
    ctx.emitter.instruction("mov eax, 1");                                      // runtime tag 1 = string mixed key
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.label(&key_done);
}

/// Boxes the current AArch64 hash value payload saved by `IterNext` into `Mixed`.
fn load_current_hash_value_as_mixed_aarch64(ctx: &mut FunctionContext<'_>, offset: usize) {
    abi::load_at_offset(ctx.emitter, "x5", offset - ITER_VALUE_TAG_OFFSET_DELTA);
    abi::load_at_offset(ctx.emitter, "x3", offset - ITER_VALUE_LO_OFFSET_DELTA);
    abi::load_at_offset(ctx.emitter, "x4", offset - ITER_VALUE_HI_OFFSET_DELTA);
    emit_box_runtime_payload_as_mixed(ctx.emitter, "x5", "x3", "x4");
}

/// Boxes the current x86_64 hash value payload saved by `IterNext` into `Mixed`.
fn load_current_hash_value_as_mixed_x86_64(ctx: &mut FunctionContext<'_>, offset: usize) {
    abi::load_at_offset(ctx.emitter, "r9", offset - ITER_VALUE_TAG_OFFSET_DELTA);
    abi::load_at_offset(ctx.emitter, "rcx", offset - ITER_VALUE_LO_OFFSET_DELTA);
    abi::load_at_offset(ctx.emitter, "r8", offset - ITER_VALUE_HI_OFFSET_DELTA);
    emit_box_runtime_payload_as_mixed(ctx.emitter, "r9", "rcx", "r8");
}

/// Loads the current indexed-array element into AArch64 result registers.
fn load_current_array_value_aarch64(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    elem_ty: &PhpType,
) -> Result<()> {
    let array_reg = "x12";
    let index_reg = abi::secondary_scratch_reg(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::load_at_offset_scratch(ctx.emitter, array_reg, offset - ITER_SOURCE_OFFSET_DELTA, "x9");
    abi::load_at_offset(ctx.emitter, index_reg, offset - ITER_CURSOR_OFFSET_DELTA);
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
    abi::load_at_offset(ctx.emitter, index_reg, offset - ITER_CURSOR_OFFSET_DELTA);
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

/// Returns the source layout handled by a stack-resident iterator.
fn iterator_source_kind(
    ctx: &FunctionContext<'_>,
    iterator: ValueId,
    inst: &Instruction,
) -> Result<IteratorSourceKind> {
    iterator_source_kind_from_type(&iterator_source_type(ctx, iterator, inst)?, inst)
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

/// Classifies iterator sources whose storage layouts are handled here.
fn iterator_source_kind_from_type(ty: &PhpType, inst: &Instruction) -> Result<IteratorSourceKind> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => Ok(IteratorSourceKind::Indexed { elem: elem.codegen_repr() }),
        PhpType::AssocArray { .. } => Ok(IteratorSourceKind::Hash),
        other => Err(CodegenIrError::unsupported(format!(
            "{} over PHP type {:?}",
            inst.op.name(),
            other
        ))),
    }
}
