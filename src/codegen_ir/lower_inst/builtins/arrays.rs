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
use crate::ir::{Immediate, Instruction, LocalSlotId, Op, ValueDef, ValueId};
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::super::{expect_operand, store_if_result};

mod key_exists;
mod keys;
mod values;

/// Lowers `array_sum()` over supported indexed-array payloads.
pub(super) fn lower_array_sum(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_indexed_array_aggregate(ctx, inst, "array_sum", "__rt_array_sum")
}

/// Lowers `array_product()` over supported indexed-array payloads.
pub(super) fn lower_array_product(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_indexed_array_aggregate(ctx, inst, "array_product", "__rt_array_product")
}

/// Lowers `array_fill()` for pointer-sized scalar and refcounted payloads.
pub(super) fn lower_array_fill(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "array_fill", 3)?;
    let start = expect_operand(inst, 0)?;
    let count = expect_operand(inst, 1)?;
    let value = expect_operand(inst, 2)?;
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    require_array_fill_value_type(&value_ty)?;
    let result_elem_ty = result_array_element_type("array_fill", &inst.result_php_type.codegen_repr())?;
    require_array_fill_result_type(&value_ty, &result_elem_ty)?;
    lower_array_fill_call(ctx, start, count, value, &value_ty)?;
    normalize_indexed_array_result(ctx, "array_fill", &value_ty, &result_elem_ty)?;
    store_if_result(ctx, inst)
}

/// Lowers `array_reverse()` for indexed arrays with 8-byte payload slots.
pub(super) fn lower_array_reverse(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "array_reverse", 1)?;
    let array = expect_operand(inst, 0)?;
    require_eight_byte_indexed_array(ctx.value_php_type(array)?, "array_reverse")?;
    ctx.load_value_to_result(array)?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the source indexed-array pointer as the reverse helper argument
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_reverse");
    store_if_result(ctx, inst)
}

/// Lowers `array_unique()` for indexed arrays with 8-byte payload slots.
pub(super) fn lower_array_unique(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "array_unique", 1)?;
    let array = expect_operand(inst, 0)?;
    require_eight_byte_indexed_array(ctx.value_php_type(array)?, "array_unique")?;
    ctx.load_value_to_result(array)?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the source indexed-array pointer as the dedup helper argument
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_unique");
    store_if_result(ctx, inst)
}

/// Lowers `array_merge()` for two compatible indexed arrays with 8-byte payload slots.
pub(super) fn lower_array_merge(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "array_merge", 2)?;
    let first = expect_operand(inst, 0)?;
    let second = expect_operand(inst, 1)?;
    require_compatible_eight_byte_indexed_arrays(
        ctx.value_php_type(first)?,
        ctx.value_php_type(second)?,
        "array_merge",
    )?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(first, "x0")?;
            ctx.load_value_to_reg(second, "x1")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(first, "rdi")?;
            ctx.load_value_to_reg(second, "rsi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_merge");
    store_if_result(ctx, inst)
}

/// Lowers `array_slice()` for indexed arrays with pointer-sized payload slots.
pub(super) fn lower_array_slice(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "array_slice", 2, 3)?;
    let array = expect_operand(inst, 0)?;
    let offset = expect_operand(inst, 1)?;
    let length = if inst.operands.len() == 3 {
        Some(expect_operand(inst, 2)?)
    } else {
        None
    };
    let source_elem_ty = array_slice_source_element_type(ctx.value_php_type(array)?)?;
    let result_elem_ty = result_array_element_type("array_slice", &inst.result_php_type.codegen_repr())?;
    require_array_slice_result_type(&source_elem_ty, &result_elem_ty)?;
    lower_array_slice_call(ctx, array, offset, length, &source_elem_ty)?;
    normalize_indexed_array_result(ctx, "array_slice", &source_elem_ty, &result_elem_ty)?;
    store_if_result(ctx, inst)
}

/// Lowers `array_values()` through the dedicated values-array builtin emitter.
pub(super) fn lower_array_values(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    values::lower_array_values(ctx, inst)
}

/// Lowers `array_keys()` through the dedicated keys-array builtin emitter.
pub(super) fn lower_array_keys(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    keys::lower_array_keys(ctx, inst)
}

/// Lowers `array_rand()` for indexed arrays.
pub(super) fn lower_array_rand(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "array_rand", 1)?;
    let array = expect_operand(inst, 0)?;
    require_indexed_array_builtin(ctx.value_php_type(array)?, "array_rand")?;
    ctx.load_value_to_result(array)?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the indexed-array pointer as the random-key helper argument
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_rand");
    store_if_result(ctx, inst)
}

/// Lowers `array_pop()` for indexed arrays by mutating length and boxing `T|null` as Mixed.
pub(super) fn lower_array_pop(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "array_pop", 1)?;
    let array = expect_operand(inst, 0)?;
    let elem_ty = array_pop_element_type(ctx.value_php_type(array)?)?;
    require_array_pop_result_type(&inst.result_php_type.codegen_repr())?;
    let source_local = source_load_local_slot(ctx, array)?;
    ensure_unique_array_pop_source(ctx, array)?;
    if let Some(slot) = source_local {
        ctx.store_value_to_local(slot, array)?;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_array_pop_aarch64(ctx, array, &elem_ty)?,
        Arch::X86_64 => lower_array_pop_x86_64(ctx, array, &elem_ty)?,
    }
    store_if_result(ctx, inst)
}

/// Lowers `array_key_exists()` through the dedicated key-existence builtin emitter.
pub(super) fn lower_array_key_exists(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    key_exists::lower_array_key_exists(ctx, inst)
}

/// Lowers `array_search()` for indexed arrays with integer-like payloads.
pub(super) fn lower_array_search(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "array_search", 2)?;
    let needle = expect_operand(inst, 0)?;
    let array = expect_operand(inst, 1)?;
    match supported_array_search_case(ctx.value_php_type(needle)?, ctx.value_php_type(array)?)? {
        ArraySearchCase::Empty => box_array_search_miss(ctx),
        ArraySearchCase::Scalar => lower_array_search_scalar(ctx, needle, array)?,
    }
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

/// Verifies a builtin can use scalar indexed-array helpers with 8-byte slots.
fn require_eight_byte_indexed_array(ty: PhpType, name: &str) -> Result<()> {
    let _ = eight_byte_indexed_array_element_type(ty, name)?;
    Ok(())
}

/// Verifies two indexed arrays can share an 8-byte scalar runtime helper.
fn require_compatible_eight_byte_indexed_arrays(
    first: PhpType,
    second: PhpType,
    name: &str,
) -> Result<()> {
    let first = eight_byte_indexed_array_element_type(first, name)?;
    let second = eight_byte_indexed_array_element_type(second, name)?;
    if first == second
        || matches!(first, PhpType::Never | PhpType::Void)
        || matches!(second, PhpType::Never | PhpType::Void)
    {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for incompatible indexed-array element PHP types {:?} and {:?}",
        name,
        first,
        second
    )))
}

/// Verifies that a builtin call has a lowered operand count within an inclusive range.
fn ensure_arg_count_between(inst: &Instruction, name: &str, min: usize, max: usize) -> Result<()> {
    let actual = inst.operands.len();
    if (min..=max).contains(&actual) {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "{} expected {}..={} args, got {}",
        name, min, max, actual
    )))
}

/// Verifies that `array_fill()` can store the value through existing runtime helpers.
fn require_array_fill_value_type(value_ty: &PhpType) -> Result<()> {
    if matches!(
        value_ty,
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Void
            | PhpType::Mixed
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
    ) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_fill value PHP type {:?}",
        value_ty
    )))
}

/// Verifies the destination element type matches the fill layout or is a Mixed widening.
fn require_array_fill_result_type(value_ty: &PhpType, result_elem_ty: &PhpType) -> Result<()> {
    if value_ty == result_elem_ty || result_elem_ty == &PhpType::Mixed {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_fill result element PHP type {:?} for value PHP type {:?}",
        result_elem_ty,
        value_ty
    )))
}

/// Calls the legacy runtime helper after materializing `array_fill()` arguments.
fn lower_array_fill_call(
    ctx: &mut FunctionContext<'_>,
    start: ValueId,
    count: ValueId,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(start, "x0")?;
            ctx.load_value_to_reg(count, "x1")?;
            ctx.load_value_to_reg(value, "x2")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(start, "rdi")?;
            ctx.load_value_to_reg(count, "rsi")?;
            ctx.load_value_to_reg(value, "rdx")?;
        }
    }
    abi::emit_call_label(ctx.emitter, array_fill_runtime_helper(value_ty));
    Ok(())
}

/// Returns the helper matching the fill value's ownership representation.
fn array_fill_runtime_helper(value_ty: &PhpType) -> &'static str {
    if value_ty.is_refcounted() {
        "__rt_array_fill_refcounted"
    } else {
        "__rt_array_fill"
    }
}

/// Returns the element type for indexed arrays supported by scalar 8-byte helpers.
fn eight_byte_indexed_array_element_type(ty: PhpType, name: &str) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => {
            let elem = elem.codegen_repr();
            if matches!(
                elem,
                PhpType::Int
                    | PhpType::Bool
                    | PhpType::Float
                    | PhpType::Callable
                    | PhpType::Void
                    | PhpType::Never
            ) {
                return Ok(elem);
            }
            Err(CodegenIrError::unsupported(format!(
                "{} for indexed-array element PHP type {:?}",
                name,
                elem
            )))
        }
        other => Err(CodegenIrError::unsupported(format!("{} for PHP type {:?}", name, other))),
    }
}

/// Returns the source element type when `array_slice()` can use legacy pointer-sized helpers.
fn array_slice_source_element_type(ty: PhpType) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => {
            let elem = elem.codegen_repr();
            require_array_slice_element_layout(&elem)?;
            Ok(elem)
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_slice for PHP type {:?}",
            other
        ))),
    }
}

/// Returns the result element type declared by the lowered builtin instruction.
fn result_array_element_type(name: &str, ty: &PhpType) -> Result<PhpType> {
    match ty {
        PhpType::Array(elem) => Ok(elem.codegen_repr()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} result PHP type {:?}",
            name, other
        ))),
    }
}

/// Verifies that the runtime slice helper can copy this element representation.
fn require_array_slice_element_layout(elem: &PhpType) -> Result<()> {
    if matches!(
        elem,
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Void
            | PhpType::Mixed
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
    ) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_slice indexed-array element PHP type {:?}",
        elem
    )))
}

/// Verifies the destination element type matches the copied layout or is a Mixed widening.
fn require_array_slice_result_type(source_elem_ty: &PhpType, result_elem_ty: &PhpType) -> Result<()> {
    if source_elem_ty == result_elem_ty || result_elem_ty == &PhpType::Mixed {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_slice result element PHP type {:?} for source element PHP type {:?}",
        result_elem_ty,
        source_elem_ty
    )))
}

/// Calls the appropriate legacy runtime helper after materializing slice arguments.
fn lower_array_slice_call(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    offset: ValueId,
    length: Option<ValueId>,
    source_elem_ty: &PhpType,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
            ctx.load_value_to_reg(offset, "x1")?;
            load_array_slice_length(ctx, length, "x2")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
            ctx.load_value_to_reg(offset, "rsi")?;
            load_array_slice_length(ctx, length, "rdx")?;
        }
    }
    abi::emit_call_label(ctx.emitter, array_slice_runtime_helper(source_elem_ty));
    Ok(())
}

/// Loads the optional slice length or the runtime until-end sentinel into `reg`.
fn load_array_slice_length(
    ctx: &mut FunctionContext<'_>,
    length: Option<ValueId>,
    reg: &str,
) -> Result<()> {
    let Some(length) = length else {
        emit_array_slice_until_end_sentinel(ctx, reg);
        return Ok(());
    };
    match ctx.value_php_type(length)?.codegen_repr() {
        PhpType::Int => {
            ctx.load_value_to_reg(length, reg)?;
        }
        PhpType::Void => emit_array_slice_until_end_sentinel(ctx, reg),
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_slice length PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Emits the `-1` runtime sentinel used when slicing to the end of the source array.
fn emit_array_slice_until_end_sentinel(ctx: &mut FunctionContext<'_>, reg: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov {}, #-1", reg));              // use -1 as the array_slice() runtime sentinel for length until the end
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov {}, -1", reg));               // use -1 as the x86_64 array_slice() runtime sentinel for length until the end
        }
    }
}

/// Returns the helper that matches the source element ownership representation.
fn array_slice_runtime_helper(source_elem_ty: &PhpType) -> &'static str {
    if source_elem_ty.is_refcounted() {
        "__rt_array_slice_refcounted"
    } else {
        "__rt_array_slice"
    }
}

/// Stamps the result array and widens typed slots when the EIR result expects Mixed.
fn normalize_indexed_array_result(
    ctx: &mut FunctionContext<'_>,
    name: &str,
    source_elem_ty: &PhpType,
    result_elem_ty: &PhpType,
) -> Result<()> {
    if result_elem_ty == &PhpType::Mixed && source_elem_ty != &PhpType::Mixed {
        let source_tag = runtime_value_tag(name, source_elem_ty)?;
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("mov x1, #{}", source_tag));   // pass the source slot value_type tag to widen the indexed-array result to Mixed
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rdi, rax");                        // pass the produced indexed-array pointer to the Mixed-widening helper
                ctx.emitter.instruction(&format!("mov rsi, {}", source_tag));   // pass the source slot value_type tag to widen the indexed-array result to Mixed
            }
        }
        abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
        return Ok(());
    }
    crate::codegen::emit_array_value_type_stamp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        result_elem_ty,
    );
    Ok(())
}

/// Returns the runtime value_type tag used by the array-to-Mixed widening helper.
fn runtime_value_tag(name: &str, elem: &PhpType) -> Result<u8> {
    match elem {
        PhpType::Int => Ok(0),
        PhpType::Str => Ok(1),
        PhpType::Float => Ok(2),
        PhpType::Bool => Ok(3),
        PhpType::Array(_) => Ok(4),
        PhpType::AssocArray { .. } => Ok(5),
        PhpType::Object(_) => Ok(6),
        PhpType::Mixed => Ok(7),
        PhpType::Void => Ok(8),
        other => Err(CodegenIrError::unsupported(format!(
            "{} Mixed widening for element PHP type {:?}",
            name, other
        ))),
    }
}

/// Verifies a builtin receives an indexed array operand.
fn require_indexed_array_builtin(ty: PhpType, name: &str) -> Result<()> {
    match ty.codegen_repr() {
        PhpType::Array(_) => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name,
            other
        ))),
    }
}

/// Returns the supported element payload type for an indexed-array `array_pop()`.
fn array_pop_element_type(ty: PhpType) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => {
            let elem = elem.codegen_repr();
            if matches!(
                elem,
                PhpType::Int
                    | PhpType::Bool
                    | PhpType::Float
                    | PhpType::Str
                    | PhpType::Callable
                    | PhpType::Mixed
                    | PhpType::Void
                    | PhpType::Never
            ) || elem.is_refcounted()
            {
                return Ok(elem);
            }
            Err(CodegenIrError::unsupported(format!(
                "array_pop indexed-array element PHP type {:?}",
                elem
            )))
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_pop for PHP type {:?}",
            other
        ))),
    }
}

/// Verifies the lowered `array_pop()` result uses PHP's `mixed` shape.
fn require_array_pop_result_type(result_ty: &PhpType) -> Result<()> {
    if result_ty == &PhpType::Mixed {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_pop result PHP type {:?}",
        result_ty
    )))
}

/// Splits a shared indexed array before `array_pop()` mutates its header.
fn ensure_unique_array_pop_source(ctx: &mut FunctionContext<'_>, array: ValueId) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_ensure_unique");
    ctx.store_result_value(array)
}

/// Emits the AArch64 `array_pop()` sequence for indexed arrays.
fn lower_array_pop_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    elem_ty: &PhpType,
) -> Result<()> {
    let empty_label = ctx.next_label("array_pop_empty");
    let done_label = ctx.next_label("array_pop_done");
    ctx.load_value_to_reg(array, "x0")?;
    ctx.emitter.instruction("ldr x9, [x0]");                                    // load the indexed-array length before deciding whether pop is empty
    ctx.emitter.instruction(&format!("cbz x9, {}", empty_label));               // return boxed null when array_pop() runs on an empty array
    ctx.emitter.instruction("sub x9, x9, #1");                                  // convert the old length into the removed last-element index
    ctx.emitter.instruction("str x9, [x0]");                                    // persist the shortened indexed-array length in the header
    emit_array_pop_value_aarch64(ctx, elem_ty)?;
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, elem_ty);
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the empty-array boxed-null path after loading the removed value
    ctx.emitter.label(&empty_label);
    emit_array_pop_null(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits the x86_64 `array_pop()` sequence for indexed arrays.
fn lower_array_pop_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    elem_ty: &PhpType,
) -> Result<()> {
    let empty_label = ctx.next_label("array_pop_empty");
    let done_label = ctx.next_label("array_pop_done");
    ctx.load_value_to_reg(array, "rax")?;
    ctx.emitter.instruction("mov r10, QWORD PTR [rax]");                        // load the indexed-array length before deciding whether pop is empty
    ctx.emitter.instruction("test r10, r10");                                   // check whether the indexed array has any live elements
    ctx.emitter.instruction(&format!("jz {}", empty_label));                    // return boxed null when array_pop() runs on an empty array
    ctx.emitter.instruction("sub r10, 1");                                      // convert the old length into the removed last-element index
    ctx.emitter.instruction("mov QWORD PTR [rax], r10");                        // persist the shortened indexed-array length in the header
    emit_array_pop_value_x86_64(ctx, elem_ty)?;
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, elem_ty);
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the empty-array boxed-null path after loading the removed value
    ctx.emitter.label(&empty_label);
    emit_array_pop_null(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Loads the removed AArch64 indexed-array payload into the canonical result registers.
fn emit_array_pop_value_aarch64(ctx: &mut FunctionContext<'_>, elem_ty: &PhpType) -> Result<()> {
    match elem_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Mixed => {
            ctx.emitter.instruction("add x10, x0, #24");                        // compute the first pointer-sized payload slot in the indexed array
            ctx.emitter.instruction("ldr x0, [x10, x9, lsl #3]");               // load the removed pointer-sized payload into the result register
        }
        PhpType::Float => {
            ctx.emitter.instruction("add x10, x0, #24");                        // compute the first float payload slot in the indexed array
            ctx.emitter.instruction("ldr d0, [x10, x9, lsl #3]");               // load the removed float payload into the result register
        }
        PhpType::Str => {
            ctx.emitter.instruction("lsl x10, x9, #4");                         // scale the removed index by the 16-byte string slot size
            ctx.emitter.instruction("add x10, x0, x10");                        // advance from the array base to the removed string slot
            ctx.emitter.instruction("add x10, x10, #24");                       // skip the indexed-array header before loading string payloads
            ctx.emitter.instruction("ldr x1, [x10]");                           // load the removed string pointer into the mixed payload register
            ctx.emitter.instruction("ldr x2, [x10, #8]");                       // load the removed string length into the mixed payload high word
        }
        PhpType::Void | PhpType::Never => {
            ctx.emitter.instruction("mov x0, #0");                              // materialize a null payload for impossible void-array live elements
        }
        other if other.is_refcounted() => {
            ctx.emitter.instruction("add x10, x0, #24");                        // compute the first refcounted payload slot in the indexed array
            ctx.emitter.instruction("ldr x0, [x10, x9, lsl #3]");               // load the removed heap pointer into the result register
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_pop element PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Loads the removed x86_64 indexed-array payload into the canonical result registers.
fn emit_array_pop_value_x86_64(ctx: &mut FunctionContext<'_>, elem_ty: &PhpType) -> Result<()> {
    match elem_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Mixed => {
            ctx.emitter.instruction("lea r11, [rax + 24]");                     // compute the first pointer-sized payload slot in the indexed array
            ctx.emitter.instruction("mov rax, QWORD PTR [r11 + r10 * 8]");      // load the removed pointer-sized payload into the result register
        }
        PhpType::Float => {
            ctx.emitter.instruction("lea r11, [rax + 24]");                     // compute the first float payload slot in the indexed array
            ctx.emitter.instruction("movsd xmm0, QWORD PTR [r11 + r10 * 8]");   // load the removed float payload into the result register
        }
        PhpType::Str => {
            ctx.emitter.instruction("lea r11, [rax + 24]");                     // compute the first string payload slot in the indexed array
            ctx.emitter.instruction("shl r10, 4");                              // scale the removed index by the 16-byte string slot size
            ctx.emitter.instruction("add r11, r10");                            // advance to the removed string slot payload
            ctx.emitter.instruction("mov rax, QWORD PTR [r11]");                // load the removed string pointer into the string result register
            ctx.emitter.instruction("mov rdx, QWORD PTR [r11 + 8]");            // load the removed string length into the string result register
        }
        PhpType::Void | PhpType::Never => {
            ctx.emitter.instruction("xor eax, eax");                            // materialize a null payload for impossible void-array live elements
        }
        other if other.is_refcounted() => {
            ctx.emitter.instruction("lea r11, [rax + 24]");                     // compute the first refcounted payload slot in the indexed array
            ctx.emitter.instruction("mov rax, QWORD PTR [r11 + r10 * 8]");      // load the removed heap pointer into the result register
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_pop element PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Boxes PHP null for an empty `array_pop()` result.
fn emit_array_pop_null(ctx: &mut FunctionContext<'_>) {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
}

/// Returns the local slot loaded by an `array_pop()` argument when it came from `load_local`.
fn source_load_local_slot(ctx: &FunctionContext<'_>, value: ValueId) -> Result<Option<LocalSlotId>> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let Some(inst_ref) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    if inst_ref.op == Op::LoadLocal {
        if let Some(Immediate::LocalSlot(slot)) = inst_ref.immediate {
            return Ok(Some(slot));
        }
    }
    Ok(None)
}

/// Describes which indexed-array `array_search()` lowering path applies.
enum ArraySearchCase {
    Empty,
    Scalar,
}

/// Verifies that an indexed-array `array_search()` call can use the scalar search helper.
fn supported_array_search_case(needle_ty: PhpType, array_ty: PhpType) -> Result<ArraySearchCase> {
    let needle_ty = needle_ty.codegen_repr();
    match array_ty.codegen_repr() {
        PhpType::Array(elem) => match elem.codegen_repr() {
            PhpType::Never | PhpType::Void => Ok(ArraySearchCase::Empty),
            PhpType::Int | PhpType::Bool if matches!(needle_ty, PhpType::Int | PhpType::Bool) => {
                Ok(ArraySearchCase::Scalar)
            }
            elem_ty => Err(CodegenIrError::unsupported(format!(
                "array_search needle PHP type {:?} for indexed-array element PHP type {:?}",
                needle_ty,
                elem_ty
            ))),
        },
        other => Err(CodegenIrError::unsupported(format!(
            "array_search for PHP array type {:?}",
            other
        ))),
    }
}

/// Lowers integer-like indexed-array search and boxes the PHP `int|false` result.
fn lower_array_search_scalar(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
            ctx.load_value_to_reg(needle, "x1")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
            ctx.load_value_to_reg(needle, "rsi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_search");
    box_array_search_result(ctx);
    Ok(())
}

/// Boxes a raw array-search helper result into PHP `int|false` Mixed form.
fn box_array_search_result(ctx: &mut FunctionContext<'_>) {
    let found_label = ctx.next_label("array_search_found");
    let end_label = ctx.next_label("array_search_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // distinguish a found index from the array_search() not-found sentinel
            ctx.emitter.instruction(&format!("b.ge {}", found_label));          // box a found index as an integer mixed result
            box_array_search_miss(ctx);
            ctx.emitter.instruction(&format!("b {}", end_label));               // skip integer boxing after producing false for a miss
            ctx.emitter.label(&found_label);
            ctx.emitter.instruction("mov x1, x0");                              // move the found index into the mixed helper payload register
            ctx.emitter.instruction("mov x2, #0");                              // integer mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #0");                              // runtime tag 0 = integer
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&end_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 0");                              // distinguish a found index from the array_search() not-found sentinel
            ctx.emitter.instruction(&format!("jge {}", found_label));           // box a found index as an integer mixed result
            box_array_search_miss(ctx);
            ctx.emitter.instruction(&format!("jmp {}", end_label));             // skip integer boxing after producing false for a miss
            ctx.emitter.label(&found_label);
            ctx.emitter.instruction("mov rdi, rax");                            // move the found index into the mixed helper payload register
            ctx.emitter.instruction("xor esi, esi");                            // integer mixed payloads do not use a high word
            ctx.emitter.instruction("xor eax, eax");                            // runtime tag 0 = integer
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&end_label);
        }
    }
}

/// Boxes `false` for an array-search miss.
fn box_array_search_miss(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, #0");                              // false mixed payload is zero
            ctx.emitter.instruction("mov x2, #0");                              // bool mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #3");                              // runtime tag 3 = bool
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("xor edi, edi");                            // false mixed payload is zero
            ctx.emitter.instruction("xor esi, esi");                            // bool mixed payloads do not use a high word
            ctx.emitter.instruction("mov eax, 3");                              // runtime tag 3 = bool
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
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
