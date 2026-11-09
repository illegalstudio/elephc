//! Purpose:
//! Lowers globals, extern globals, constants, Mixed boxing, and invoker ref markers.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// Lowers a global storage load into the result register and SSA destination slot.
pub(super) fn lower_load_global(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let data = expect_global_name(inst)?;
    let name = ctx.global_name_data(data)?;
    let symbol = ir_global_symbol(name);
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("load_global missing result value"))?;
    let ty = ctx.value_php_type(result)?;
    ctx.data
        .add_comm(symbol.clone(), ty.codegen_repr().stack_size().max(8));
    abi::emit_load_symbol_to_result(ctx.emitter, &symbol, &ty);
    store_if_result(ctx, inst)
}

/// Lowers a global storage store from one SSA operand.
pub(super) fn lower_store_global(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let data = expect_global_name(inst)?;
    let name = ctx.global_name_data(data)?.to_string();
    let symbol = ir_global_symbol(&name);
    let value = expect_operand(inst, 0)?;
    let ty = ctx.load_value_to_result(value)?;
    let store_ty = if ctx.module.web && crate::superglobals::is_superglobal(&name) {
        ty.codegen_repr()
    } else {
        let source_ty = ty.codegen_repr();
        if source_ty != PhpType::Mixed {
            // The declared type for a `?ptr<T>`, so address 0 boxes as PHP null.
            let box_ty = ctx.mixed_box_type(value)?;
            if ctx.value_can_transfer_ownership_to_consumer(value)?
                || global_store_owns_persisted_string(ctx, value, &source_ty)
            {
                emit_box_current_owned_value_as_mixed(ctx.emitter, &box_ty);
            } else {
                emit_box_current_value_as_mixed(ctx.emitter, &box_ty);
            }
        }
        PhpType::Mixed
    };
    ctx.data
        .add_comm(symbol.clone(), store_ty.codegen_repr().stack_size().max(8));
    abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &store_ty, true);
    Ok(())
}

/// Recognizes an owned string whose sole payload owner must move into the global box.
fn global_store_owns_persisted_string(ctx: &FunctionContext<'_>, value: ValueId, ty: &PhpType) -> bool {
    if ty != &PhpType::Str { return false; }
    let Some(value_ref) = ctx.function.value(value) else { return false; };
    let ValueDef::Instruction { inst, .. } = value_ref.def else { return false; };
    let Some(producer) = ctx.function.instruction(inst) else { return false; };
    (matches!(producer.op, Op::Acquire | Op::StrPersist)
        || (producer.op == Op::RuntimeCall
            && producer.immediate == Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::ExecutablePath))))
        && !ctx.function.instructions.iter().any(|inst| {
            inst.op == Op::Release && inst.operands.first().copied() == Some(value)
        })
}

/// Length of PHP's undefined-global warning prefix emitted by dynamic `$GLOBALS` reads.
const UNDEFINED_GLOBAL_PREFIX_LEN: usize = "Warning: Undefined global variable $".len();

/// Lowers the bare `$GLOBALS` materialization into the runtime hash builder.
pub(super) fn lower_load_globals_array(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    abi::emit_call_label(ctx.emitter, "__rt_globals_materialize");
    store_if_result(ctx, inst)
}

/// Lowers a `$GLOBALS[$name]` read through the program's runtime global-name table.
///
/// The lookup returns the address of the global's Mixed storage word, or zero when the name
/// was never a program global. A miss boxes PHP null and, for ordinary reads, emits PHP's
/// undefined-global warning; silent `isset()`/`??` probes set the immediate Bool to false.
pub(super) fn lower_load_global_dynamic(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let name = expect_operand(inst, 0)?;
    let warn_on_missing = matches!(inst.immediate, Some(Immediate::Bool(true)));
    let miss = ctx.next_label("dyn_global_load_miss");
    let done = ctx.next_label("dyn_global_load_done");
    abi::emit_reserve_temporary_stack(ctx.emitter, 32);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_string_value_to_regs(name, "x1", "x2")?;
            abi::emit_store_to_sp(ctx.emitter, "x1", 0);
            abi::emit_store_to_sp(ctx.emitter, "x2", 8);
            abi::emit_call_label(ctx.emitter, "__rt_global_lookup_dynamic");
            ctx.emitter.instruction(&format!("cbz x0, {miss}"));                // no matching global: use the undefined-name path
            ctx.emitter.instruction("ldr x0, [x0]");                            // load the global's boxed Mixed pointer
            ctx.emitter.instruction(&format!("cbz x0, {miss}"));                // an unset global uses the same miss path
            ctx.emitter.instruction(&format!("b {done}"));                      // the found value is ready
        }
        Arch::X86_64 => {
            ctx.load_string_value_to_regs(name, "rdi", "rsi")?;
            abi::emit_store_to_sp(ctx.emitter, "rdi", 0);
            abi::emit_store_to_sp(ctx.emitter, "rsi", 8);
            abi::emit_call_label(ctx.emitter, "__rt_global_lookup_dynamic");
            ctx.emitter.instruction("test rax, rax");                           // no matching global: use the undefined-name path
            ctx.emitter.instruction(&format!("jz {miss}"));                     // no matching global: use the undefined-name path
            ctx.emitter.instruction("mov rax, QWORD PTR [rax]");                // load the global's boxed Mixed pointer
            ctx.emitter.instruction("test rax, rax");                           // an unset global uses the same miss path
            ctx.emitter.instruction(&format!("jz {miss}"));                     // an unset global uses the same miss path
            ctx.emitter.instruction(&format!("jmp {done}"));                    // the found value is ready
        }
    }
    ctx.emitter.label(&miss);
    if warn_on_missing {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_symbol_address(ctx.emitter, "x1", "_diag_undefined_global_prefix");
                abi::emit_load_int_immediate(ctx.emitter, "x2", UNDEFINED_GLOBAL_PREFIX_LEN as i64);
                abi::emit_call_label(ctx.emitter, "__rt_diag_warning_fragment");
                abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", 0);
                abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", 8);
                abi::emit_call_label(ctx.emitter, "__rt_diag_warning_fragment");
                abi::emit_symbol_address(ctx.emitter, "x1", "_diag_undefined_array_key_suffix");
                abi::emit_load_int_immediate(ctx.emitter, "x2", 1);
                abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            }
            Arch::X86_64 => {
                abi::emit_symbol_address(ctx.emitter, "rdi", "_diag_undefined_global_prefix");
                abi::emit_load_int_immediate(ctx.emitter, "rsi", UNDEFINED_GLOBAL_PREFIX_LEN as i64);
                abi::emit_call_label(ctx.emitter, "__rt_diag_warning_fragment");
                abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 0);
                abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", 8);
                abi::emit_call_label(ctx.emitter, "__rt_diag_warning_fragment");
                abi::emit_symbol_address(ctx.emitter, "rdi", "_diag_undefined_array_key_suffix");
                abi::emit_load_int_immediate(ctx.emitter, "rsi", 1);
                abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            }
        }
    }
    crate::codegen::lower_inst::objects::emit_boxed_null(ctx);
    ctx.emitter.label(&done);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    store_if_result(ctx, inst)
}

/// Lowers `$GLOBALS[$name] = $value` into the runtime-selected global storage word.
///
/// Known program globals are replaced in place, publishing the new box before retiring the
/// previous payload so a destructor observes the assigned value. A name absent from the table
/// is evaluated for its side effects and discarded, matching the current known-name scope.
pub(super) fn lower_store_global_dynamic(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let name = expect_operand(inst, 0)?;
    let value = expect_operand(inst, 1)?;
    let miss = ctx.next_label("dyn_global_store_miss");
    let done = ctx.next_label("dyn_global_store_done");
    abi::emit_reserve_temporary_stack(ctx.emitter, 32);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_string_value_to_regs(name, "x1", "x2")?;
            abi::emit_call_label(ctx.emitter, "__rt_global_lookup_dynamic");
            abi::emit_store_to_sp(ctx.emitter, "x0", 0);
        }
        Arch::X86_64 => {
            ctx.load_string_value_to_regs(name, "rdi", "rsi")?;
            abi::emit_call_label(ctx.emitter, "__rt_global_lookup_dynamic");
            abi::emit_store_to_sp(ctx.emitter, "rax", 0);
        }
    }
    let ty = ctx.load_value_to_result(value)?;
    let source_ty = ty.codegen_repr();
    if source_ty != PhpType::Mixed {
        let box_ty = ctx.mixed_box_type(value)?;
        if ctx.value_can_transfer_ownership_to_consumer(value)?
            || global_store_owns_persisted_string(ctx, value, &source_ty)
        {
            emit_box_current_owned_value_as_mixed(ctx.emitter, &box_ty);
        } else {
            emit_box_current_value_as_mixed(ctx.emitter, &box_ty);
        }
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_store_to_sp(ctx.emitter, "x0", 8);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", 0);
            ctx.emitter.instruction(&format!("cbz x9, {miss}"));                // does the lookup return a writable global cell?
            ctx.emitter.instruction("ldr x10, [x9]");                           // capture the previous Mixed owner
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x11", 8);
            ctx.emitter.instruction("str x11, [x9]");                           // publish the assigned value before releasing the old one
            ctx.emitter.instruction(&format!("cbz x10, {done}"));               // nothing to release when the global was unset
            ctx.emitter.instruction("mov x0, x10");                             // pass the retired Mixed cell to the decrement helper
            abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
            ctx.emitter.instruction(&format!("b {done}"));                      // the assignment is complete
        }
        Arch::X86_64 => {
            abi::emit_store_to_sp(ctx.emitter, "rax", 8);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", 0);
            ctx.emitter.instruction(&format!("test r10, r10"));                 // does the lookup return a writable global cell?
            ctx.emitter.instruction(&format!("jz {miss}"));                     // no matching global: evaluate then discard the value
            ctx.emitter.instruction("mov r11, QWORD PTR [r10]");                // capture the previous Mixed owner
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", 8);
            ctx.emitter.instruction("mov QWORD PTR [r10], rax");                // publish the assigned value before releasing the old one
            ctx.emitter.instruction("test r11, r11");                           // nothing to release when the global was unset
            ctx.emitter.instruction(&format!("jz {done}"));                     // nothing to release when the global was unset
            ctx.emitter.instruction("mov rax, r11");                            // pass the retired Mixed cell in the x86_64 decref argument register
            abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
            ctx.emitter.instruction(&format!("jmp {done}"));                    // the assignment is complete
        }
    }
    ctx.emitter.label(&miss);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 8);
            ctx.emitter.instruction(&format!("cbz x0, {done}"));                // unknown names are evaluated then discarded
            abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", 8);
            ctx.emitter.instruction(&format!("test rax, rax"));                 // nothing to release when the value was never owned
            ctx.emitter.instruction(&format!("jz {done}"));                     // nothing to release when the value was never owned
            abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
        }
    }
    ctx.emitter.label(&done);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    Ok(())
}

/// Lowers a C extern global load into the EIR result slot.
pub(super) fn lower_extern_global_load(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let data = expect_global_name(inst)?;
    let name = ctx.global_name_data(data)?;
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("extern_global_load missing result value"))?;
    let ty = ctx.value_php_type(result)?;
    let symbol = ctx.emitter.target.extern_symbol(name);
    match ty.codegen_repr() {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Resource(_)
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Callable => {
            abi::emit_load_extern_symbol_to_reg(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                &symbol,
                0,
            );
        }
        PhpType::Float => {
            abi::emit_load_extern_symbol_to_reg(
                ctx.emitter,
                abi::float_result_reg(ctx.emitter),
                &symbol,
                0,
            );
        }
        PhpType::Str => {
            abi::emit_load_extern_symbol_to_reg(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                &symbol,
                0,
            );
            abi::emit_call_label(ctx.emitter, "__rt_cstr_to_str");
        }
        other => {
            ctx.emitter.comment(&format!(
                "WARNING: unsupported extern global load for ${} with PHP type {:?}",
                name, other
            ));
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers a C extern global store from one SSA operand.
pub(super) fn lower_extern_global_store(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let data = expect_global_name(inst)?;
    let name = ctx.global_name_data(data)?.to_string();
    let value = expect_operand(inst, 0)?;
    let ty = ctx.load_value_to_result(value)?.codegen_repr();
    let symbol = ctx.emitter.target.extern_symbol(&name);
    match ty {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Resource(_)
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Callable => {
            abi::emit_store_reg_to_extern_symbol(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                &symbol,
                0,
            );
        }
        PhpType::Float => {
            abi::emit_store_reg_to_extern_symbol(
                ctx.emitter,
                abi::float_result_reg(ctx.emitter),
                &symbol,
                0,
            );
        }
        PhpType::Str => {
            abi::emit_call_label(ctx.emitter, "__rt_str_to_cstr");
            abi::emit_store_reg_to_extern_symbol(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                &symbol,
                0,
            );
        }
        other => {
            ctx.emitter.comment(&format!(
                "WARNING: unsupported extern global store for ${} with PHP type {:?}",
                name, other
            ));
        }
    }
    Ok(())
}

/// Lowers an integer constant into the canonical integer result register and slot.
pub(super) fn lower_const_i64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_i64(inst)?;
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), value);
    store_if_result(ctx, inst)
}

/// Lowers a boolean constant into the canonical integer result register and slot.
pub(super) fn lower_const_bool(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = i64::from(expect_bool(inst)?);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), value);
    store_if_result(ctx, inst)
}

/// Lowers a null constant to the selected one-word or tagged-scalar representation.
pub(super) fn lower_const_null(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.result_php_type.codegen_repr() == PhpType::TaggedScalar {
        crate::codegen::sentinels::emit_tagged_scalar_null(ctx.emitter);
    } else {
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            0x7fff_ffff_ffff_fffe,
        );
    }
    store_if_result(ctx, inst)
}

/// Lowers explicit Mixed boxing for scalar, string, object, and existing Mixed operands.
pub(super) fn lower_mixed_box(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let source_ty = ctx.load_value_to_result(value)?;
    let raw_source_ty = ctx.raw_value_php_type(value)?;
    let box_ty = mixed_box_payload_type(source_ty, raw_source_ty);
    box_loaded_value_as_mixed(ctx, value, &box_ty);
    store_if_result(ctx, inst)
}

/// Produces an owned Mixed cell whose storage is independent from the assignment source.
///
/// Lowering initially inserts `MixedClone` for a boxed result, but optimization may narrow its
/// producer to a scalar constant. The final SSA storage type is authoritative: clone an existing
/// Mixed cell, otherwise box the current scalar, string, heap payload, or tagged nullable value.
pub(super) fn lower_mixed_clone(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let source_ir_type = ctx.value_ir_type(value)?;
    if matches!(
        source_ir_type,
        crate::ir::IrType::Heap(crate::ir::IrHeapKind::Mixed | crate::ir::IrHeapKind::Union)
    ) {
        load_value_to_first_int_arg(ctx, value)?;
        abi::emit_call_label(ctx.emitter, "__rt_mixed_clone");
    } else {
        let source_ty = ctx.load_value_to_result(value)?;
        let raw_source_ty = ctx.raw_value_php_type(value)?;
        let box_ty = mixed_box_payload_type(source_ty, raw_source_ty);
        box_loaded_value_as_mixed(ctx, value, &box_ty);
    }
    store_if_result(ctx, inst)
}

/// Boxes a loaded value while keeping borrowed string-offset bytes outside managed heap headers.
fn box_loaded_value_as_mixed(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    box_ty: &PhpType,
) {
    let staged = if box_ty.codegen_repr() == PhpType::Str {
        let (pointer_reg, length_reg) = abi::string_result_regs(ctx.emitter);
        super::ownership::stage_string_byte_view(ctx, value, pointer_reg, length_reg)
    } else {
        false
    };
    emit_box_current_value_as_mixed(ctx.emitter, box_ty);
    super::ownership::release_string_byte_stage(ctx, staged);
}

/// Selects the runtime boxing strategy without losing erased array-layout metadata.
fn mixed_box_payload_type(source_ty: PhpType, raw_source_ty: PhpType) -> PhpType {
    if matches!(
        &source_ty,
        PhpType::Array(element) if element.codegen_repr() == PhpType::Mixed
    ) {
        // `Array<Mixed>` is the raw ABI used for PHP arrays whose runtime storage may already
        // be associative. Reuse the iterable boxer so the owned Mixed cell records tag 4 or 5
        // from the payload's heap kind instead of stamping every value as indexed storage.
        PhpType::Iterable
    } else if matches!(raw_source_ty, PhpType::Resource(_))
        || crate::types::is_nullable_pointer_type(&raw_source_ty)
    {
        // A resource keeps its tag, and a `?ptr<T>` its null member (address 0 boxes as null).
        raw_source_ty
    } else {
        source_ty
    }
}

/// Lowers an invoker-only by-reference argument marker for descriptor calls.
pub(super) fn lower_invoker_ref_arg(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let slot = expect_local_slot(inst)?;
    let source_ty = ctx.local_php_type(slot)?.codegen_repr();
    let ref_cell_reg = abi::secondary_scratch_reg(ctx.emitter);
    let marker_tag_reg = abi::tertiary_scratch_reg(ctx.emitter);
    let source_tag_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.materialize_local_storage_address(slot, ref_cell_reg)?;
    abi::emit_load_int_immediate(
        ctx.emitter,
        marker_tag_reg,
        callable_invoker_args::INVOKER_ARG_REF_CELL_TAG,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        source_tag_reg,
        crate::codegen::runtime_value_tag(&source_ty) as i64,
    );
    ctx.emitter.comment("cufa_invoker_ref_cell");
    emit_box_runtime_payload_as_mixed(ctx.emitter, marker_tag_reg, ref_cell_reg, source_tag_reg);
    store_if_result(ctx, inst)
}
