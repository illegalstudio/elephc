//! Purpose:
//! Lowers globals, extern globals, constants, Mixed boxing, and invoker ref markers.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;
use crate::ir::RuntimeCallTarget;

/// Lowers a global storage load into the result register and SSA destination slot.
pub(super) fn lower_load_global(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let data = expect_global_name(inst)?;
    // Owned, because the shared-ref-cell question needs `ctx.shared` mutably to answer it once
    // per module instead of walking every instruction per name.
    let name = ctx.global_name_data(data)?.to_string();
    let symbol = ir_global_symbol(&name);
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("load_global missing result value"))?;
    let ty = ctx.value_php_type(result)?;
    if ctx.shared.uses_shared_ref_cell(ctx.module, &name) {
        load_shared_global_to_result(ctx, &symbol, &ty);
        return store_if_result(ctx, inst);
    }
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
    if ctx.shared.uses_shared_ref_cell(ctx.module, &name) {
        // `ArrayFetchForWrite` consumed the container it was given and returned its replacement,
        // so a nested write's storeback only publishes. Releasing the previous pointer again
        // freed a `--web` superglobal's hash twice on `$_SESSION['bag']['k'] = 1`.
        if value_is_consuming_container_fetch(ctx, value)? {
            return store_shared_global(ctx, &symbol, &store_ty, SharedGlobalPrevious::Keep);
        }
        return lower_store_shared_global(ctx, &symbol, &store_ty);
    }
    ctx.data
        .add_comm(symbol.clone(), store_ty.codegen_repr().stack_size().max(8));
    let release_previous = !value_is_consuming_container_fetch(ctx, value)?;
    abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &store_ty, release_previous);
    Ok(())
}

/// Whether `value` is a nested write's parent fetch on a concrete array, which consumes it.
fn value_is_consuming_container_fetch(ctx: &FunctionContext<'_>, value: ValueId) -> Result<bool> {
    let Some(fetch) = super::local_stores::instruction_for_value(ctx, value)? else {
        return Ok(false);
    };
    Ok(fetch.op == Op::RuntimeCall
        && matches!(
            fetch.immediate,
            Some(Immediate::RuntimeCall(
                RuntimeCallTarget::ArrayFetchForWrite
                    | RuntimeCallTarget::ArrayFetchForWriteAlreadyDiagnosed
            ))
        )
        && matches!(
            fetch.result_php_type.codegen_repr(),
            PhpType::Array(_) | PhpType::AssocArray { .. }
        ))
}

/// Removes the name's global owner while existing aliases retain their cells.
pub(super) fn lower_unset_global(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let name = ctx.global_name_data(expect_global_name(inst)?)?.to_string();
    unset_global_name(ctx, &name);
    Ok(())
}

/// Removes the symbol's owner before invoking any payload destructor.
pub(in crate::codegen) fn unset_global_name(ctx: &mut FunctionContext<'_>, name: &str) {
    let symbol = ir_global_symbol(name);
    ctx.data.add_comm(symbol.clone(), 8);
    abi::emit_load_symbol_to_reg(ctx.emitter, abi::int_result_reg(ctx.emitter), &symbol, 0);
    abi::emit_store_zero_to_symbol(ctx.emitter, &symbol, 0);
    let release = if ctx.shared.uses_shared_ref_cell(ctx.module, name) {
        "__rt_global_ref_cell_decref"
    } else {
        "__rt_decref_mixed"
    };
    abi::emit_call_label(ctx.emitter, release);
}

/// Acquires a durable reference-cell share before replacing a local binding.
pub(super) fn lower_global_ref_cell(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let data = expect_global_name(inst)?;
    let name = ctx.global_name_data(data)?.to_string();
    let symbol = ir_global_symbol(&name);
    ctx.data.add_comm(symbol.clone(), 8);
    let ready = ctx.next_label("global_ref_cell_ready");
    abi::emit_load_symbol_to_reg(ctx.emitter, abi::int_result_reg(ctx.emitter), &symbol, 0);
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &ready);
    if ctx.module.web && crate::superglobals::is_superglobal(&name) {
        // A `--web` superglobal's cell holds the bare hash every reader expects
        // (`$s = &$_SESSION` before `session_start()` gets an empty one), not a box.
        let hash_ty = crate::superglobals::superglobal_type();
        let capacity_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
        let value_tag_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
        abi::emit_load_int_immediate(ctx.emitter, capacity_reg, 16);
        abi::emit_load_int_immediate(ctx.emitter, value_tag_reg, runtime_value_tag(&PhpType::Mixed) as i64);
        abi::emit_call_label(ctx.emitter, "__rt_hash_new");
        lower_store_shared_global(ctx, &symbol, &hash_ty)?;
    } else {
        // Off-web a superglobal is stored like any other global: a boxed Mixed.
        emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
        lower_store_shared_global(ctx, &symbol, &PhpType::Mixed)?;
    }
    ctx.emitter.label(&ready);
    abi::emit_load_symbol_to_reg(ctx.emitter, abi::int_result_reg(ctx.emitter), &symbol, 0);
    abi::emit_call_label(ctx.emitter, "__rt_global_ref_cell_incref");
    store_if_result(ctx, inst)
}

/// Stores a global through its shared reference cell, preserving live aliases.
pub(in crate::codegen) fn lower_store_shared_global(
    ctx: &mut FunctionContext<'_>,
    symbol: &str,
    ty: &PhpType,
) -> Result<()> {
    store_shared_global(ctx, symbol, ty, SharedGlobalPrevious::Release)
}

/// Stores a global through its shared reference cell and leaves the replaced payload owner in
/// the result register, for a caller that retires it on its own cleanup path.
pub(in crate::codegen) fn lower_replace_shared_global(
    ctx: &mut FunctionContext<'_>,
    symbol: &str,
    ty: &PhpType,
) -> Result<()> {
    store_shared_global(ctx, symbol, ty, SharedGlobalPrevious::Return)
}

/// Publishes a container that was mutated in place back into its global's shared reference
/// cell, without releasing what the cell held.
///
/// The ordinary-global write-back (`writeback_global_array_source`) stores with
/// `release_previous: false` because the cell already owns this container: only its ADDRESS may
/// have changed, and the relocating helper (`__rt_hash_set`'s growth or split) has accounted for
/// the old one. Routing the shared-cell twin through the releasing store decremented the
/// container the cell was about to hold again -- when `__rt_hash_set` updated in place, that
/// freed the live `$_SESSION` hash on the first `$_SESSION[$key] = ...` of a request whose
/// program also takes `&$_SESSION` (Symfony's session storage), and the next write reused the
/// freed block.
pub(in crate::codegen) fn lower_writeback_shared_global(
    ctx: &mut FunctionContext<'_>,
    symbol: &str,
    ty: &PhpType,
) -> Result<()> {
    store_shared_global(ctx, symbol, ty, SharedGlobalPrevious::Keep)
}

/// Stores the result register into a global's shared reference cell, creating the cell on first
/// use; `previous` says what happens to the payload it held.
fn store_shared_global(
    ctx: &mut FunctionContext<'_>,
    symbol: &str,
    ty: &PhpType,
    previous: SharedGlobalPrevious,
) -> Result<()> {
    let store_ty = ty.codegen_repr();
    if !matches!(store_ty, PhpType::AssocArray { .. } | PhpType::Mixed) {
        return Err(CodegenIrError::unsupported(format!(
            "global reference-cell store for PHP type {:?}",
            store_ty
        )));
    }
    ctx.data.add_comm(symbol.to_string(), 8);
    let cell_ready = ctx.next_label("web_superglobal_cell_ready");
    abi::emit_push_result_value(ctx.emitter, &store_ty);
    abi::emit_load_symbol_to_reg(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        symbol,
        0,
    );
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &cell_ready);

    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 16);
    abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
    let kind_reg = abi::secondary_scratch_reg(ctx.emitter);
    let kind_word = match ctx.emitter.target.arch {
        Arch::AArch64 => crate::codegen_support::sentinels::GLOBAL_REF_CELL_HEAP_KIND as i64,
        Arch::X86_64 => crate::codegen_support::sentinels::x86_64_heap_kind_word(
            crate::codegen_support::sentinels::GLOBAL_REF_CELL_HEAP_KIND.into(),
        ) as i64,
    };
    abi::emit_load_int_immediate(ctx.emitter, kind_reg, kind_word);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!(
            "str {}, [{}, #-8]",
            kind_reg,
            abi::int_result_reg(ctx.emitter)
        )),
        Arch::X86_64 => ctx.emitter.instruction(&format!(
            "mov QWORD PTR [{} - 8], {}",
            abi::int_result_reg(ctx.emitter),
            kind_reg
        )),
    }
    abi::emit_store_zero_to_address(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_store_zero_to_address(ctx.emitter, abi::int_result_reg(ctx.emitter), 8);
    abi::emit_store_reg_to_symbol(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        symbol,
        0,
    );

    ctx.emitter.label(&cell_ready);
    let cell_reg = abi::secondary_scratch_reg(ctx.emitter);
    let previous_reg = abi::tertiary_scratch_reg(ctx.emitter);
    abi::emit_reg_move(ctx.emitter, cell_reg, abi::int_result_reg(ctx.emitter));
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    if store_ty != PhpType::Mixed {
        // A `--web` superglobal hash: the cell holds a boxed Mixed shared by every alias
        // (locals, array elements, properties), so the hash goes INTO that box.
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x1, x0");                          // the transferred hash is the helper's second argument
                ctx.emitter.instruction(&format!("mov x0, {cell_reg}"));         // the cell is its first
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rdi, rax");                        // the transferred hash is the helper's second argument
                ctx.emitter.instruction(&format!("mov rax, {cell_reg}"));        // the cell is its first
            }
        }
        abi::emit_call_label(ctx.emitter, "__rt_shared_cell_hash_store");
        match previous {
            SharedGlobalPrevious::Keep => {}
            SharedGlobalPrevious::Release => {
                abi::emit_call_label(ctx.emitter, "__rt_decref_hash");
            }
            SharedGlobalPrevious::Return => {}
        }
        return Ok(());
    }
    // Publish the new payload before the old one is released: the release can run a
    // destructor, and php's destructor already reads the replacement through the global.
    abi::emit_load_from_address(ctx.emitter, previous_reg, cell_reg, 0);
    abi::emit_store_to_address(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        cell_reg,
        0,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        runtime_value_tag(&store_ty) as i64,
    );
    abi::emit_store_to_address(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        cell_reg,
        8,
    );
    match previous {
        SharedGlobalPrevious::Keep => {}
        SharedGlobalPrevious::Release => {
            abi::emit_reg_move(ctx.emitter, abi::int_result_reg(ctx.emitter), previous_reg);
            abi::emit_call_label(ctx.emitter, shared_global_payload_release(&store_ty));
        }
        SharedGlobalPrevious::Return => {
            abi::emit_reg_move(ctx.emitter, abi::int_result_reg(ctx.emitter), previous_reg);
        }
    }
    Ok(())
}

/// What a shared-cell global store does with the payload it replaced.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SharedGlobalPrevious {
    /// The cell already owns the stored container (an in-place write-back).
    Keep,
    /// Release the replaced owner after the new payload is published.
    Release,
    /// Leave the replaced owner in the result register for the caller to retire.
    Return,
}

/// Returns the release helper for a shared-cell global payload of `store_ty`.
fn shared_global_payload_release(store_ty: &PhpType) -> &'static str {
    if *store_ty == PhpType::Mixed {
        "__rt_decref_mixed"
    } else {
        "__rt_decref_hash"
    }
}

/// Loads the pointer-sized payload held by a shared global reference cell, as `ty`.
///
/// The cell holds a boxed Mixed; a hash-typed reader (a `--web` superglobal) borrows the hash
/// inside it instead.
pub(in crate::codegen) fn load_shared_global_to_result(
    ctx: &mut FunctionContext<'_>,
    symbol: &str,
    ty: &PhpType,
) {
    ctx.data.add_comm(symbol.to_string(), 8);
    if !matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_load_symbol_to_reg(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            symbol,
            0,
        );
        abi::emit_call_label(ctx.emitter, "__rt_shared_cell_hash_load");
        return;
    }
    let done = ctx.next_label("load_web_superglobal_done");
    abi::emit_load_symbol_to_reg(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        symbol,
        0,
    );
    abi::emit_branch_if_int_result_zero(ctx.emitter, &done);
    let cell_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_from_address(ctx.emitter, cell_reg, cell_reg, 0);
    ctx.emitter.label(&done);
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
        && !ctx.value_uses().is_released(value)
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
    if let Some(Immediate::GlobalName(data)) = inst.immediate {
        return lower_global_array_ref_marker(ctx, inst, data);
    }
    let slot = expect_local_slot(inst)?;
    let source_ty = ctx.local_php_type(slot)?.codegen_repr();
    let ref_cell_reg = abi::secondary_scratch_reg(ctx.emitter);
    let marker_tag_reg = abi::tertiary_scratch_reg(ctx.emitter);
    let source_tag_reg = abi::symbol_scratch_reg(ctx.emitter);
    emit_marker_source_tag(ctx, slot, &source_ty, source_tag_reg)?;
    ctx.materialize_local_storage_address(slot, ref_cell_reg)?;
    ctx.emitter.comment("cufa_invoker_ref_cell");
    if ctx.is_by_ref_param_slot(slot) {
        // The address came from the caller and may sit inside an allocation; see the bit's doc.
        abi::emit_reg_move(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), ref_cell_reg);
        abi::emit_reg_move(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), source_tag_reg);
        abi::emit_call_label(ctx.emitter, "__rt_invoker_borrowed_ref_marker");
        return store_if_result(ctx, inst);
    }
    abi::emit_load_int_immediate(
        ctx.emitter,
        marker_tag_reg,
        callable_invoker_args::INVOKER_ARG_REF_CELL_TAG,
    );
    emit_box_runtime_payload_as_mixed(ctx.emitter, marker_tag_reg, ref_cell_reg, source_tag_reg);
    store_if_result(ctx, inst)
}

/// Lowers an owning array marker for a promoted local reference cell.
pub(super) fn lower_array_local_ref_cell(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let slot = expect_local_slot(inst)?;
    let source_ty = ctx.local_php_type(slot)?.codegen_repr();
    let ref_cell_reg = abi::secondary_scratch_reg(ctx.emitter);
    ctx.materialize_local_storage_address(slot, ref_cell_reg)?;
    abi::emit_reg_move(ctx.emitter, abi::int_result_reg(ctx.emitter), ref_cell_reg);
    abi::emit_call_label(ctx.emitter, "__rt_ref_cell_incref");
    abi::emit_reg_move(ctx.emitter, ref_cell_reg, abi::int_result_reg(ctx.emitter));
    let marker_tag_reg = abi::tertiary_scratch_reg(ctx.emitter);
    let source_tag_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_load_int_immediate(
        ctx.emitter,
        marker_tag_reg,
        callable_invoker_args::ARRAY_LOCAL_REF_CELL_TAG,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        source_tag_reg,
        crate::codegen::runtime_value_tag(&source_ty) as i64,
    );
    ctx.emitter.comment("array_local_ref_cell");
    emit_box_runtime_payload_as_mixed(ctx.emitter, marker_tag_reg, ref_cell_reg, source_tag_reg);
    store_if_result(ctx, inst)
}

/// Lowers an owning array-reference marker for a request-global ref-cell.
fn lower_global_array_ref_marker(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    data: crate::ir::DataId,
) -> Result<()> {
    let name = ctx.global_name_data(data)?.to_string();
    if !crate::superglobals::is_superglobal(&name) {
        return Err(CodegenIrError::unsupported(format!(
            "array reference to non-superglobal global ${name}"
        )));
    }
    // The superglobal's shared cell holds a boxed Mixed in every mode (`store_shared_global`),
    // so the marker advertises a Mixed payload and its writers go through the same box the
    // superglobal's own hash-typed readers unwrap.
    let symbol = ir_global_symbol(&name);
    ctx.data.add_comm(symbol.clone(), 8);
    abi::emit_load_symbol_to_reg(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &symbol,
        0,
    );
    abi::emit_call_label(ctx.emitter, "__rt_global_ref_cell_incref");
    let cell_reg = abi::secondary_scratch_reg(ctx.emitter);
    let marker_tag_reg = abi::tertiary_scratch_reg(ctx.emitter);
    let source_tag_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_reg_move(ctx.emitter, cell_reg, abi::int_result_reg(ctx.emitter));
    abi::emit_load_int_immediate(
        ctx.emitter,
        marker_tag_reg,
        callable_invoker_args::ARRAY_GLOBAL_REF_CELL_TAG,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        source_tag_reg,
        runtime_value_tag(&PhpType::Mixed) as i64,
    );
    emit_box_runtime_payload_as_mixed(
        ctx.emitter,
        marker_tag_reg,
        cell_reg,
        source_tag_reg,
    );
    store_if_result(ctx, inst)
}

/// Loads the runtime value tag a by-reference marker should advertise for one local.
///
/// Every tag but the array ones is exactly what the static type spells. `array` is not: it is the
/// PACKED tag whatever the slot ends up holding, and a marker that promises packed for a
/// string-keyed hash hands the reader an array with no keys and a raw slot word at element 0.
/// The heap-kind byte in front of the payload is the only thing that knows which it is.
///
/// Read INLINE rather than through a helper call: this lowering sits in the middle of an
/// argument-marshalling sequence that never called anything, and introducing a call there
/// clobbers the caller-saved registers the surrounding pack is still holding.
pub(super) fn emit_marker_source_tag(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
    source_ty: &PhpType,
    source_tag_reg: &str,
) -> Result<()> {
    // An `int|float`-style tagged scalar has no static tag: its runtime tag is word 1 of the
    // local's storage, beside the payload (the layout every tagged load and store uses). Asking
    // `runtime_value_tag` for it panicked the compiler on a by-reference descriptor argument.
    if matches!(source_ty, PhpType::TaggedScalar) {
        let pointer = abi::int_result_reg(ctx.emitter);
        ctx.materialize_local_storage_address(slot, pointer)?;
        abi::emit_load_from_address(ctx.emitter, source_tag_reg, pointer, 8);
        return Ok(());
    }
    if !matches!(source_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        abi::emit_load_int_immediate(
            ctx.emitter,
            source_tag_reg,
            crate::codegen::runtime_value_tag(source_ty) as i64,
        );
        return Ok(());
    }
    let pointer = abi::int_result_reg(ctx.emitter);
    let kind = abi::temp_int_reg(ctx.emitter.target);
    let done = ctx.next_label("marker_array_tag_done");
    ctx.materialize_local_storage_address(slot, pointer)?;
    abi::emit_load_from_address(ctx.emitter, pointer, pointer, 0);
    abi::emit_load_int_immediate(ctx.emitter, source_tag_reg, 4);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz {pointer}, {done}")); // an empty slot has no heap kind to read
            ctx.emitter.instruction(&format!("ldur {kind}, [{pointer}, #-8]")); // uniform heap header word
            ctx.emitter.instruction(&format!("and {kind}, {kind}, #0xff")); // isolate the low-byte heap kind
            ctx.emitter.instruction(&format!("cmp {kind}, #3")); // heap kind 3 stores associative hashes
            ctx.emitter.instruction(&format!("b.ne {done}")); // anything else keeps the packed tag
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {pointer}, {pointer}")); // an empty slot has no heap kind to read
            ctx.emitter.instruction(&format!("jz {done}"));
            ctx.emitter.instruction(&format!("mov {kind}, QWORD PTR [{pointer} - 8]")); // uniform heap header word
            ctx.emitter.instruction(&format!("and {kind}, 0xff")); // isolate the low-byte heap kind
            ctx.emitter.instruction(&format!("cmp {kind}, 3")); // heap kind 3 stores associative hashes
            ctx.emitter.instruction(&format!("jne {done}")); // anything else keeps the packed tag
        }
    }
    abi::emit_load_int_immediate(ctx.emitter, source_tag_reg, 5);
    ctx.emitter.label(&done);
    Ok(())
}
