//! Purpose:
//! Lowers function static-local loads, stores, and one-time initializers for
//! the Phase 04 EIR backend.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - Static locals are backed by `.comm` symbols and an initialization marker,
//!   so their values persist across function calls without using frame slots.
//! - The initializer expression runs only while the marker is clear: lowering branches
//!   on `static_local_uninitialized` around it, so a constructor or any other side effect
//!   in it happens once, as in PHP. `init_static_local` still checks the marker, because
//!   a recursive call made by the initializer can initialize the slot first; PHP keeps
//!   that innermost value and the outer one is released. Assignments retain refcounted
//!   values before publishing a second owner.
//! - A managed initializer publishes its cell and initialization marker before releasing
//!   a non-transferable source owner, so destructor side effects observe initialized storage.
//! - Both symbols come from `crate::names::static_local_symbol()` /
//!   `static_local_init_symbol()`, which encode the (function, variable) pair injectively.
//!   Building them by string concatenation merged unrelated statics onto one cell.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::ir::{Instruction, LocalSlot, LocalSlotId, ValueId};
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{coerce_loaded_local_to_result_type, expect_local_slot, expect_operand};
use crate::codegen::{CodegenIrError, Result};

/// Resolved function static-local metadata for symbol-backed storage.
struct StaticLocalSlot {
    name: String,
    php_type: PhpType,
    symbol: String,
    init_symbol: String,
    managed_reference: bool,
}

/// Lowers a static-local read into the current result register(s).
pub(super) fn lower_load_static_local(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let slot = resolve_static_local_slot(ctx, inst)?;
    ensure_static_local_type_supported(&slot, inst)?;
    let result = inst.result.ok_or_else(|| {
        CodegenIrError::invalid_module("load_static_local missing result value")
    })?;
    abi::emit_load_symbol_to_result(ctx.emitter, &slot.symbol, &slot.php_type);
    let result_ty = ctx.value_php_type(result)?;
    coerce_loaded_local_to_result_type(ctx, &slot.php_type, &result_ty)?;
    ctx.store_result_value(result)
}

/// Borrows a persistent managed cell while the static symbol keeps its lifetime owner.
pub(super) fn lower_load_static_local_ref_cell(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let slot = resolve_static_local_slot(ctx, inst)?;
    ensure_static_local_type_supported(&slot, inst)?;
    let result = inst.result.ok_or_else(|| CodegenIrError::invalid_module("static reference missing result"))?;
    abi::emit_load_symbol_to_reg(ctx.emitter, abi::int_result_reg(ctx.emitter), &slot.symbol, 0);
    ctx.store_result_value(result)
}

/// Lowers a static-local assignment from one SSA operand into symbol-backed storage.
pub(super) fn lower_store_static_local(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let slot = resolve_static_local_slot(ctx, inst)?;
    ensure_static_local_type_supported(&slot, inst)?;
    ensure_static_local_value_supported(ctx, &slot, value, inst)?;
    let mut loaded_ty = ctx.load_value_to_result(value)?.codegen_repr();
    // Narrow Mixed to Int when the static local slot is Int-typed
    // (from checked integer arithmetic that may overflow to float).
    if matches!(slot.php_type.codegen_repr(), PhpType::Int)
        && matches!(loaded_ty, PhpType::Mixed)
    {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                // x0 already holds the Mixed pointer
                abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            }
            Arch::X86_64 => {
                // rax holds the Mixed pointer; __rt_mixed_cast_int expects rdi
                ctx.emitter.instruction("mov rdi, rax");                        // move the Mixed pointer into the first SysV argument register
                abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            }
        }
        // Release the original Mixed box after narrowing.
        // The value SSA still holds the Mixed pointer; reload it and decref.
        // But the narrowed int is in the result reg, so save it first.
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        ctx.load_value_to_result(value)?;
        abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
        abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        loaded_ty = PhpType::Int;
    }
    // A persistent Mixed slot needs a box even when this arm has a concrete scalar or object.
    // The fresh box owns its payload independently of the source SSA value.
    if matches!(slot.php_type.codegen_repr(), PhpType::Mixed) && loaded_ty != PhpType::Mixed {
        let box_ty = if matches!(loaded_ty, PhpType::Pointer(_)) { ctx.mixed_box_type(value)? } else { loaded_ty.clone() };
        crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &box_ty);
    } else if loaded_ty.is_refcounted() {
        abi::emit_incref_if_refcounted(ctx.emitter, &loaded_ty);
    }
    abi::emit_store_result_to_symbol(ctx.emitter, &slot.symbol, &slot.php_type, true);
    clear_static_local_high_word_if_needed(ctx, &slot);
    Ok(())
}

/// Lowers the "initializer has not run yet" test of a static local: 1 while its marker is
/// clear, 0 once `init_static_local` has set it.
pub(super) fn lower_static_local_uninitialized(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let slot = resolve_static_local_slot(ctx, inst)?;
    let result = inst.result.ok_or_else(|| {
        CodegenIrError::invalid_module("static_local_uninitialized missing result value")
    })?;
    let unset_label = ctx.next_label("static_local_unset");
    let done_label = ctx.next_label("static_local_test_done");
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_symbol_to_reg(ctx.emitter, result_reg, &slot.init_symbol, 0);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &unset_label);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&unset_label);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 1);
    ctx.emitter.label(&done_label);
    ctx.store_result_value(result)
}

/// Lowers a static-local declaration initializer guarded by the per-slot marker.
pub(super) fn lower_init_static_local(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let slot = resolve_static_local_slot(ctx, inst)?;
    ensure_static_local_type_supported(&slot, inst)?;
    ensure_static_local_value_supported(ctx, &slot, value, inst)?;
    let initialized_label = ctx.next_label("static_local_initialized");
    let done_label = ctx.next_label("static_local_init_done");
    abi::emit_load_symbol_to_reg(ctx.emitter, abi::int_result_reg(ctx.emitter), &slot.init_symbol, 0);
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &initialized_label);
    if slot.managed_reference {
        initialize_managed_static(ctx, &slot, value)?;
        abi::emit_jump(ctx.emitter, &done_label);
        ctx.emitter.label(&initialized_label);
        release_unadopted_static_initializer(ctx, value)?;
        ctx.emitter.label(&done_label);
        return Ok(());
    }
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
    abi::emit_store_reg_to_symbol(ctx.emitter, abi::int_result_reg(ctx.emitter), &slot.init_symbol, 0);
    let mut loaded_ty = ctx.load_value_to_result(value)?.codegen_repr();
    // Narrow Mixed to Int when the static local slot is Int-typed.
    if matches!(slot.php_type.codegen_repr(), PhpType::Int)
        && matches!(loaded_ty, PhpType::Mixed)
    {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rdi, rax");                        // move the Mixed pointer into the first SysV argument register
                abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            }
        }
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        ctx.load_value_to_result(value)?;
        abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
        abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        loaded_ty = PhpType::Int;
    }
    // Box Int/Float/Bool/Void as Mixed when the static local slot is Mixed-typed. A pointer
    // word boxes by its declared type, so a null `?ptr<T>` initializer is PHP null.
    if matches!(slot.php_type.codegen_repr(), PhpType::Mixed)
        && !matches!(loaded_ty, PhpType::Mixed)
    {
        let box_ty = if matches!(loaded_ty, PhpType::Pointer(_)) {
            ctx.mixed_box_type(value)?
        } else {
            loaded_ty.clone()
        };
        crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &box_ty);
    }
    let store_ty = slot.php_type.codegen_repr();
    abi::emit_store_result_to_symbol(ctx.emitter, &slot.symbol, &store_ty, false);
    clear_static_local_high_word_if_needed(ctx, &slot);
    abi::emit_jump(ctx.emitter, &done_label);
    // Lowering only evaluates the initializer while the marker is clear, but a recursive call
    // made BY the initializer can set it first. PHP keeps that innermost value, so this outer
    // initializer is released here unless a published operand owner already owns its cleanup.
    ctx.emitter.label(&initialized_label);
    release_unadopted_static_initializer(ctx, value)?;
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Transfers the initializer into a canonical cell before publishing successful initialization.
fn initialize_managed_static(ctx: &mut FunctionContext<'_>, slot: &StaticLocalSlot, value: ValueId) -> Result<()> {
    let source = ctx.mixed_box_type(value)?;
    let can_transfer = ctx.value_can_own_mixed_box_source(value)?;
    let release_unadopted_source = !can_transfer;
    ctx.load_value_to_result(value)?;
    if can_transfer {
        crate::codegen::emit_box_current_owned_value_as_mixed(ctx.emitter, &source);
    } else if source.codegen_repr() == PhpType::Mixed {
        abi::emit_call_label(ctx.emitter, "__rt_mixed_clone");
    } else {
        crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &source);
    }
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), 7);
    abi::emit_call_label(ctx.emitter, "__rt_reference_cell_new");
    let boxed = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_pop_reg(ctx.emitter, boxed);
    abi::emit_store_to_address(ctx.emitter, boxed, result, 0);
    abi::emit_call_label(ctx.emitter, "__rt_reference_cell_promote_mixed");
    abi::emit_store_reg_to_symbol(ctx.emitter, result, &slot.symbol, 0);
    abi::emit_load_int_immediate(ctx.emitter, result, 1);
    abi::emit_store_reg_to_symbol(ctx.emitter, result, &slot.init_symbol, 0);
    if release_unadopted_source {
        release_unadopted_static_initializer(ctx, value)?;
    }
    Ok(())
}

/// Retires an initializer only when no explicit release or published owner already owns cleanup.
fn release_unadopted_static_initializer(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    if !ctx.value_cleanup_is_deferred(value) {
        super::ownership::release_value_if_owned(ctx, value)?;
    }
    Ok(())
}

/// Resolves a local-slot immediate into static-local symbol metadata.
fn resolve_static_local_slot(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<StaticLocalSlot> {
    let slot = expect_local_slot(inst)?;
    let local = local_slot(ctx, slot)?;
    let name = local.name.clone().ok_or_else(|| {
        CodegenIrError::invalid_module(format!("{} static local is missing a source name", inst.op.name()))
    })?;
    let php_type = local.php_type.codegen_repr();
    let symbol = crate::names::static_local_symbol(&ctx.function.name, &name);
    let init_symbol = crate::names::static_local_init_symbol(&ctx.function.name, &name);
    ctx.data.add_comm(symbol.clone(), 16);
    ctx.data.add_comm(init_symbol.clone(), 8);
    // Record this static so the `--web` `__rt_web_reset` routine can release and
    // zero it between requests. Deduped by symbol inside the recorder, so the
    // repeated resolves on every load/store/init of this static cost nothing.
    ctx.data.record_static_local(crate::codegen::data_section::StaticLocalRecord {
        symbol: symbol.clone(),
        init_symbol: init_symbol.clone(),
        php_type: php_type.clone(),
        managed_reference: !ctx.function.flags.is_main,
    });
    Ok(StaticLocalSlot {
        name,
        php_type,
        symbol,
        init_symbol,
        managed_reference: !ctx.function.flags.is_main,
    })
}

/// Returns the EIR local slot metadata for one static-local instruction.
fn local_slot<'a>(
    ctx: &'a FunctionContext<'_>,
    slot: LocalSlotId,
) -> Result<&'a LocalSlot> {
    ctx.function
        .locals
        .get(slot.as_raw() as usize)
        .ok_or_else(|| CodegenIrError::missing_entry("local slot", slot.as_raw()))
}

/// Verifies that this backend slice knows how to represent the static-local type.
fn ensure_static_local_type_supported(slot: &StaticLocalSlot, inst: &Instruction) -> Result<()> {
    let ty = slot.php_type.codegen_repr();
    // A `?ptr<T>` static local is one pointer word (null is address 0).
    if matches!(
        ty,
        PhpType::Bool | PhpType::Int | PhpType::Float | PhpType::Str | PhpType::Void | PhpType::Pointer(_)
    ) || ty.is_refcounted()
    {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
            "{} for static local ${} with PHP type {:?}",
            inst.op.name(),
            slot.name,
            slot.php_type
    )))
}

/// Verifies the assigned value already has the static-local storage representation.
fn ensure_static_local_value_supported(
    ctx: &FunctionContext<'_>,
    slot: &StaticLocalSlot,
    value: ValueId,
    inst: &Instruction,
) -> Result<()> {
    if slot.managed_reference { return Ok(()); }
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    let slot_ty = slot.php_type.codegen_repr();
    if static_local_value_type_matches(&value_ty, &slot_ty) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} assigning PHP type {:?} to static local ${} with PHP type {:?}",
        inst.op.name(),
        value_ty,
        slot.name,
        slot.php_type
    )))
}

/// Returns true when a stored value can use the static-local symbol layout.
fn static_local_value_type_matches(value_ty: &PhpType, slot_ty: &PhpType) -> bool {
    if value_ty == slot_ty {
        return true;
    }
    // PHP coercive mode: Mixed (from checked arithmetic) can be narrowed to Int,
    // and Int/Bool/Void/Float can be boxed to Mixed for init.
    if matches!(slot_ty, PhpType::Int) && matches!(value_ty, PhpType::Mixed) {
        return true;
    }
    if matches!(slot_ty, PhpType::Mixed)
        && matches!(
            value_ty,
            PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Float | PhpType::Pointer(_) | PhpType::Object(_)
        )
    {
        return true;
    }
    // Any pointer fits a pointer slot once the checker accepted the assignment.
    if matches!((value_ty, slot_ty), (PhpType::Pointer(_), PhpType::Pointer(_))) {
        return true;
    }
    matches!(
        (value_ty, slot_ty),
        (PhpType::Array(value_elem), PhpType::Array(_))
            if matches!(value_elem.codegen_repr(), PhpType::Never | PhpType::Void)
    )
}

/// Clears the unused second word for non-string static-local storage.
fn clear_static_local_high_word_if_needed(ctx: &mut FunctionContext<'_>, slot: &StaticLocalSlot) {
    if !matches!(slot.php_type.codegen_repr(), PhpType::Str | PhpType::TaggedScalar) {
        abi::emit_store_zero_to_symbol(ctx.emitter, &slot.symbol, 8);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::{generate_user_asm_from_ir, platform::Target};
    use crate::ir::{
        Builder, Function, FunctionParam, IrHeapKind, IrType, LocalKind, Module, Op, Ownership,
        Terminator,
    };

    /// Managed initializers leave scoped source retirement to its single published owner cleanup.
    #[test]
    fn managed_static_initializer_preserves_scoped_source_until_owner_cleanup_on_every_target() {
        for name in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let target = Target::parse(name).unwrap();
            let module = managed_static_initializer_fixture(target);
            let assembly = generate_user_asm_from_ir(&module, false, false).unwrap();
            let body = assembly
                .split_once("@fn name=managed_static_initializer_fixture symbol=")
                .unwrap()
                .1
                .split_once("@endfn name=managed_static_initializer_fixture")
                .unwrap()
                .0;
            let label_offset = |needle: &str| {
                let mut offset = 0;
                for line in body.split_inclusive('\n') {
                    if line.trim_end().ends_with(':') && line.contains(needle) {
                        return Some(offset);
                    }
                    offset += line.len();
                }
                None
            };
            let initialized_label = label_offset("static_local_initialized").unwrap();
            let done_label = label_offset("static_local_init_done").unwrap();
            let uninitialized_arm = &body[..initialized_label];
            let recursive_loser_arm = &body[initialized_label..done_label];
            let scoped_cleanup = &body[done_label..];
            let promoted = uninitialized_arm
                .find("__rt_reference_cell_promote_mixed")
                .unwrap();
            let marker = uninitialized_arm[promoted..]
                .find(&crate::names::static_local_init_symbol(
                    "managed_static_initializer_fixture",
                    "value",
                ))
                .map(|offset| promoted + offset)
                .unwrap();
            assert!(
                promoted < marker,
                "{name}: the managed static must be promoted before publication: {uninitialized_arm}"
            );
            assert!(
                !uninitialized_arm[marker..].contains("__rt_decref_object"),
                "{name}: successful publication must leave cleanup to the scoped owner"
            );
            assert!(
                !recursive_loser_arm.contains("__rt_decref_object"),
                "{name}: the recursive loser must not duplicate its scoped cleanup"
            );
            assert!(
                scoped_cleanup.contains("__rt_decref_object"),
                "{name}: the common owner cleanup must still retire the staged object"
            );
        }
    }

    /// A transient concat copied into managed static storage retires its unadopted buffer last.
    #[test]
    fn transient_static_initializer_releases_source_after_publish_on_every_target() {
        let source = r#"<?php
function staticTransientPart(): string { return "part"; }
function transientStaticInitializer(): string {
    static $value = staticTransientPart() . "-value";
    return $value;
}
transientStaticInitializer();
"#;
        for name in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let target = Target::parse(name).unwrap();
            let module = crate::ir_lower::tests::lower_source_at_for_target(
                source,
                std::path::Path::new("main.php"),
                std::path::Path::new("."),
                target,
            );
            let assembly = generate_user_asm_from_ir(&module, false, false).unwrap();
            let body = assembly
                .split_once("@fn name=transientStaticInitializer symbol=")
                .unwrap()
                .1
                .split_once("@endfn name=transientStaticInitializer")
                .unwrap()
                .0;
            let boxed = body.find("__rt_mixed_from_value").unwrap();
            let marker = body[boxed..]
                .find(&crate::names::static_local_init_symbol(
                    "transientStaticInitializer",
                    "value",
                ))
                .map(|offset| boxed + offset)
                .unwrap();
            let released = body[boxed..]
                .find("__rt_heap_free_safe")
                .map(|offset| boxed + offset)
                .unwrap();
            assert!(
                boxed < marker && marker < released,
                "{name}: publish the copied concat before retiring its transient buffer: {body}"
            );
        }
    }

    /// Builds an owned object whose published operand slot prevents direct transfer.
    fn managed_static_initializer_fixture(target: Target) -> Module {
        let object = PhpType::Object("ManagedStaticOwner".into());
        let object_ir = IrType::Heap(IrHeapKind::Object);
        let mixed_ir = IrType::Heap(IrHeapKind::Mixed);
        let mut module = Module::new(target);
        let mut function = Function::new(
            "managed_static_initializer_fixture".into(),
            IrType::Void,
            PhpType::Void,
        );
        function.params.push(FunctionParam {
            name: "source".into(),
            ir_type: object_ir,
            php_type: object.clone(),
            by_ref: false,
            variadic: false,
        });
        let source = function.add_local(
            Some("source".into()),
            object_ir,
            object.clone(),
            LocalKind::PhpLocal,
        );
        let operand_owner = function.add_local(
            Some("operand_owner".into()),
            object_ir,
            object.clone(),
            LocalKind::OwnedTemp,
        );
        let persistent = function.add_local(
            Some("value".into()),
            mixed_ir,
            PhpType::Mixed,
            LocalKind::StaticLocal,
        );
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let borrowed = builder.emit_load_local(source, object_ir, object.clone());
            let owned = builder
                .emit(
                    Op::Acquire,
                    vec![borrowed],
                    None,
                    object_ir,
                    object,
                    Ownership::Owned,
                )
                .unwrap();
            builder.emit(
                Op::StoreLocal,
                vec![owned],
                Some(crate::ir::Immediate::LocalSlot(operand_owner)),
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            builder.emit(
                Op::PushCallOperandOwner,
                Vec::new(),
                Some(crate::ir::Immediate::LocalSlot(operand_owner)),
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            builder.emit(
                Op::InitStaticLocal,
                vec![owned],
                Some(crate::ir::Immediate::LocalSlot(persistent)),
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            builder.emit(
                Op::PopCallOperandOwner,
                Vec::new(),
                Some(crate::ir::Immediate::LocalSlot(operand_owner)),
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            builder.emit(
                Op::ReleaseLocalSlot,
                Vec::new(),
                Some(crate::ir::Immediate::LocalSlot(operand_owner)),
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);

        let mut main = Function::new("main".into(), IrType::Void, PhpType::Void);
        main.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut main);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(main);
        crate::ir::validate_module(&module).expect("valid managed static initializer fixture");
        module
    }
}
