//! Purpose:
//! Dispatches runtime-name property probes and reads through user magic accessors.
//!
//! Called from:
//! - Runtime dynamic-property reads on boxed Mixed receivers.
//!
//! Key details:
//! - A real per-instance hash entry takes precedence over `__isset` and `__get`.
//! - The staged receiver and runtime string key remain available across both accessor calls.
//! - Every successful or absent arm releases the caller's temporary stack frame.

use super::*;

/// One runtime class that can answer an unknown property name through magic accessors.
pub(super) struct RuntimeMagicReadArm {
    /// Accessor metadata and class id used by the dispatch ladder.
    pub(super) magic: MixedMagicArm,
    /// The class's per-instance dynamic hash, when it has one.
    pub(super) hash_offset: Option<usize>,
}

/// Collects all user classes whose accessors can answer the requested fetch mode.
pub(super) fn runtime_magic_read_arms(
    ctx: &mut FunctionContext<'_>,
    mode: PropertyFetchMode,
) -> Result<Vec<RuntimeMagicReadArm>> {
    let mut names = ctx
        .module
        .class_infos
        .iter()
        .filter(|(name, _)| !crate::types::checker::builtin_stdclass::is_stdclass(name))
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    names.sort();
    let mut arms = Vec::new();
    for name in names {
        let Some(magic) = magic_property_arm(ctx, &name, mode)? else {
            continue;
        };
        let class_info = &ctx.module.class_infos[&name];
        let hash_offset = class_info
            .dynamic_property_hash_is_name_addressable()
            .then(|| dynamic_property_hash_offset(class_info.properties.len()));
        arms.push(RuntimeMagicReadArm { magic, hash_offset });
    }
    arms.sort_by_key(|arm| arm.magic.class_id);
    Ok(arms)
}

/// Collects runtime-name dynamic-hash arms not already owned by another class dispatch.
pub(super) fn runtime_class_hash_arms(
    ctx: &FunctionContext<'_>,
    dispatched: &[u64],
) -> Vec<MixedClassHashArm> {
    let mut arms = Vec::new();
    for (class_name, class_info) in &ctx.module.class_infos {
        if crate::types::checker::builtin_stdclass::is_stdclass(class_name)
            || dispatched.contains(&class_info.class_id)
            || !class_info.dynamic_property_hash_is_name_addressable()
        {
            continue;
        }
        arms.push(MixedClassHashArm {
            class_id: class_info.class_id,
            class_name: class_name.clone(),
            hash_offset: dynamic_property_hash_offset(class_info.properties.len()),
        });
    }
    arms.sort_by_key(|arm| arm.class_id);
    arms
}

/// Reads a stacked runtime name from a class hash, then falls back to its magic accessors.
pub(super) fn emit_runtime_magic_property_arm(
    ctx: &mut FunctionContext<'_>,
    arm: &MixedMagicArm,
    hash_offset: Option<usize>,
    receiver_offset: usize,
    name_offset: usize,
    frame_bytes: usize,
) -> Result<()> {
    let found = ctx.next_label("mixed_dyn_magic_hash_found");
    let accessor = ctx.next_label("mixed_dyn_magic_accessor");
    let done = ctx.next_label("mixed_dyn_magic_done");
    if let Some(hash_offset) = hash_offset {
        emit_runtime_hash_probe(
            ctx,
            hash_offset,
            receiver_offset,
            name_offset,
            &found,
        );
    }
    abi::emit_jump(ctx.emitter, &accessor);

    ctx.emitter.label(&found);
    let payload = if ctx.emitter.target.arch == Arch::AArch64 {
        "x1"
    } else {
        "rdi"
    };
    abi::emit_reg_move(ctx.emitter, abi::int_result_reg(ctx.emitter), payload);
    abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Mixed);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&accessor);
    emit_runtime_magic_accessors(ctx, arm, receiver_offset, name_offset, &done)?;

    ctx.emitter.label(&done);
    abi::emit_release_temporary_stack(ctx.emitter, frame_bytes);
    Ok(())
}

/// Calls `__isset` and `__get` for one runtime key, leaving an owned Mixed result.
fn emit_runtime_magic_accessors(
    ctx: &mut FunctionContext<'_>,
    arm: &MixedMagicArm,
    receiver_offset: usize,
    name_offset: usize,
    done_label: &str,
) -> Result<()> {
    if arm.absent_without_isset {
        emit_boxed_null(ctx);
        return Ok(());
    }
    if let Some(isset) = &arm.isset {
        let absent = ctx.next_label("mixed_dyn_magic_absent");
        emit_runtime_magic_accessor_args(ctx, receiver_offset, name_offset);
        emit_resolved_method_call(ctx, isset)?;
        if isset.by_ref_return {
            preserve_and_release_reference_bool(ctx);
        }
        abi::emit_branch_if_int_result_zero(ctx.emitter, &absent);
        if arm.isset_only {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
        } else if let Some(get) = &arm.get {
            emit_runtime_magic_accessor_args(ctx, receiver_offset, name_offset);
            emit_resolved_method_call(ctx, get)?;
            box_magic_result(ctx, get);
        } else {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
        }
        abi::emit_jump(ctx.emitter, done_label);
        ctx.emitter.label(&absent);
        emit_boxed_null(ctx);
        return Ok(());
    }
    if let Some(get) = &arm.get {
        emit_runtime_magic_accessor_args(ctx, receiver_offset, name_offset);
        emit_resolved_method_call(ctx, get)?;
        box_magic_result(ctx, get);
    } else {
        emit_boxed_null(ctx);
    }
    Ok(())
}

/// Probes a class's dynamic hash using the receiver and name in the staged frame.
fn emit_runtime_hash_probe(
    ctx: &mut FunctionContext<'_>,
    hash_offset: usize,
    receiver_offset: usize,
    name_offset: usize,
    found_label: &str,
) {
    let target = ctx.emitter.target;
    let object = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object, receiver_offset);
    abi::emit_load_from_address(
        ctx.emitter,
        abi::int_arg_reg_name(target, 0),
        object,
        hash_offset,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(target, 1),
        name_offset,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(target, 2),
        name_offset + 8,
    );
    abi::emit_call_label(ctx.emitter, "__rt_hash_get");
    emit_branch_if_hash_entry_found(ctx, found_label);
}

/// Loads `(this, string $name)` from the staged runtime-name frame.
fn emit_runtime_magic_accessor_args(
    ctx: &mut FunctionContext<'_>,
    receiver_offset: usize,
    name_offset: usize,
) {
    let target = ctx.emitter.target;
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(target, 0),
        receiver_offset,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(target, 1),
        name_offset,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(target, 2),
        name_offset + 8,
    );
}

/// Loads a by-reference bool result and retires the transferred reference cell.
fn preserve_and_release_reference_bool(ctx: &mut FunctionContext<'_>) {
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result);
    abi::emit_load_from_address(ctx.emitter, result, result, 0);
    abi::emit_push_reg(ctx.emitter, result);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 16);
    abi::emit_call_label(ctx.emitter, "__rt_reference_cell_owner");
    abi::emit_call_label(ctx.emitter, "__rt_reference_cell_release");
    abi::emit_pop_reg(ctx.emitter, result);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
}
