//! Purpose:
//! Lowers high-level EIR iterator opcodes for the Phase 04 backend.
//! Handles stack-resident iteration over indexed and associative arrays.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - `IterStart` values reserve a fixed stack state for source, cursor, and current hash payload.
//! - Current values are boxed into `Mixed` unless EIR preserves a concrete indexed-array element type.

use crate::codegen::platform::Arch;
use crate::codegen::{abi, emit_box_current_value_as_mixed, emit_box_runtime_payload_as_mixed};
use crate::intrinsics::IntrinsicCall;
use crate::ir::{Immediate, Instruction, LocalSlotId, Op, ValueDef, ValueId};
use crate::names::{method_symbol, php_symbol_key};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{direct_call_stack_pad_bytes, expect_local_slot, expect_operand, store_if_result};
use crate::codegen_ir::{CodegenIrError, Result};

const ITER_SOURCE_OFFSET_DELTA: usize = 0;
const ITER_CURSOR_OFFSET_DELTA: usize = 8;
const ITER_KEY_LO_OFFSET_DELTA: usize = 16;
const ITER_KEY_HI_OFFSET_DELTA: usize = 24;
const ITER_VALUE_LO_OFFSET_DELTA: usize = 32;
const ITER_VALUE_HI_OFFSET_DELTA: usize = 40;
const ITER_VALUE_TAG_OFFSET_DELTA: usize = 48;
const ITER_VALUE_ADDR_OFFSET_DELTA: usize = 56;

enum IteratorSourceKind {
    Indexed { elem: PhpType },
    Hash,
    DynamicIterable,
    DynamicMixed,
    Object {
        class_name: String,
        aggregate_class_name: Option<String>,
    },
    Interface {
        interface_name: String,
        aggregate_class_name: Option<String>,
    },
}

/// Lowers iterator initialization by storing the source pointer and initial cursor.
pub(super) fn lower_iter_start(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let source = expect_operand(inst, 0)?;
    let source_kind = iterator_source_kind_from_type(ctx, &ctx.value_php_type(source)?, inst)?;
    let by_ref = iter_start_is_by_ref(inst);
    let result = inst.result.ok_or_else(|| {
        CodegenIrError::invalid_module("iter_start missing result value".to_string())
    })?;
    let offset = ctx.value_frame_offset(result)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(source, result_reg)?;
    if matches!(source_kind, IteratorSourceKind::DynamicMixed) {
        initialize_dynamic_mixed_iterator(ctx, offset, by_ref)?;
        return Ok(());
    }
    if by_ref {
        ensure_unique_static_iter_source(ctx, source, &source_kind)?;
    }
    abi::store_at_offset(ctx.emitter, result_reg, offset - ITER_SOURCE_OFFSET_DELTA);
    if matches!(source_kind, IteratorSourceKind::DynamicIterable) {
        initialize_dynamic_iterable_iterator(ctx, offset, by_ref, source)?;
        return Ok(());
    }
    if let IteratorSourceKind::Object {
        class_name,
        aggregate_class_name: None,
    } = &source_kind
    {
        abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Object(class_name.clone()));
    }
    if let IteratorSourceKind::Interface {
        interface_name,
        aggregate_class_name: None,
    } = &source_kind
    {
        abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Object(interface_name.clone()));
    }
    let initial_cursor = match &source_kind {
        IteratorSourceKind::Indexed { .. } => -1,
        IteratorSourceKind::Hash => 0,
        IteratorSourceKind::DynamicIterable => 0,
        IteratorSourceKind::DynamicMixed => 0,
        IteratorSourceKind::Object { .. } => 0,
        IteratorSourceKind::Interface { .. } => 0,
    };
    match &source_kind {
        IteratorSourceKind::Object {
            aggregate_class_name: Some(aggregate_class_name),
            ..
        }
        | IteratorSourceKind::Interface {
            aggregate_class_name: Some(aggregate_class_name),
            ..
        } => {
            emit_object_iterator_method_call(ctx, offset, aggregate_class_name, "getIterator")?;
            abi::store_at_offset(ctx.emitter, result_reg, offset - ITER_SOURCE_OFFSET_DELTA);
        }
        _ => {}
    }
    abi::emit_load_int_immediate(ctx.emitter, result_reg, initial_cursor);
    abi::store_at_offset(ctx.emitter, result_reg, offset - ITER_CURSOR_OFFSET_DELTA);
    match source_kind {
        IteratorSourceKind::Object { class_name, .. } => {
            emit_object_iterator_method_call(ctx, offset, &class_name, "rewind")?;
        }
        IteratorSourceKind::Interface { interface_name, .. } => {
            emit_interface_iterator_method_call(ctx, offset, &interface_name, "rewind")?;
        }
        _ => {}
    }
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
        IteratorSourceKind::DynamicIterable | IteratorSourceKind::DynamicMixed => {
            lower_dynamic_iter_next(ctx, offset)?;
        }
        IteratorSourceKind::Object { class_name, .. } => {
            lower_object_iter_next(ctx, offset, &class_name)?;
        }
        IteratorSourceKind::Interface { interface_name, .. } => {
            lower_interface_iter_next(ctx, offset, &interface_name)?;
        }
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
        IteratorSourceKind::DynamicIterable | IteratorSourceKind::DynamicMixed => {
            lower_dynamic_iter_current_key(ctx, inst, offset)?;
        }
        IteratorSourceKind::Object { class_name, .. } => {
            let return_ty = emit_object_iterator_method_call(ctx, offset, &class_name, "key")?;
            box_iterator_method_result_if_needed(ctx, inst, &return_ty)?;
        }
        IteratorSourceKind::Interface { interface_name, .. } => {
            let return_ty = emit_interface_iterator_method_call(ctx, offset, &interface_name, "key")?;
            box_iterator_method_result_if_needed(ctx, inst, &return_ty)?;
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers the current iterator value into the EIR result representation.
pub(super) fn lower_iter_current_value(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let iterator = expect_operand(inst, 0)?;
    let offset = ctx.value_frame_offset(iterator)?;
    let result_ty = iter_current_result_type(ctx, inst)?;
    match iterator_source_kind(ctx, iterator, inst)? {
        IteratorSourceKind::Indexed { elem } => {
            match ctx.emitter.target.arch {
                Arch::AArch64 => load_current_array_value_aarch64(ctx, offset, &elem)?,
                Arch::X86_64 => load_current_array_value_x86_64(ctx, offset, &elem)?,
            }
            box_current_indexed_value_if_needed(ctx, &elem, &result_ty)?;
        }
        IteratorSourceKind::Hash => match ctx.emitter.target.arch {
            Arch::AArch64 => load_current_hash_value_as_mixed_aarch64(ctx, offset),
            Arch::X86_64 => load_current_hash_value_as_mixed_x86_64(ctx, offset),
        },
        IteratorSourceKind::DynamicIterable | IteratorSourceKind::DynamicMixed => {
            lower_dynamic_iter_current_value(ctx, inst, offset)?;
        }
        IteratorSourceKind::Object { class_name, .. } => {
            let return_ty = emit_object_iterator_method_call(ctx, offset, &class_name, "current")?;
            box_iterator_method_result_if_needed(ctx, inst, &return_ty)?;
        }
        IteratorSourceKind::Interface { interface_name, .. } => {
            let return_ty = emit_interface_iterator_method_call(ctx, offset, &interface_name, "current")?;
            box_iterator_method_result_if_needed(ctx, inst, &return_ty)?;
        }
    }
    store_if_result(ctx, inst)
}

/// Returns the declared PHP result type for an `iter_current_value` instruction.
fn iter_current_result_type(ctx: &FunctionContext<'_>, inst: &Instruction) -> Result<PhpType> {
    let Some(result) = inst.result else {
        return Ok(PhpType::Void);
    };
    Ok(ctx.value_php_type(result)?.codegen_repr())
}

/// Boxes an indexed iterator element only when the EIR result expects `Mixed`.
fn box_current_indexed_value_if_needed(
    ctx: &mut FunctionContext<'_>,
    elem: &PhpType,
    result_ty: &PhpType,
) -> Result<()> {
    match result_ty.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => {
            emit_box_current_value_as_mixed(ctx.emitter, elem);
            Ok(())
        }
        result_ty if result_ty == elem.codegen_repr() => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "indexed iterator value PHP type {:?} stored as {:?}",
            elem,
            other
        ))),
    }
}

/// Binds a local slot to the current iterator value address for by-reference foreach.
pub(super) fn lower_iter_current_value_ref(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let iterator = expect_operand(inst, 0)?;
    let slot = expect_local_slot(inst)?;
    let offset = ctx.value_frame_offset(iterator)?;
    match iterator_source_kind(ctx, iterator, inst)? {
        IteratorSourceKind::Indexed { elem } => {
            bind_indexed_current_value_ref(ctx, offset, slot, &elem)?;
        }
        IteratorSourceKind::Hash => {
            bind_hash_current_value_ref(ctx, offset, slot)?;
        }
        IteratorSourceKind::DynamicIterable | IteratorSourceKind::DynamicMixed => {
            bind_dynamic_current_value_ref(ctx, offset, slot)?;
        }
        IteratorSourceKind::Object { .. } | IteratorSourceKind::Interface { .. } => {
            return Err(CodegenIrError::unsupported(
                "by-reference foreach over object iterators in EIR backend",
            ))
        }
    }
    Ok(())
}

/// Initializes an `Iterable`-typed iterator by dispatching on the source heap kind.
fn initialize_dynamic_iterable_iterator(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    by_ref: bool,
    source: ValueId,
) -> Result<()> {
    let indexed_case = ctx.next_label("iter_start_dyn_indexed");
    let hash_case = ctx.next_label("iter_start_dyn_hash");
    let object_case = ctx.next_label("iter_start_dyn_object");
    let done = ctx.next_label("iter_start_dyn_done");
    branch_on_dynamic_source_heap_kind(ctx, offset, &indexed_case, &hash_case, &object_case);
    abi::emit_call_label(ctx.emitter, "__rt_iterable_unsupported_kind");

    ctx.emitter.label(&indexed_case);
    if by_ref {
        convert_dynamic_indexed_source_for_ref(ctx, offset)?;
        store_iter_source_to_origin_if_local(ctx, offset, source)?;
    }
    store_iterator_cursor(ctx, offset, -1);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&hash_case);
    if by_ref {
        convert_dynamic_hash_source_for_ref(ctx, offset)?;
        store_iter_source_to_origin_if_local(ctx, offset, source)?;
    }
    store_iterator_cursor(ctx, offset, 0);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&object_case);
    store_iterator_cursor(ctx, offset, 0);
    resolve_dynamic_object_iterator_source(ctx, offset)?;
    emit_interface_iterator_method_call(ctx, offset, "Iterator", "rewind")?;
    ctx.emitter.label(&done);
    Ok(())
}

/// Initializes a `Mixed`-typed iterator by unboxing the source once into raw iterable state.
fn initialize_dynamic_mixed_iterator(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    by_ref: bool,
) -> Result<()> {
    let indexed_case = ctx.next_label("iter_start_mixed_indexed");
    let hash_case = ctx.next_label("iter_start_mixed_hash");
    let object_case = ctx.next_label("iter_start_mixed_object");
    let done = ctx.next_label("iter_start_mixed_done");
    if by_ref {
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    branch_on_mixed_iterable_tag(ctx, &indexed_case, &hash_case, &object_case);
    abi::emit_call_label(ctx.emitter, "__rt_iterable_unsupported_kind");

    ctx.emitter.label(&indexed_case);
    if by_ref {
        convert_mixed_indexed_source_for_ref(ctx, offset)?;
    } else {
        store_mixed_payload_low_as_iterator_source(ctx, offset);
    }
    store_iterator_cursor(ctx, offset, -1);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&hash_case);
    if by_ref {
        convert_mixed_hash_source_for_ref(ctx, offset)?;
    } else {
        store_mixed_payload_low_as_iterator_source(ctx, offset);
    }
    store_iterator_cursor(ctx, offset, 0);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&object_case);
    store_mixed_payload_low_as_iterator_source(ctx, offset);
    store_iterator_cursor(ctx, offset, 0);
    resolve_dynamic_object_iterator_source(ctx, offset)?;
    emit_interface_iterator_method_call(ctx, offset, "Iterator", "rewind")?;
    ctx.emitter.label(&done);
    if by_ref {
        abi::emit_pop_reg(ctx.emitter, abi::temp_int_reg(ctx.emitter.target));
    }
    Ok(())
}

/// Returns true when an `iter_start` instruction is preparing a by-reference foreach.
fn iter_start_is_by_ref(inst: &Instruction) -> bool {
    matches!(inst.immediate, Some(Immediate::Bool(true)))
}

/// Splits statically typed array/hash sources before by-reference iteration.
fn ensure_unique_static_iter_source(
    ctx: &mut FunctionContext<'_>,
    source: ValueId,
    source_kind: &IteratorSourceKind,
) -> Result<()> {
    let helper = match source_kind {
        IteratorSourceKind::Indexed { .. } => "__rt_array_ensure_unique",
        IteratorSourceKind::Hash => "__rt_hash_ensure_unique",
        _ => return Ok(()),
    };
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the foreach source pointer to the COW helper
    }
    abi::emit_call_label(ctx.emitter, helper);
    ctx.store_result_value(source)?;
    if let Some(slot) = source_load_local_slot(ctx, source)? {
        ctx.store_value_to_local(slot, source)?;
    }
    Ok(())
}

/// Stores a converted dynamic iterator source back to its originating local when possible.
fn store_iter_source_to_origin_if_local(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    source: ValueId,
) -> Result<()> {
    let Some(slot) = source_load_local_slot(ctx, source)? else {
        return Ok(());
    };
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::load_at_offset(ctx.emitter, "x0", offset - ITER_SOURCE_OFFSET_DELTA);
        }
        Arch::X86_64 => {
            abi::load_at_offset(ctx.emitter, "rax", offset - ITER_SOURCE_OFFSET_DELTA);
        }
    }
    ctx.store_result_value(source)?;
    ctx.store_value_to_local(slot, source)
}

/// Resolves a source SSA value back to a local slot when it was produced by `load_local`.
fn source_load_local_slot(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<Option<LocalSlotId>> {
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
    if inst_ref.op != Op::LoadLocal {
        return Ok(None);
    }
    let Some(Immediate::LocalSlot(slot)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(
            "load_local iterator source missing local slot",
        ));
    };
    Ok(Some(slot))
}

/// Converts the raw dynamic indexed-array iterator source to boxed Mixed slots.
fn convert_dynamic_indexed_source_for_ref(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::load_at_offset(ctx.emitter, "x0", offset - ITER_SOURCE_OFFSET_DELTA);
            ctx.emitter.instruction("ldr x1, [x0, #-8]");                       // load indexed-array metadata before by-reference Mixed conversion
            ctx.emitter.instruction("lsr x1, x1, #8");                          // move the runtime value_type tag into the low bits
            ctx.emitter.instruction("and x1, x1, #0x7f");                       // isolate the indexed-array value_type tag
            abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
            abi::store_at_offset(ctx.emitter, "x0", offset - ITER_SOURCE_OFFSET_DELTA);
        }
        Arch::X86_64 => {
            abi::load_at_offset(ctx.emitter, "rdi", offset - ITER_SOURCE_OFFSET_DELTA);
            ctx.emitter.instruction("mov rsi, QWORD PTR [rdi - 8]");            // load indexed-array metadata before by-reference Mixed conversion
            ctx.emitter.instruction("shr rsi, 8");                              // move the runtime value_type tag into the low bits
            ctx.emitter.instruction("and rsi, 0x7f");                           // isolate the indexed-array value_type tag
            abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
            abi::store_at_offset(ctx.emitter, "rax", offset - ITER_SOURCE_OFFSET_DELTA);
        }
    }
    Ok(())
}

/// Converts the raw dynamic hash iterator source to boxed Mixed entries.
fn convert_dynamic_hash_source_for_ref(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::load_at_offset(ctx.emitter, "x0", offset - ITER_SOURCE_OFFSET_DELTA);
            abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
            abi::store_at_offset(ctx.emitter, "x0", offset - ITER_SOURCE_OFFSET_DELTA);
        }
        Arch::X86_64 => {
            abi::load_at_offset(ctx.emitter, "rdi", offset - ITER_SOURCE_OFFSET_DELTA);
            abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
            abi::store_at_offset(ctx.emitter, "rax", offset - ITER_SOURCE_OFFSET_DELTA);
        }
    }
    Ok(())
}

/// Converts an unboxed Mixed indexed payload and updates the preserved Mixed source cell.
fn convert_mixed_indexed_source_for_ref(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // pass the unboxed indexed-array payload to the Mixed conversion helper
            ctx.emitter.instruction("ldr x1, [x0, #-8]");                       // load indexed-array metadata before by-reference Mixed conversion
            ctx.emitter.instruction("lsr x1, x1, #8");                          // move the runtime value_type tag into the low bits
            ctx.emitter.instruction("and x1, x1, #0x7f");                       // isolate the indexed-array value_type tag
            abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
            ctx.emitter.instruction("ldr x9, [sp]");                            // reload the preserved boxed Mixed source cell
            ctx.emitter.instruction("str x0, [x9, #8]");                        // publish the unique converted indexed-array pointer into the Mixed cell
            abi::store_at_offset(ctx.emitter, "x0", offset - ITER_SOURCE_OFFSET_DELTA);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, QWORD PTR [rdi - 8]");            // load indexed-array metadata before by-reference Mixed conversion
            ctx.emitter.instruction("shr rsi, 8");                              // move the runtime value_type tag into the low bits
            ctx.emitter.instruction("and rsi, 0x7f");                           // isolate the indexed-array value_type tag
            abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");                // reload the preserved boxed Mixed source cell
            ctx.emitter.instruction("mov QWORD PTR [r10 + 8], rax");            // publish the unique converted indexed-array pointer into the Mixed cell
            abi::store_at_offset(ctx.emitter, "rax", offset - ITER_SOURCE_OFFSET_DELTA);
        }
    }
    Ok(())
}

/// Converts an unboxed Mixed hash payload and updates the preserved Mixed source cell.
fn convert_mixed_hash_source_for_ref(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // pass the unboxed hash payload to the Mixed conversion helper
            abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
            ctx.emitter.instruction("ldr x9, [sp]");                            // reload the preserved boxed Mixed source cell
            ctx.emitter.instruction("str x0, [x9, #8]");                        // publish the unique converted hash pointer into the Mixed cell
            abi::store_at_offset(ctx.emitter, "x0", offset - ITER_SOURCE_OFFSET_DELTA);
        }
        Arch::X86_64 => {
            abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");                // reload the preserved boxed Mixed source cell
            ctx.emitter.instruction("mov QWORD PTR [r10 + 8], rax");            // publish the unique converted hash pointer into the Mixed cell
            abi::store_at_offset(ctx.emitter, "rax", offset - ITER_SOURCE_OFFSET_DELTA);
        }
    }
    Ok(())
}

/// Binds a local slot to the current indexed-array element address.
fn bind_indexed_current_value_ref(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    slot: LocalSlotId,
    elem_ty: &PhpType,
) -> Result<()> {
    let local_offset = ctx.local_offset(slot)?;
    let is_string_slot = matches!(elem_ty.codegen_repr(), PhpType::Str);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::load_at_offset_scratch(ctx.emitter, "x9", offset - ITER_SOURCE_OFFSET_DELTA, "x11");
            abi::load_at_offset_scratch(ctx.emitter, "x10", offset - ITER_CURSOR_OFFSET_DELTA, "x11");
            if is_string_slot {
                ctx.emitter.instruction("lsl x10, x10, #4");                    // scale the cursor to the 16-byte indexed string slot
            } else {
                ctx.emitter.instruction("lsl x10, x10, #3");                    // scale the cursor to the 8-byte indexed payload slot
            }
            ctx.emitter.instruction("add x9, x9, #24");                         // skip the indexed-array header to reach payload storage
            ctx.emitter.instruction("add x9, x9, x10");                         // compute the current indexed element address
            abi::store_at_offset_scratch(ctx.emitter, "x9", local_offset, "x11");
        }
        Arch::X86_64 => {
            abi::load_at_offset(ctx.emitter, "r11", offset - ITER_SOURCE_OFFSET_DELTA);
            abi::load_at_offset(ctx.emitter, "r10", offset - ITER_CURSOR_OFFSET_DELTA);
            if is_string_slot {
                ctx.emitter.instruction("shl r10, 4");                          // scale the cursor to the 16-byte indexed string slot
            } else {
                ctx.emitter.instruction("shl r10, 3");                          // scale the cursor to the 8-byte indexed payload slot
            }
            ctx.emitter.instruction("add r11, 24");                             // skip the indexed-array header to reach payload storage
            ctx.emitter.instruction("add r11, r10");                            // compute the current indexed element address
            abi::store_at_offset(ctx.emitter, "r11", local_offset);
        }
    }
    Ok(())
}

/// Binds a local slot to the current associative-array entry value address.
fn bind_hash_current_value_ref(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    slot: LocalSlotId,
) -> Result<()> {
    let local_offset = ctx.local_offset(slot)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::load_at_offset(ctx.emitter, "x9", offset - ITER_VALUE_ADDR_OFFSET_DELTA);
            abi::store_at_offset_scratch(ctx.emitter, "x9", local_offset, "x11");
        }
        Arch::X86_64 => {
            abi::load_at_offset(ctx.emitter, "r11", offset - ITER_VALUE_ADDR_OFFSET_DELTA);
            abi::store_at_offset(ctx.emitter, "r11", local_offset);
        }
    }
    Ok(())
}

/// Binds a local slot to the current value address after dynamic iterable dispatch.
fn bind_dynamic_current_value_ref(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    slot: LocalSlotId,
) -> Result<()> {
    let indexed_case = ctx.next_label("iter_ref_dyn_indexed");
    let hash_case = ctx.next_label("iter_ref_dyn_hash");
    let object_case = ctx.next_label("iter_ref_dyn_object");
    let done = ctx.next_label("iter_ref_dyn_done");
    branch_on_dynamic_source_heap_kind(ctx, offset, &indexed_case, &hash_case, &object_case);
    abi::emit_call_label(ctx.emitter, "__rt_iterable_unsupported_kind");

    ctx.emitter.label(&indexed_case);
    bind_indexed_current_value_ref(ctx, offset, slot, &PhpType::Mixed)?;
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&hash_case);
    bind_hash_current_value_ref(ctx, offset, slot)?;
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&object_case);
    abi::emit_call_label(ctx.emitter, "__rt_iterable_unsupported_kind");
    ctx.emitter.label(&done);
    Ok(())
}

/// Lowers dynamic iterator advancement by dispatching to the concrete heap layout.
fn lower_dynamic_iter_next(ctx: &mut FunctionContext<'_>, offset: usize) -> Result<()> {
    let indexed_case = ctx.next_label("iter_next_dyn_indexed");
    let hash_case = ctx.next_label("iter_next_dyn_hash");
    let object_case = ctx.next_label("iter_next_dyn_object");
    let done = ctx.next_label("iter_next_dyn_done");
    branch_on_dynamic_source_heap_kind(ctx, offset, &indexed_case, &hash_case, &object_case);
    abi::emit_call_label(ctx.emitter, "__rt_iterable_unsupported_kind");

    ctx.emitter.label(&indexed_case);
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_indexed_iter_next_aarch64(ctx, offset),
        Arch::X86_64 => lower_indexed_iter_next_x86_64(ctx, offset),
    }
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&hash_case);
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_hash_iter_next_aarch64(ctx, offset),
        Arch::X86_64 => lower_hash_iter_next_x86_64(ctx, offset),
    }
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&object_case);
    lower_interface_iter_next(ctx, offset, "Iterator")?;
    ctx.emitter.label(&done);
    Ok(())
}

/// Lowers dynamic iterator key loading by dispatching to the concrete heap layout.
fn lower_dynamic_iter_current_key(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    offset: usize,
) -> Result<()> {
    let indexed_case = ctx.next_label("iter_key_dyn_indexed");
    let hash_case = ctx.next_label("iter_key_dyn_hash");
    let object_case = ctx.next_label("iter_key_dyn_object");
    let done = ctx.next_label("iter_key_dyn_done");
    branch_on_dynamic_source_heap_kind(ctx, offset, &indexed_case, &hash_case, &object_case);
    abi::emit_call_label(ctx.emitter, "__rt_iterable_unsupported_kind");

    ctx.emitter.label(&indexed_case);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, result_reg, offset - ITER_CURSOR_OFFSET_DELTA);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&hash_case);
    match ctx.emitter.target.arch {
        Arch::AArch64 => load_current_hash_key_as_mixed_aarch64(ctx, offset),
        Arch::X86_64 => load_current_hash_key_as_mixed_x86_64(ctx, offset),
    }
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&object_case);
    let return_ty = emit_interface_iterator_method_call(ctx, offset, "Iterator", "key")?;
    box_iterator_method_result_if_needed(ctx, inst, &return_ty)?;
    ctx.emitter.label(&done);
    Ok(())
}

/// Lowers dynamic iterator value loading by dispatching to the concrete heap layout.
fn lower_dynamic_iter_current_value(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    offset: usize,
) -> Result<()> {
    let indexed_case = ctx.next_label("iter_value_dyn_indexed");
    let hash_case = ctx.next_label("iter_value_dyn_hash");
    let object_case = ctx.next_label("iter_value_dyn_object");
    let done = ctx.next_label("iter_value_dyn_done");
    branch_on_dynamic_source_heap_kind(ctx, offset, &indexed_case, &hash_case, &object_case);
    abi::emit_call_label(ctx.emitter, "__rt_iterable_unsupported_kind");

    ctx.emitter.label(&indexed_case);
    match ctx.emitter.target.arch {
        Arch::AArch64 => load_current_dynamic_indexed_value_as_mixed_aarch64(ctx, offset),
        Arch::X86_64 => load_current_dynamic_indexed_value_as_mixed_x86_64(ctx, offset),
    }
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&hash_case);
    match ctx.emitter.target.arch {
        Arch::AArch64 => load_current_hash_value_as_mixed_aarch64(ctx, offset),
        Arch::X86_64 => load_current_hash_value_as_mixed_x86_64(ctx, offset),
    }
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&object_case);
    let return_ty = emit_interface_iterator_method_call(ctx, offset, "Iterator", "current")?;
    box_iterator_method_result_if_needed(ctx, inst, &return_ty)?;
    ctx.emitter.label(&done);
    Ok(())
}

/// Replaces a dynamic object iterator source with `IteratorAggregate::getIterator()` when available.
fn resolve_dynamic_object_iterator_source(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
) -> Result<()> {
    if !ctx.module.interface_infos.contains_key("IteratorAggregate") {
        return Ok(());
    }
    let keep_original = ctx.next_label("iter_dynamic_keep_original_object");
    emit_interface_iterator_method_call(ctx, offset, "IteratorAggregate", "getIterator")?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz {}, {}", result_reg, keep_original)); // keep direct Iterator objects when IteratorAggregate dispatch misses
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {}, {}", result_reg, result_reg)); // keep direct Iterator objects when IteratorAggregate dispatch misses
            ctx.emitter.instruction(&format!("je {}", keep_original));          // skip source replacement when getIterator() was not resolved
        }
    }
    abi::store_at_offset(ctx.emitter, result_reg, offset - ITER_SOURCE_OFFSET_DELTA);
    ctx.emitter.label(&keep_original);
    Ok(())
}

/// Branches on the heap kind of the raw iterable source stored in iterator state.
fn branch_on_dynamic_source_heap_kind(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    indexed_case: &str,
    hash_case: &str,
    object_case: &str,
) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, result_reg, offset - ITER_SOURCE_OFFSET_DELTA);
    abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
    branch_on_heap_kind_result(ctx, indexed_case, hash_case, object_case);
}

/// Branches to the concrete iterator path from a `__rt_heap_kind` result.
fn branch_on_heap_kind_result(
    ctx: &mut FunctionContext<'_>,
    indexed_case: &str,
    hash_case: &str,
    object_case: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #2");                              // heap kind 2 identifies indexed arrays
            ctx.emitter.instruction(&format!("b.eq {}", indexed_case));         // dispatch to the indexed-array iterator path
            ctx.emitter.instruction("cmp x0, #3");                              // heap kind 3 identifies associative arrays
            ctx.emitter.instruction(&format!("b.eq {}", hash_case));            // dispatch to the associative-array iterator path
            ctx.emitter.instruction("cmp x0, #4");                              // heap kind 4 identifies object payloads
            ctx.emitter.instruction(&format!("b.eq {}", object_case));          // dispatch to the object Iterator protocol path
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 2");                              // heap kind 2 identifies indexed arrays
            ctx.emitter.instruction(&format!("je {}", indexed_case));           // dispatch to the indexed-array iterator path
            ctx.emitter.instruction("cmp rax, 3");                              // heap kind 3 identifies associative arrays
            ctx.emitter.instruction(&format!("je {}", hash_case));              // dispatch to the associative-array iterator path
            ctx.emitter.instruction("cmp rax, 4");                              // heap kind 4 identifies object payloads
            ctx.emitter.instruction(&format!("je {}", object_case));            // dispatch to the object Iterator protocol path
        }
    }
}

/// Branches to the concrete iterator path from a `__rt_mixed_unbox` tag result.
fn branch_on_mixed_iterable_tag(
    ctx: &mut FunctionContext<'_>,
    indexed_case: &str,
    hash_case: &str,
    object_case: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #4");                              // mixed tag 4 identifies indexed arrays
            ctx.emitter.instruction(&format!("b.eq {}", indexed_case));         // dispatch to the indexed-array iterator path
            ctx.emitter.instruction("cmp x0, #5");                              // mixed tag 5 identifies associative arrays
            ctx.emitter.instruction(&format!("b.eq {}", hash_case));            // dispatch to the associative-array iterator path
            ctx.emitter.instruction("cmp x0, #6");                              // mixed tag 6 identifies object payloads
            ctx.emitter.instruction(&format!("b.eq {}", object_case));          // dispatch to the object Iterator protocol path
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 4");                              // mixed tag 4 identifies indexed arrays
            ctx.emitter.instruction(&format!("je {}", indexed_case));           // dispatch to the indexed-array iterator path
            ctx.emitter.instruction("cmp rax, 5");                              // mixed tag 5 identifies associative arrays
            ctx.emitter.instruction(&format!("je {}", hash_case));              // dispatch to the associative-array iterator path
            ctx.emitter.instruction("cmp rax, 6");                              // mixed tag 6 identifies object payloads
            ctx.emitter.instruction(&format!("je {}", object_case));            // dispatch to the object Iterator protocol path
        }
    }
}

/// Stores the low payload produced by `__rt_mixed_unbox` as the raw iterator source.
fn store_mixed_payload_low_as_iterator_source(ctx: &mut FunctionContext<'_>, offset: usize) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::store_at_offset(ctx.emitter, "x1", offset - ITER_SOURCE_OFFSET_DELTA);
        }
        Arch::X86_64 => {
            abi::store_at_offset(ctx.emitter, "rdi", offset - ITER_SOURCE_OFFSET_DELTA);
        }
    }
}

/// Stores an iterator cursor value into the stack-resident iterator state.
fn store_iterator_cursor(ctx: &mut FunctionContext<'_>, offset: usize, cursor: i64) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, cursor);
    abi::store_at_offset(ctx.emitter, result_reg, offset - ITER_CURSOR_OFFSET_DELTA);
}

/// Boxes a concrete iterator method result when the EIR result slot expects `Mixed`.
fn box_iterator_method_result_if_needed(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    return_ty: &PhpType,
) -> Result<()> {
    let Some(result) = inst.result else {
        return Ok(());
    };
    let result_ty = ctx.value_php_type(result)?;
    if result_ty == PhpType::Mixed && return_ty.codegen_repr() != PhpType::Mixed {
        emit_box_current_value_as_mixed(ctx.emitter, &return_ty.codegen_repr());
    }
    Ok(())
}

/// Lowers object iterator advancement using PHP's Iterator method protocol.
fn lower_object_iter_next(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    class_name: &str,
) -> Result<()> {
    let first_label = ctx.next_label("object_iter_first");
    let valid_label = ctx.next_label("object_iter_valid");
    let started_reg = abi::int_result_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, started_reg, offset - ITER_CURSOR_OFFSET_DELTA);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #0", started_reg));       // check whether this object iterator has already yielded once
            ctx.emitter.instruction(&format!("b.eq {}", first_label));          // skip next() before the first valid() probe
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {}, {}", started_reg, started_reg)); // check whether this object iterator has already yielded once
            ctx.emitter.instruction(&format!("je {}", first_label));            // skip next() before the first valid() probe
        }
    }
    emit_object_iterator_method_call(ctx, offset, class_name, "next")?;
    abi::emit_jump(ctx.emitter, &valid_label);
    ctx.emitter.label(&first_label);
    abi::emit_load_int_immediate(ctx.emitter, started_reg, 1);
    abi::store_at_offset(ctx.emitter, started_reg, offset - ITER_CURSOR_OFFSET_DELTA);
    ctx.emitter.label(&valid_label);
    emit_object_iterator_method_call(ctx, offset, class_name, "valid")?;
    Ok(())
}

/// Lowers iterator advancement through an `Iterator`-typed interface receiver.
fn lower_interface_iter_next(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    interface_name: &str,
) -> Result<()> {
    let first_label = ctx.next_label("interface_iter_first");
    let valid_label = ctx.next_label("interface_iter_valid");
    let started_reg = abi::int_result_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, started_reg, offset - ITER_CURSOR_OFFSET_DELTA);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #0", started_reg));       // check whether this interface iterator has already yielded once
            ctx.emitter.instruction(&format!("b.eq {}", first_label));          // skip next() before the first valid() probe
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {}, {}", started_reg, started_reg)); // check whether this interface iterator has already yielded once
            ctx.emitter.instruction(&format!("je {}", first_label));            // skip next() before the first valid() probe
        }
    }
    emit_interface_iterator_method_call(ctx, offset, interface_name, "next")?;
    abi::emit_jump(ctx.emitter, &valid_label);
    ctx.emitter.label(&first_label);
    abi::emit_load_int_immediate(ctx.emitter, started_reg, 1);
    abi::store_at_offset(ctx.emitter, started_reg, offset - ITER_CURSOR_OFFSET_DELTA);
    ctx.emitter.label(&valid_label);
    emit_interface_iterator_method_call(ctx, offset, interface_name, "valid")?;
    Ok(())
}

/// Emits a zero-argument Iterator method call against the object stored in iterator state.
fn emit_object_iterator_method_call(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    class_name: &str,
    method_name: &str,
) -> Result<PhpType> {
    let method_key = php_symbol_key(method_name);
    if let Some(helper) = generator_iterator_runtime_helper(class_name, &method_key) {
        emit_generator_iterator_runtime_call(ctx, offset, helper);
        return Ok(generator_iterator_return_type(&method_key));
    }
    let target = object_iterator_method_target(ctx, class_name, &method_key)?;
    let assignments = abi::build_outgoing_arg_assignments_for_target(
        ctx.emitter.target,
        &[PhpType::Object(class_name.to_string())],
        0,
    );
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, result_reg, offset - ITER_SOURCE_OFFSET_DELTA);
    abi::emit_push_result_value(ctx.emitter, &PhpType::Object(class_name.to_string()));
    let overflow_bytes = abi::materialize_outgoing_args(ctx.emitter, &assignments);
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    if let Some(helper) = target.runtime_helper {
        abi::emit_call_label(ctx.emitter, helper);
    } else {
        abi::emit_call_label(ctx.emitter, &method_symbol(&target.impl_class, &method_key));
    }
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, overflow_bytes);
    Ok(target.return_type)
}

/// Emits a zero-argument Iterator method call through runtime interface metadata.
fn emit_interface_iterator_method_call(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    interface_name: &str,
    method_name: &str,
) -> Result<PhpType> {
    let receiver_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::load_at_offset(ctx.emitter, receiver_arg, offset - ITER_SOURCE_OFFSET_DELTA);
    let method_key = php_symbol_key(method_name);
    let done = if interface_name.trim_start_matches('\\') == "Iterator" {
        IntrinsicCall::instance_method("Generator", &method_key)
            .and_then(|intrinsic| intrinsic.runtime_helper())
            .map(|helper| emit_generator_interface_fast_path(ctx, helper))
    } else {
        None
    };
    let return_ty = emit_interface_dispatch_call(ctx, interface_name, &method_key, done.as_deref())?;
    if let Some(done) = done {
        ctx.emitter.label(&done);
    }
    Ok(return_ty)
}

/// Emits a fast path for Generator objects before generic `Iterator` interface dispatch.
fn emit_generator_interface_fast_path(
    ctx: &mut FunctionContext<'_>,
    helper: &str,
) -> String {
    let done = ctx.next_label("interface_dispatch_done");
    let not_generator = ctx.next_label("interface_dispatch_not_generator");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x10, [x0]");                           // load receiver class id before checking for the built-in Generator
            abi::emit_load_symbol_to_reg(ctx.emitter, "x11", "_generator_class_id", 0);
            ctx.emitter.instruction("cmp x10, x11");                            // compare receiver class id with Generator
            ctx.emitter.instruction(&format!("b.ne {}", not_generator));        // fall back to interface dispatch for non-Generator iterators
            abi::emit_call_label(ctx.emitter, helper);
            ctx.emitter.instruction(&format!("b {}", done));                    // skip generic interface dispatch after the fast path
            ctx.emitter.label(&not_generator);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, QWORD PTR [rdi]");                // load receiver class id before checking for the built-in Generator
            abi::emit_load_symbol_to_reg(ctx.emitter, "r11", "_generator_class_id", 0);
            ctx.emitter.instruction("cmp r10, r11");                            // compare receiver class id with Generator
            ctx.emitter.instruction(&format!("jne {}", not_generator));         // fall back to interface dispatch for non-Generator iterators
            abi::emit_call_label(ctx.emitter, helper);
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip generic interface dispatch after the fast path
            ctx.emitter.label(&not_generator);
        }
    }
    done
}

/// Emits the interface table scan and calls the resolved method slot.
pub(super) fn emit_interface_dispatch_call(
    ctx: &mut FunctionContext<'_>,
    interface_name: &str,
    method_key: &str,
    external_done: Option<&str>,
) -> Result<PhpType> {
    let normalized = interface_name.trim_start_matches('\\');
    let interface_info = ctx
        .module
        .interface_infos
        .get(normalized)
        .ok_or_else(|| CodegenIrError::unsupported(format!("iterator interface {}", normalized)))?;
    let interface_id = interface_info.interface_id as i64;
    let slot = interface_info.method_slots.get(method_key).copied().ok_or_else(|| {
        CodegenIrError::unsupported(format!("iterator interface method {}::{}", normalized, method_key))
    })?;
    let return_ty = interface_info
        .methods
        .get(method_key)
        .map(|sig| sig.return_type.clone())
        .unwrap_or(PhpType::Mixed);
    let scan_loop = ctx.next_label("interface_dispatch_scan");
    let found = ctx.next_label("interface_dispatch_found");
    let missing = ctx.next_label("interface_dispatch_missing");
    let local_done = external_done
        .map(str::to_string)
        .unwrap_or_else(|| ctx.next_label("interface_dispatch_done"));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x10, [x0]");                           // load receiver class id for interface metadata lookup
            abi::emit_symbol_address(ctx.emitter, "x11", "_class_interface_ptrs");
            ctx.emitter.instruction("ldr x11, [x11, x10, lsl #3]");             // select this class's interface metadata block
            ctx.emitter.instruction("ldr x10, [x11]");                          // load implemented interface count
            ctx.emitter.instruction("add x11, x11, #8");                        // move to first [interface id, table] pair
            abi::emit_load_int_immediate(ctx.emitter, "x13", interface_id);
            ctx.emitter.label(&scan_loop);
            ctx.emitter.instruction(&format!("cbz x10, {}", missing));          // stop when no implemented interface matched
            ctx.emitter.instruction("ldr x12, [x11]");                          // load current implemented interface id
            ctx.emitter.instruction("cmp x12, x13");                            // compare with target interface id
            ctx.emitter.instruction(&format!("b.eq {}", found));                // dispatch through this table when matched
            ctx.emitter.instruction("add x11, x11, #16");                       // advance to next interface metadata entry
            ctx.emitter.instruction("sub x10, x10, #1");                        // consume one interface entry
            ctx.emitter.instruction(&format!("b {}", scan_loop));               // continue scanning implemented interfaces
            ctx.emitter.label(&found);
            ctx.emitter.instruction("ldr x11, [x11, #8]");                      // load implementation table pointer
            if slot == 0 {
                ctx.emitter.instruction("ldr x11, [x11]");                      // load first interface method implementation pointer
            } else {
                ctx.emitter.instruction(&format!("ldr x11, [x11, #{}]", slot * 8)); // load selected interface method implementation pointer
            }
            ctx.emitter.instruction("blr x11");                                 // call resolved interface method implementation
            ctx.emitter.instruction(&format!("b {}", local_done));              // skip defensive missing-interface fallback
            ctx.emitter.label(&missing);
            ctx.emitter.instruction("mov x0, #0");                              // defensive fallback for invalid interface metadata
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, QWORD PTR [rdi]");                // load receiver class id for interface metadata lookup
            abi::emit_symbol_address(ctx.emitter, "r11", "_class_interface_ptrs");
            ctx.emitter.instruction("mov r11, QWORD PTR [r11 + r10 * 8]");      // select this class's interface metadata block
            ctx.emitter.instruction("mov r10, QWORD PTR [r11]");                // load implemented interface count
            ctx.emitter.instruction("add r11, 8");                              // move to first [interface id, table] pair
            abi::emit_load_int_immediate(ctx.emitter, "r9", interface_id);
            ctx.emitter.label(&scan_loop);
            ctx.emitter.instruction("test r10, r10");                           // check whether implemented interfaces remain
            ctx.emitter.instruction(&format!("je {}", missing));                // stop when no implemented interface matched
            ctx.emitter.instruction("mov r8, QWORD PTR [r11]");                 // load current implemented interface id
            ctx.emitter.instruction("cmp r8, r9");                              // compare with target interface id
            ctx.emitter.instruction(&format!("je {}", found));                  // dispatch through this table when matched
            ctx.emitter.instruction("add r11, 16");                             // advance to next interface metadata entry
            ctx.emitter.instruction("sub r10, 1");                              // consume one interface entry
            ctx.emitter.instruction(&format!("jmp {}", scan_loop));             // continue scanning implemented interfaces
            ctx.emitter.label(&found);
            ctx.emitter.instruction("mov r11, QWORD PTR [r11 + 8]");            // load implementation table pointer
            if slot == 0 {
                ctx.emitter.instruction("mov r11, QWORD PTR [r11]");            // load first interface method implementation pointer
            } else {
                ctx.emitter.instruction(&format!("mov r11, QWORD PTR [r11 + {}]", slot * 8)); // load selected interface method implementation pointer
            }
            ctx.emitter.instruction("call r11");                                // call resolved interface method implementation
            ctx.emitter.instruction(&format!("jmp {}", local_done));            // skip defensive missing-interface fallback
            ctx.emitter.label(&missing);
            ctx.emitter.instruction("xor eax, eax");                            // defensive fallback for invalid interface metadata
        }
    }
    if external_done.is_none() {
        ctx.emitter.label(&local_done);
    }
    Ok(return_ty)
}

/// Returns the PHP type produced by a `Generator` iterator runtime helper.
fn generator_iterator_return_type(method_key: &str) -> PhpType {
    match method_key {
        "valid" => PhpType::Bool,
        "rewind" | "next" => PhpType::Void,
        _ => PhpType::Mixed,
    }
}

/// Returns the runtime helper for `Generator` methods used by object iterator lowering.
fn generator_iterator_runtime_helper(class_name: &str, method_key: &str) -> Option<&'static str> {
    if class_name.trim_start_matches('\\') != "Generator" {
        return None;
    }
    match method_key {
        "rewind" => Some("__rt_gen_rewind"),
        "current" => Some("__rt_gen_current"),
        "key" => Some("__rt_gen_key"),
        "next" => Some("__rt_gen_next"),
        "valid" => Some("__rt_gen_valid"),
        _ => None,
    }
}

/// Emits a direct `Generator` runtime call for foreach iterator protocol methods.
fn emit_generator_iterator_runtime_call(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
    helper: &str,
) {
    let receiver_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::load_at_offset(ctx.emitter, receiver_arg, offset - ITER_SOURCE_OFFSET_DELTA);
    abi::emit_call_label(ctx.emitter, helper);
}

/// Resolves the concrete implementation class for an object iterator method.
fn object_iterator_method_target(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method_key: &str,
) -> Result<ObjectIteratorMethodTarget> {
    let normalized = class_name.trim_start_matches('\\');
    let class_info = ctx
        .module
        .class_infos
        .get(normalized)
        .ok_or_else(|| CodegenIrError::unsupported(format!("iterator object class {}", normalized)))?;
    let callee_sig = class_info
        .methods
        .get(method_key)
        .ok_or_else(|| CodegenIrError::unsupported(format!("iterator method {}::{}", normalized, method_key)))?;
    if !callee_sig.params.is_empty() {
        return Err(CodegenIrError::unsupported(format!(
            "iterator method {}::{} with {} params",
            normalized,
            method_key,
            callee_sig.params.len()
        )));
    }
    let impl_class = class_info
        .method_impl_classes
        .get(method_key)
        .cloned()
        .unwrap_or_else(|| normalized.to_string());
    let runtime_helper =
        IntrinsicCall::instance_method(&impl_class, method_key).and_then(|intrinsic| intrinsic.runtime_helper());
    if runtime_helper.is_none() && !class_method_body_exists(ctx, &impl_class, method_key) {
        return Err(CodegenIrError::unsupported(format!(
            "iterator method {}::{} without an emitted EIR method body",
            impl_class, method_key
        )));
    }
    Ok(ObjectIteratorMethodTarget {
        impl_class,
        runtime_helper,
        return_type: callee_sig.return_type.clone(),
    })
}

/// Resolved method implementation for an object iterator method call.
struct ObjectIteratorMethodTarget {
    impl_class: String,
    runtime_helper: Option<&'static str>,
    return_type: PhpType,
}

/// Returns true when the EIR module contains the concrete instance-method body.
fn class_method_body_exists(ctx: &FunctionContext<'_>, class_name: &str, method_key: &str) -> bool {
    ctx.module.class_methods.iter().any(|function| {
        !function.flags.is_static
            && function
                .name
                .rsplit_once("::")
                .is_some_and(|(candidate_class, candidate_method)| {
                    candidate_class == class_name && php_symbol_key(candidate_method) == method_key
                })
    })
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
    abi::store_at_offset(ctx.emitter, "x6", offset - ITER_VALUE_ADDR_OFFSET_DELTA);
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
    abi::store_at_offset(ctx.emitter, "r10", offset - ITER_VALUE_ADDR_OFFSET_DELTA);
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
    box_hash_payload_as_mixed_aarch64(ctx);
}

/// Boxes the current x86_64 hash value payload saved by `IterNext` into `Mixed`.
fn load_current_hash_value_as_mixed_x86_64(ctx: &mut FunctionContext<'_>, offset: usize) {
    abi::load_at_offset(ctx.emitter, "r9", offset - ITER_VALUE_TAG_OFFSET_DELTA);
    abi::load_at_offset(ctx.emitter, "rcx", offset - ITER_VALUE_LO_OFFSET_DELTA);
    abi::load_at_offset(ctx.emitter, "r8", offset - ITER_VALUE_HI_OFFSET_DELTA);
    box_hash_payload_as_mixed_x86_64(ctx);
}

/// Boxes or retains an AArch64 hash payload as an owned `Mixed` value.
fn box_hash_payload_as_mixed_aarch64(ctx: &mut FunctionContext<'_>) {
    let inspect_tagged_box = ctx.next_label("iter_hash_value_inspect_box");
    let done = ctx.next_label("iter_hash_value_boxed");
    ctx.emitter.instruction("cmp x5, #7");                                      // does the hash entry use the Mixed-or-iterable runtime tag?
    ctx.emitter.instruction(&format!("b.eq {}", inspect_tagged_box));           // inspect tag-7 payloads because iterable hashes also use that tag
    emit_box_runtime_payload_as_mixed(ctx.emitter, "x5", "x3", "x4");
    ctx.emitter.instruction(&format!("b {}", done));                            // skip tag-7 inspection after boxing a concrete payload
    ctx.emitter.label(&inspect_tagged_box);
    box_tagged_hash_payload_as_mixed_aarch64(ctx);
    ctx.emitter.label(&done);
}

/// Boxes or retains an AArch64 tag-7 hash payload as Mixed after checking its heap kind.
fn box_tagged_hash_payload_as_mixed_aarch64(ctx: &mut FunctionContext<'_>) {
    let reuse_box = ctx.next_label("iter_hash_value_reuse_box");
    let done = ctx.next_label("iter_hash_value_tagged_done");
    ctx.emitter.instruction("str x3, [sp, #-16]!");                             // preserve the tag-7 payload while probing its heap kind
    ctx.emitter.instruction("mov x0, x3");                                      // pass the tag-7 payload to the heap-kind probe
    abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
    ctx.emitter.instruction("cmp x0, #5");                                      // heap kind 5 means the payload is already a boxed Mixed cell
    ctx.emitter.instruction(&format!("b.eq {}", reuse_box));                    // retain existing Mixed boxes instead of nesting them
    ctx.emitter.instruction("ldr x0, [sp], #16");                               // restore the raw iterable payload before boxing it as Mixed
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Iterable);
    ctx.emitter.instruction(&format!("b {}", done));                            // skip the existing Mixed retention path
    ctx.emitter.label(&reuse_box);
    ctx.emitter.instruction("ldr x0, [sp], #16");                               // restore the existing Mixed box before retaining it
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    ctx.emitter.label(&done);
}

/// Boxes or retains an x86_64 hash payload as an owned `Mixed` value.
fn box_hash_payload_as_mixed_x86_64(ctx: &mut FunctionContext<'_>) {
    let inspect_tagged_box = ctx.next_label("iter_hash_value_inspect_box");
    let done = ctx.next_label("iter_hash_value_boxed");
    ctx.emitter.instruction("cmp r9, 7");                                       // does the hash entry use the Mixed-or-iterable runtime tag?
    ctx.emitter.instruction(&format!("je {}", inspect_tagged_box));             // inspect tag-7 payloads because iterable hashes also use that tag
    emit_box_runtime_payload_as_mixed(ctx.emitter, "r9", "rcx", "r8");
    ctx.emitter.instruction(&format!("jmp {}", done));                          // skip tag-7 inspection after boxing a concrete payload
    ctx.emitter.label(&inspect_tagged_box);
    box_tagged_hash_payload_as_mixed_x86_64(ctx);
    ctx.emitter.label(&done);
}

/// Boxes or retains an x86_64 tag-7 hash payload as Mixed after checking its heap kind.
fn box_tagged_hash_payload_as_mixed_x86_64(ctx: &mut FunctionContext<'_>) {
    let reuse_box = ctx.next_label("iter_hash_value_reuse_box");
    let done = ctx.next_label("iter_hash_value_tagged_done");
    abi::emit_push_reg(ctx.emitter, "rcx");
    ctx.emitter.instruction("mov rax, rcx");                                    // pass the tag-7 payload to the heap-kind probe
    abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
    ctx.emitter.instruction("cmp rax, 5");                                      // heap kind 5 means the payload is already a boxed Mixed cell
    ctx.emitter.instruction(&format!("je {}", reuse_box));                      // retain existing Mixed boxes instead of nesting them
    abi::emit_pop_reg(ctx.emitter, "rax");
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Iterable);
    ctx.emitter.instruction(&format!("jmp {}", done));                          // skip the existing Mixed retention path
    ctx.emitter.label(&reuse_box);
    abi::emit_pop_reg(ctx.emitter, "rax");
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    ctx.emitter.label(&done);
}

/// Loads a runtime-typed AArch64 indexed-array element and returns it as an owned `Mixed` box.
fn load_current_dynamic_indexed_value_as_mixed_aarch64(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
) {
    let string_case = ctx.next_label("iter_dynamic_indexed_string");
    let loaded = ctx.next_label("iter_dynamic_indexed_loaded");
    let reuse_box = ctx.next_label("iter_dynamic_indexed_reuse_box");
    let done = ctx.next_label("iter_dynamic_indexed_done");
    abi::load_at_offset_scratch(ctx.emitter, "x11", offset - ITER_SOURCE_OFFSET_DELTA, "x9");
    abi::load_at_offset(ctx.emitter, "x0", offset - ITER_CURSOR_OFFSET_DELTA);
    ctx.emitter.instruction("ldr x5, [x11, #-8]");                              // load the packed indexed-array heap metadata
    ctx.emitter.instruction("lsr x5, x5, #8");                                  // move the runtime value_type tag into the low bits
    ctx.emitter.instruction("and x5, x5, #0x7f");                               // isolate the indexed-array value_type tag
    ctx.emitter.instruction("cmp x5, #1");                                      // does this indexed array store string slots?
    ctx.emitter.instruction(&format!("b.eq {}", string_case));                  // branch to the 16-byte string-slot loader
    ctx.emitter.instruction("add x11, x11, #24");                               // skip the indexed-array header to the 8-byte payload slots
    ctx.emitter.instruction("ldr x3, [x11, x0, lsl #3]");                       // load the scalar or pointer payload from the selected indexed slot
    ctx.emitter.instruction("mov x4, xzr");                                     // non-string indexed payloads have no high payload word
    ctx.emitter.instruction(&format!("b {}", loaded));                          // continue with a normalized runtime payload triple

    ctx.emitter.label(&string_case);
    ctx.emitter.instruction("lsl x10, x0, #4");                                 // scale the index by the 16-byte string slot size
    ctx.emitter.instruction("add x11, x11, x10");                               // move to the selected string slot
    ctx.emitter.instruction("add x11, x11, #24");                               // skip the indexed-array header before loading the slot
    ctx.emitter.instruction("ldr x3, [x11]");                                   // load the string pointer payload
    ctx.emitter.instruction("ldr x4, [x11, #8]");                               // load the string length payload

    ctx.emitter.label(&loaded);
    ctx.emitter.instruction("cmp x5, #7");                                      // does the slot already hold a boxed Mixed value?
    ctx.emitter.instruction(&format!("b.eq {}", reuse_box));                    // retain existing Mixed boxes instead of nesting them
    emit_box_runtime_payload_as_mixed(ctx.emitter, "x5", "x3", "x4");
    ctx.emitter.instruction(&format!("b {}", done));                            // skip the existing-box retention path
    ctx.emitter.label(&reuse_box);
    ctx.emitter.instruction("mov x0, x3");                                      // pass the existing Mixed box to the retain helper
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    ctx.emitter.label(&done);
}

/// Loads a runtime-typed x86_64 indexed-array element and returns it as an owned `Mixed` box.
fn load_current_dynamic_indexed_value_as_mixed_x86_64(
    ctx: &mut FunctionContext<'_>,
    offset: usize,
) {
    let string_case = ctx.next_label("iter_dynamic_indexed_string");
    let loaded = ctx.next_label("iter_dynamic_indexed_loaded");
    let reuse_box = ctx.next_label("iter_dynamic_indexed_reuse_box");
    let done = ctx.next_label("iter_dynamic_indexed_done");
    abi::load_at_offset(ctx.emitter, "r11", offset - ITER_SOURCE_OFFSET_DELTA);
    abi::load_at_offset(ctx.emitter, "r10", offset - ITER_CURSOR_OFFSET_DELTA);
    ctx.emitter.instruction("mov r9, QWORD PTR [r11 - 8]");                     // load the packed indexed-array heap metadata
    ctx.emitter.instruction("shr r9, 8");                                       // move the runtime value_type tag into the low bits
    ctx.emitter.instruction("and r9, 0x7f");                                    // isolate the indexed-array value_type tag
    ctx.emitter.instruction("cmp r9, 1");                                       // does this indexed array store string slots?
    ctx.emitter.instruction(&format!("je {}", string_case));                    // branch to the 16-byte string-slot loader
    ctx.emitter.instruction("add r11, 24");                                     // skip the indexed-array header to the 8-byte payload slots
    ctx.emitter.instruction("mov rcx, QWORD PTR [r11 + r10 * 8]");              // load the scalar or pointer payload from the selected indexed slot
    ctx.emitter.instruction("xor r8, r8");                                      // non-string indexed payloads have no high payload word
    ctx.emitter.instruction(&format!("jmp {}", loaded));                        // continue with a normalized runtime payload triple

    ctx.emitter.label(&string_case);
    ctx.emitter.instruction("shl r10, 4");                                      // scale the index by the 16-byte string slot size
    ctx.emitter.instruction("add r11, r10");                                    // move to the selected string slot
    ctx.emitter.instruction("add r11, 24");                                     // skip the indexed-array header before loading the slot
    ctx.emitter.instruction("mov rcx, QWORD PTR [r11]");                        // load the string pointer payload
    ctx.emitter.instruction("mov r8, QWORD PTR [r11 + 8]");                     // load the string length payload

    ctx.emitter.label(&loaded);
    ctx.emitter.instruction("cmp r9, 7");                                       // does the slot already hold a boxed Mixed value?
    ctx.emitter.instruction(&format!("je {}", reuse_box));                      // retain existing Mixed boxes instead of nesting them
    emit_box_runtime_payload_as_mixed(ctx.emitter, "r9", "rcx", "r8");
    ctx.emitter.instruction(&format!("jmp {}", done));                          // skip the existing-box retention path
    ctx.emitter.label(&reuse_box);
    ctx.emitter.instruction("mov rax, rcx");                                    // pass the existing Mixed box to the retain helper
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    ctx.emitter.label(&done);
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
    iterator_source_kind_from_type(ctx, &iterator_source_type(ctx, iterator, inst)?, inst)
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
fn iterator_source_kind_from_type(
    ctx: &FunctionContext<'_>,
    ty: &PhpType,
    inst: &Instruction,
) -> Result<IteratorSourceKind> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => {
            let elem_repr = elem.codegen_repr();
            // A boxed-Mixed/Union-element indexed array may be runtime-promoted
            // to associative hash storage by `Op::ArraySetMixedKey` (notably a
            // `foreach($src as $k=>$v) $dst[$k]=$v` rebuild that writes string
            // keys), so its iteration must dispatch on the runtime heap kind
            // instead of assuming indexed storage. The dynamic indexed value
            // loader reuses existing Mixed boxes (value_type 7) via incref, so a
            // genuinely indexed Mixed-element array iterates identically to the
            // static indexed path; only runtime-promoted hashes route to hash
            // iteration. Concrete-element indexed arrays (int/string/etc.) can
            // never be promoted by `Op::ArraySetMixedKey`, so they keep the
            // faster static indexed path with no extra heap-kind check.
            if matches!(elem_repr, PhpType::Mixed | PhpType::Union(_)) {
                Ok(IteratorSourceKind::DynamicIterable)
            } else {
                Ok(IteratorSourceKind::Indexed { elem: elem_repr })
            }
        }
        PhpType::AssocArray { .. } => Ok(IteratorSourceKind::Hash),
        PhpType::Iterable => Ok(IteratorSourceKind::DynamicIterable),
        PhpType::Mixed | PhpType::Union(_) => Ok(IteratorSourceKind::DynamicMixed),
        PhpType::Object(class_name) => {
            let source = object_iterator_source(ctx, class_name.trim_start_matches('\\'));
            Ok(source)
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} over PHP type {:?}",
            inst.op.name(),
            other
        ))),
    }
}

/// Returns the effective iterator dispatch target for an object source.
fn object_iterator_source(
    ctx: &FunctionContext<'_>,
    class_name: &str,
) -> IteratorSourceKind {
    if ctx.module.interface_infos.contains_key(class_name) {
        return IteratorSourceKind::Interface {
            interface_name: class_name.to_string(),
            aggregate_class_name: None,
        };
    }
    if class_implements_interface(ctx, class_name, "Iterator") {
        return IteratorSourceKind::Object {
            class_name: class_name.to_string(),
            aggregate_class_name: None,
        };
    }
    if !class_implements_interface(ctx, class_name, "IteratorAggregate") {
        return IteratorSourceKind::Object {
            class_name: class_name.to_string(),
            aggregate_class_name: None,
        };
    }
    match iterator_method_return_type(ctx, class_name, "getIterator") {
        PhpType::Object(iterator_class) => {
            let iterator_class = iterator_class.trim_start_matches('\\').to_string();
            if ctx.module.interface_infos.contains_key(&iterator_class) {
                IteratorSourceKind::Interface {
                    interface_name: iterator_class,
                    aggregate_class_name: Some(class_name.to_string()),
                }
            } else {
                IteratorSourceKind::Object {
                    class_name: iterator_class,
                    aggregate_class_name: Some(class_name.to_string()),
                }
            }
        }
        _ => IteratorSourceKind::Object {
            class_name: class_name.to_string(),
            aggregate_class_name: None,
        },
    }
}

/// Returns the declared return type for a no-arg iterator protocol method.
fn iterator_method_return_type(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method_name: &str,
) -> PhpType {
    let method_key = php_symbol_key(method_name);
    ctx.module
        .class_infos
        .get(class_name)
        .and_then(|class_info| class_info.methods.get(&method_key))
        .map(|sig| sig.return_type.clone())
        .unwrap_or(PhpType::Mixed)
}

/// Returns true when a class implements or inherits an implementation of an interface.
fn class_implements_interface(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    interface_name: &str,
) -> bool {
    let Some(class_info) = ctx.module.class_infos.get(class_name) else {
        return false;
    };
    class_info.interfaces.iter().any(|implemented| {
        normalized_type_name(implemented) == interface_name
            || interface_extends_interface(ctx, normalized_type_name(implemented), interface_name)
    })
}

/// Returns true when an interface extends the requested ancestor interface.
fn interface_extends_interface(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
    ancestor_name: &str,
) -> bool {
    if interface_name == ancestor_name {
        return true;
    }
    let Some(interface_info) = ctx.module.interface_infos.get(interface_name) else {
        return false;
    };
    interface_info.parents.iter().any(|parent| {
        normalized_type_name(parent) == ancestor_name
            || interface_extends_interface(ctx, normalized_type_name(parent), ancestor_name)
    })
}

/// Returns a class-like name without PHP's optional leading namespace separator.
fn normalized_type_name(name: &str) -> &str {
    name.trim_start_matches('\\')
}
