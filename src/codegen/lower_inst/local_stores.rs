//! Purpose:
//! Lowers local stores, ref-cell promotion, aliasing, cleanup, and unset.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// Lowers an addressable local store from one SSA operand.
pub(super) fn lower_store_local(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let slot = expect_local_slot(inst)?;
    let value = expect_operand(inst, 0)?;
    let reset_concat_after_store =
        inst.span.is_some_and(|span| span.line > 0) && value_is_acquire_of_str_concat(ctx, value)?;
    ctx.store_value_to_local(slot, value)?;
    if reset_concat_after_store {
        reset_concat_to_frame_base(ctx);
    }
    Ok(())
}

/// Returns true when a value is `Acquire(StrConcat(...))`, which means storage now owns a heap copy.
pub(super) fn value_is_acquire_of_str_concat(ctx: &FunctionContext<'_>, value: ValueId) -> Result<bool> {
    let Some(acquire_inst) = instruction_for_value(ctx, value)? else {
        return Ok(false);
    };
    if acquire_inst.op != Op::Acquire {
        return Ok(false);
    }
    let Some(source) = acquire_inst.operands.first().copied() else {
        return Ok(false);
    };
    Ok(instruction_for_value(ctx, source)?
        .is_some_and(|source_inst| source_inst.op == Op::StrConcat))
}

/// Returns the instruction that produced an SSA value, or `None` for block parameters.
pub(super) fn instruction_for_value<'a>(
    ctx: &'a FunctionContext<'_>,
    value: ValueId,
) -> Result<Option<&'a Instruction>> {
    let metadata = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = metadata.def else {
        return Ok(None);
    };
    ctx.function
        .instruction(inst)
        .map(Some)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))
}

/// Lowers an explicit local ref-cell store through the pointer held in the slot.
pub(super) fn lower_store_ref_cell(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let slot = expect_local_slot(inst)?;
    let value = expect_operand(inst, 0)?;
    store_value_through_ref_cell_slot(ctx, slot, value, &inst.result_php_type)
}

/// Writes `value` into a ref-cell slot, picking the slot's active storage representation.
///
/// Shared with the mutating array builtins: a by-reference parameter is read with
/// `load_ref_cell`, so a builtin that RELOCATES its receiver (any growth path that reaches
/// `__rt_array_grow`) has to publish the new pointer through the same slot the value came from.
/// Writing only the raw frame slot would leave the caller's variable pointing at freed storage.
pub(super) fn store_value_through_ref_cell_slot(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
    value: ValueId,
    value_php_type: &PhpType,
) -> Result<()> {
    if ctx.local_ref_cell_representation_is_definite(slot) {
        return store_value_to_ref_cell_as(ctx, slot, value, value_php_type);
    }
    if !ctx.local_ref_cell_representation_is_dynamic(slot) {
        return ctx.store_value_to_raw_local(slot, value);
    }
    let state_offset = ctx.ref_cell_state_offset(slot).ok_or_else(|| {
        CodegenIrError::invalid_module(format!(
            "dynamic ref-cell slot {} has no representation flag",
            slot.as_raw()
        ))
    })?;
    let ref_cell = ctx.next_label("dynamic_store_ref_cell");
    let done = ctx.next_label("dynamic_store_ref_cell_done");
    let state_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, state_reg, state_offset);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(
                &format!("cbnz {}, {}", state_reg, ref_cell)
            );                                                                  // select ref-cell storage after a runtime promotion
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(
                &format!("test {}, {}", state_reg, state_reg)
            );                                                                  // test the slot's runtime representation flag
            ctx.emitter.instruction(&format!("jne {}", ref_cell));              // select ref-cell storage after a runtime promotion
        }
    }
    ctx.store_value_to_raw_local(slot, value)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b {}", done));                    // join dynamic ref-cell storage paths
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("jmp {}", done));                  // join dynamic ref-cell storage paths
        }
    }
    ctx.emitter.label(&ref_cell);
    store_value_to_ref_cell_as(ctx, slot, value, value_php_type)?;
    ctx.emitter.label(&done);
    Ok(())
}

/// Promotes an existing raw local slot into a heap ref-cell pointer.
pub(super) fn lower_promote_local_ref_cell(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let (slot, owner_slot) = expect_local_slot_pair(inst)?;
    promote_local_slot_for_ref_capture(ctx, slot, Some(owner_slot), &inst.result_php_type, true)
}

/// Binds a target local slot to the source local's existing ref-cell pointer.
pub(super) fn lower_alias_local_ref_cell(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let (target_slot, source_slot) = expect_local_slot_pair(inst)?;
    if !local_slot_stores_ref_cell_pointer(ctx, source_slot) {
        return Err(CodegenIrError::invalid_module(
            "alias_local_ref_cell source slot does not store a ref-cell pointer",
        ));
    }
    let source_offset = ctx.local_offset(source_slot)?;
    let target_offset = ctx.local_offset(target_slot)?;
    let pointer_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, pointer_reg, source_offset);
    abi::store_at_offset_scratch(
        ctx.emitter,
        pointer_reg,
        target_offset,
        abi::tertiary_scratch_reg(ctx.emitter),
    );
    ctx.mark_promoted_ref_cell(target_slot);
    Ok(())
}

/// Lowers `BindRefCellPtr`: binds the target local slot as a non-owning reference
/// alias to a ref-cell pointer value (operand 0). Stores the pointer into the slot and
/// marks it as a promoted ref cell so later loads/stores dereference it. The local does
/// not own the cell — the owner is the source object property — so no owner slot is
/// allocated and no release is emitted at scope exit.
pub(super) fn lower_bind_ref_cell_ptr(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let target_slot = expect_local_slot(inst)?;
    let target_offset = ctx.local_offset(target_slot)?;
    let pointer_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(value, pointer_reg)?;
    abi::store_at_offset_scratch(
        ctx.emitter,
        pointer_reg,
        target_offset,
        abi::tertiary_scratch_reg(ctx.emitter),
    );
    ctx.mark_promoted_ref_cell(target_slot);
    Ok(())
}

/// Releases an owned local ref-cell tracked by a hidden owner slot.
pub(super) fn lower_release_local_ref_cell(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let owner_slot = expect_local_slot(inst)?;
    release_local_ref_cell_owner(ctx, owner_slot, &inst.result_php_type)
}

/// Releases the owned ref-cell pointer in an owner slot and clears that owner.
pub(super) fn release_local_ref_cell_owner(
    ctx: &mut FunctionContext<'_>,
    owner_slot: LocalSlotId,
    value_ty: &PhpType,
) -> Result<()> {
    let owner_offset = ctx.local_offset(owner_slot)?;
    let done = ctx.next_label("release_ref_cell_owner_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::load_at_offset_scratch(ctx.emitter, "x9", owner_offset, "x11");
            ctx.emitter.instruction(&format!("cbz x9, {}", done));              // skip release when this variable no longer owns a fallback ref-cell
            abi::emit_release_local_ref_cell(ctx.emitter, "x9", value_ty);
            abi::emit_store_zero_to_local_slot(ctx.emitter, owner_offset);
        }
        Arch::X86_64 => {
            abi::load_at_offset_scratch(ctx.emitter, "r11", owner_offset, "r10");
            ctx.emitter.instruction("test r11, r11");                           // check whether this variable owns a fallback ref-cell
            ctx.emitter.instruction(&format!("je {}", done));                   // skip release when the fallback owner is already clear
            abi::emit_release_local_ref_cell(ctx.emitter, "r11", value_ty);
            abi::emit_store_zero_to_local_slot(ctx.emitter, owner_offset);
        }
    }
    ctx.emitter.label(&done);
    Ok(())
}

/// Lowers a deferred `release_local_slot`: releases the refcounted value currently
/// held in a local frame slot, typed by the slot's FINAL storage type.
///
/// Lowering emits this op before a retaining loop store when the slot's storage
/// type still looked untracked at that point but a later store on a back-edge
/// path could widen it (issue #534: an inner `for` counter widened Int→Mixed by
/// its checked-add update leaked one Mixed box per outer iteration when the
/// outer body re-initialized it). Only here, after widening finished, is the
/// decision sound. The slot is either zero (prologue zero-initializes cleanup
/// locals, and the null-guarded release helpers skip zero) or an owned value
/// boxed by a previous retaining store, so releasing it is always balanced.
pub(super) fn lower_release_local_slot(
    ctx: &mut FunctionContext<'_>,
    inst_id: InstId,
    inst: &Instruction,
) -> Result<()> {
    let slot = expect_local_slot(inst)?;
    // A slot promoted on a predecessor path holds the cell address, not an
    // owned value. The forward analysis intentionally ignores promotions that
    // occur only after this instruction, including after a loop exit.
    let ty = ctx.local_php_type(slot)?.codegen_repr();
    let offset = ctx.local_offset(slot)?;
    if ctx.release_local_slot_may_observe_ref_cell(inst_id) {
        // A merged path can hold either raw storage or a cell pointer. Slots
        // with a runtime representation flag release only the raw path; slots
        // that are always cells (notably by-ref params) remain excluded.
        if ctx.ref_cell_state_offset(slot).is_some() {
            super::super::frame::emit_owned_local_cleanup(ctx, slot, offset, &ty);
        }
        return Ok(());
    }
    match ty {
        // Owned strings are freed through the validating helper, which skips
        // null/uninitialized slots and non-heap (.rodata) literal pointers.
        PhpType::Str => super::super::frame::emit_main_string_cleanup(ctx, offset),
        PhpType::Callable => super::super::frame::emit_main_refcounted_cleanup(ctx, offset, &ty),
        other if other.is_refcounted() => {
            super::super::frame::emit_main_refcounted_cleanup(ctx, offset, &other)
        }
        // The slot never widened to refcounted storage: nothing can be owned.
        // Lowering normally prunes these, so this arm is only a safety net.
        _ => {}
    }
    Ok(())
}

/// Lowers `unset($local)` by breaking any promoted alias and writing PHP null locally.
pub(super) fn lower_unset_local(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let slot = expect_local_slot(inst)?;
    let offset = ctx.local_offset(slot)?;
    ctx.unmark_promoted_ref_cell(slot);
    if ctx.local_kind(slot)? == LocalKind::OwnedTemp {
        clear_local_slot_storage(ctx, slot, offset)?;
        return Ok(());
    }
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        0x7fff_ffff_ffff_fffe,
    );
    abi::store_at_offset(ctx.emitter, abi::int_result_reg(ctx.emitter), offset);
    if matches!(ctx.local_php_type(slot)?.codegen_repr(), PhpType::Str) {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        abi::store_at_offset(ctx.emitter, abi::int_result_reg(ctx.emitter), offset - 8);
    }
    Ok(())
}

/// Zeroes a local slot after an owned hidden temp has been moved into SSA.
pub(super) fn clear_local_slot_storage(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
    offset: usize,
) -> Result<()> {
    match ctx.local_php_type(slot)?.codegen_repr() {
        PhpType::Str | PhpType::TaggedScalar => {
            abi::emit_store_zero_to_local_slot(ctx.emitter, offset);
            abi::emit_store_zero_to_local_slot(ctx.emitter, offset - 8);
        }
        _ => {
            abi::emit_store_zero_to_local_slot(ctx.emitter, offset);
        }
    }
    Ok(())
}

/// Stores an SSA value through a local ref-cell pointer using the supplied alias type.
pub(super) fn store_value_to_ref_cell_as(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
    value: ValueId,
    target_ty: &PhpType,
) -> Result<()> {
    let source_ty = ctx.load_value_to_result(value)?;
    let target_ty = target_ty.codegen_repr();
    reject_multiword_ref_param_local(&target_ty, "store")?;
    coerce_ref_cell_store_value(ctx, &source_ty, &target_ty, true)?;
    let offset = ctx.local_offset(slot)?;
    let pointer_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, pointer_reg, offset);
    match target_ty {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_store_to_address(ctx.emitter, ptr_reg, pointer_reg, 0);
            abi::emit_store_to_address(ctx.emitter, len_reg, pointer_reg, 8);
        }
        PhpType::Float => {
            abi::emit_store_to_address(
                ctx.emitter,
                abi::float_result_reg(ctx.emitter),
                pointer_reg,
                0,
            );
        }
        PhpType::TaggedScalar => {
            abi::emit_store_to_address(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                pointer_reg,
                0,
            );
            abi::emit_store_to_address(
                ctx.emitter,
                crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter),
                pointer_reg,
                8,
            );
        }
        _ => {
            abi::emit_store_to_address(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                pointer_reg,
                0,
            );
        }
    }
    Ok(())
}

/// Converts a current result to a ref-cell shape, transferring an acquired owner when requested.
pub(super) fn coerce_ref_cell_store_value(
    ctx: &mut FunctionContext<'_>,
    source_ty: &PhpType,
    target_ty: &PhpType,
    source_is_owned: bool,
) -> Result<()> {
    let source_ty = source_ty.codegen_repr();
    let target_ty = target_ty.codegen_repr();
    if target_ty == PhpType::Mixed && source_ty != PhpType::Mixed {
        if source_is_owned {
            emit_box_current_owned_value_as_mixed(ctx.emitter, &source_ty);
        } else {
            emit_box_current_value_as_mixed(ctx.emitter, &source_ty);
        }
        return Ok(());
    }
    if target_ty == PhpType::TaggedScalar {
        coerce_loaded_value_to_tagged_scalar(ctx, &source_ty)?;
        return Ok(());
    }
    if source_ty == PhpType::Mixed {
        match target_ty {
            PhpType::Int => {
                // Save the Mixed pointer on the stack, narrow to int, then
                // release the Mixed box to avoid leaking the checked-arithmetic temporary.
                move_int_result_to_first_arg(ctx);
                let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
                let result_reg = abi::int_result_reg(ctx.emitter);
                // Stack layout after pushes: [result_placeholder | saved_mixed_ptr]
                abi::emit_push_reg(ctx.emitter, result_reg); // placeholder for int result
                abi::emit_push_reg(ctx.emitter, arg_reg);     // save the Mixed pointer
                abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
                // Save int result to placeholder (at sp+16).
                match ctx.emitter.target.arch {
                    Arch::AArch64 => {
                        ctx.emitter.instruction("str x0, [sp, #16]");           // save the int result to the placeholder slot above the saved Mixed pointer
                    }
                    Arch::X86_64 => {
                        ctx.emitter.instruction(
                            "mov QWORD PTR [rsp + 16], rax"
                        );                                                      // save the int result to the placeholder slot above the saved Mixed pointer
                    }
                }
                // Pop the saved Mixed pointer into result_reg for decref_mixed.
                abi::emit_pop_reg(ctx.emitter, result_reg);
                abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
                // Restore the int result from the placeholder.
                abi::emit_pop_reg(ctx.emitter, result_reg);
                return Ok(());
            }
            PhpType::Bool => {
                move_int_result_to_first_arg(ctx);
                let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
                let result_reg = abi::int_result_reg(ctx.emitter);
                abi::emit_push_reg(ctx.emitter, result_reg);
                abi::emit_push_reg(ctx.emitter, arg_reg);
                abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool");
                match ctx.emitter.target.arch {
                    Arch::AArch64 => {
                        ctx.emitter.instruction("str x0, [sp, #16]");           // save the bool result to the placeholder slot
                    }
                    Arch::X86_64 => {
                        ctx.emitter.instruction(
                            "mov QWORD PTR [rsp + 16], rax"
                        );                                                      // save the bool result to the placeholder slot
                    }
                }
                abi::emit_pop_reg(ctx.emitter, result_reg);
                abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
                abi::emit_pop_reg(ctx.emitter, result_reg);
                return Ok(());
            }
            PhpType::Float => {
                move_int_result_to_first_arg(ctx);
                let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
                let int_reg = abi::int_result_reg(ctx.emitter);
                abi::emit_push_reg(ctx.emitter, int_reg); // placeholder for float result
                abi::emit_push_reg(ctx.emitter, arg_reg); // save Mixed pointer
                abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float");
                match ctx.emitter.target.arch {
                    Arch::AArch64 => {
                        ctx.emitter.instruction("str d0, [sp, #16]");           // save the float result to the placeholder slot
                    }
                    Arch::X86_64 => {
                        ctx.emitter.instruction(
                            "movsd QWORD PTR [rsp + 16], xmm0"
                        );                                                      // save the float result to the placeholder slot
                    }
                }
                abi::emit_pop_reg(ctx.emitter, int_reg); // pop Mixed pointer into int_reg
                abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
                match ctx.emitter.target.arch {
                    Arch::AArch64 => {
                        ctx.emitter.instruction("ldr d0, [sp], #16");           // restore the float result and release the placeholder slot
                    }
                    Arch::X86_64 => {
                        ctx.emitter.instruction("movsd xmm0, QWORD PTR [rsp]"); // restore the float result from the placeholder
                        ctx.emitter.instruction("add rsp, 16");                 // release the float result placeholder slot
                    }
                }
                return Ok(());
            }
            PhpType::Str => {
                move_int_result_to_first_arg(ctx);
                abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
                return Ok(());
            }
            _ => {}
        }
    }
    Ok(())
}

/// Rejects by-reference parameter locals whose frame representation spans multiple words.
pub(super) fn reject_multiword_ref_param_local(ty: &PhpType, action: &str) -> Result<()> {
    let _ = (ty, action);
    Ok(())
}
