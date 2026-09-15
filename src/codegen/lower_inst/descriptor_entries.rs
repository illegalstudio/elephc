//! Purpose:
//! Emits descriptor entry wrappers and their ABI argument layouts.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// Emits an entry wrapper that receives visible args followed by a captured called-class id.
pub(super) fn emit_static_late_bound_descriptor_entry_wrapper(
    ctx: &mut FunctionContext<'_>,
    impl_class: &str,
    method_key: &str,
    sig: &FunctionSig,
    dynamic_slot: Option<usize>,
) -> Result<String> {
    let wrapper_label = ctx.next_label("static_late_bound_descriptor_entry");
    let done_label = ctx.next_label("static_late_bound_descriptor_entry_done");
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&wrapper_label);
    emit_static_late_bound_descriptor_entry_wrapper_body(
        ctx,
        impl_class,
        method_key,
        sig,
        dynamic_slot,
    );
    ctx.emitter.label(&done_label);
    Ok(wrapper_label)
}

/// Emits an entry wrapper that prepends a concrete called-class id before calling a static method.
pub(in crate::codegen) fn emit_static_method_descriptor_entry_wrapper(
    ctx: &mut FunctionContext<'_>,
    impl_class: &str,
    method_key: &str,
    sig: &FunctionSig,
    called_class_id: u64,
) -> Result<String> {
    let visible_arg_types = descriptor_visible_arg_types(sig);
    let wrapper_label = ctx.next_label("static_method_descriptor_entry");
    let done_label = ctx.next_label("static_method_descriptor_entry_done");
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&wrapper_label);
    emit_static_method_descriptor_entry_wrapper_body(
        ctx,
        impl_class,
        method_key,
        &visible_arg_types,
        called_class_id,
    );
    ctx.emitter.label(&done_label);
    Ok(wrapper_label)
}

/// Emits an entry wrapper that receives visible args followed by the captured receiver.
pub(super) fn emit_instance_method_descriptor_entry_wrapper(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    method_key: &str,
    sig: &FunctionSig,
) -> Result<String> {
    let visible_arg_types = descriptor_visible_arg_types(sig);
    let wrapper_label = ctx.next_label("callable_instance_method");
    let done_label = ctx.next_label("callable_instance_method_done");
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&wrapper_label);
    emit_instance_method_descriptor_entry_wrapper_body(
        ctx,
        class_name,
        method_key,
        &visible_arg_types,
    );
    ctx.emitter.label(&done_label);
    Ok(wrapper_label)
}

/// Returns codegen-representation parameter types for a descriptor entry wrapper.
pub(super) fn descriptor_visible_arg_types(sig: &FunctionSig) -> Vec<PhpType> {
    sig.params.iter().map(|(_, ty)| ty.codegen_repr()).collect()
}

/// Emits a descriptor entry wrapper body by reordering visible args after the receiver.
pub(super) fn emit_instance_method_descriptor_entry_wrapper_body(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    method_key: &str,
    visible_arg_types: &[PhpType],
) {
    let receiver_ty = descriptor_receiver_type(class_name);
    let incoming_types = descriptor_entry_incoming_types(visible_arg_types, &receiver_ty);
    let actual_types = descriptor_entry_actual_types(visible_arg_types, &receiver_ty);
    let incoming_assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, &incoming_types, 0);
    let actual_assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, &actual_types, 0);
    let (incoming_stack_offsets, _) = descriptor_entry_stack_offsets(&incoming_assignments);
    let (actual_stack_offsets, actual_overflow_bytes) =
        descriptor_entry_stack_offsets(&actual_assignments);
    let frame_size = descriptor_entry_frame_size(incoming_types.len());

    abi::emit_frame_prologue(ctx.emitter, frame_size);
    for (idx, (ty, assignment)) in incoming_types
        .iter()
        .zip(incoming_assignments.iter())
        .enumerate()
    {
        store_descriptor_entry_incoming_arg(
            ctx.emitter,
            ty,
            assignment,
            descriptor_entry_slot_offset(idx),
            incoming_stack_offsets[idx],
        );
    }
    if actual_overflow_bytes > 0 {
        abi::emit_reserve_temporary_stack(ctx.emitter, actual_overflow_bytes);
    }
    for (idx, (ty, assignment)) in actual_types
        .iter()
        .zip(actual_assignments.iter())
        .enumerate()
    {
        let source_idx = if idx == 0 {
            visible_arg_types.len()
        } else {
            idx - 1
        };
        load_descriptor_entry_actual_arg(
            ctx.emitter,
            ty,
            assignment,
            descriptor_entry_slot_offset(source_idx),
            actual_stack_offsets[idx],
        );
    }
    abi::emit_call_label(ctx.emitter, &method_symbol(class_name, method_key));
    if actual_overflow_bytes > 0 {
        abi::emit_release_temporary_stack(ctx.emitter, actual_overflow_bytes);
    }
    abi::emit_frame_restore(ctx.emitter, frame_size);
    abi::emit_return(ctx.emitter);
}

/// Emits a static descriptor entry wrapper body by prepending a constant class id.
pub(super) fn emit_static_method_descriptor_entry_wrapper_body(
    ctx: &mut FunctionContext<'_>,
    impl_class: &str,
    method_key: &str,
    visible_arg_types: &[PhpType],
    called_class_id: u64,
) {
    let actual_types = {
        let mut types = Vec::with_capacity(visible_arg_types.len() + 1);
        types.push(PhpType::Int);
        types.extend_from_slice(visible_arg_types);
        types
    };
    let incoming_assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, visible_arg_types, 0);
    let actual_assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, &actual_types, 0);
    let (incoming_stack_offsets, _) = descriptor_entry_stack_offsets(&incoming_assignments);
    let (actual_stack_offsets, actual_overflow_bytes) =
        descriptor_entry_stack_offsets(&actual_assignments);
    let frame_size = descriptor_entry_frame_size(visible_arg_types.len());

    abi::emit_frame_prologue(ctx.emitter, frame_size);
    for (idx, (ty, assignment)) in visible_arg_types
        .iter()
        .zip(incoming_assignments.iter())
        .enumerate()
    {
        store_descriptor_entry_incoming_arg(
            ctx.emitter,
            ty,
            assignment,
            descriptor_entry_slot_offset(idx),
            incoming_stack_offsets[idx],
        );
    }
    if actual_overflow_bytes > 0 {
        abi::emit_reserve_temporary_stack(ctx.emitter, actual_overflow_bytes);
    }
    for (idx, (ty, assignment)) in actual_types
        .iter()
        .zip(actual_assignments.iter())
        .enumerate()
    {
        if idx == 0 {
            load_descriptor_entry_static_class_id(
                ctx.emitter,
                called_class_id,
                assignment,
                actual_stack_offsets[idx],
            );
        } else {
            load_descriptor_entry_actual_arg(
                ctx.emitter,
                ty,
                assignment,
                descriptor_entry_slot_offset(idx - 1),
                actual_stack_offsets[idx],
            );
        }
    }
    abi::emit_call_label(ctx.emitter, &static_method_symbol(impl_class, method_key));
    if actual_overflow_bytes > 0 {
        abi::emit_release_temporary_stack(ctx.emitter, actual_overflow_bytes);
    }
    abi::emit_frame_restore(ctx.emitter, frame_size);
    abi::emit_return(ctx.emitter);
}

/// Emits a static descriptor entry wrapper body by prepending the called-class id.
pub(super) fn emit_static_late_bound_descriptor_entry_wrapper_body(
    ctx: &mut FunctionContext<'_>,
    impl_class: &str,
    method_key: &str,
    sig: &FunctionSig,
    dynamic_slot: Option<usize>,
) {
    let visible_arg_types = descriptor_visible_arg_types(sig);
    let called_class_ty = PhpType::Int;
    let incoming_types = descriptor_entry_incoming_types(&visible_arg_types, &called_class_ty);
    let actual_types = descriptor_entry_actual_types(&visible_arg_types, &called_class_ty);
    let incoming_assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, &incoming_types, 0);
    let actual_assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, &actual_types, 0);
    let (incoming_stack_offsets, _) = descriptor_entry_stack_offsets(&incoming_assignments);
    let (actual_stack_offsets, actual_overflow_bytes) =
        descriptor_entry_stack_offsets(&actual_assignments);
    let frame_size = descriptor_entry_frame_size(incoming_types.len());

    abi::emit_frame_prologue(ctx.emitter, frame_size);
    for (idx, (ty, assignment)) in incoming_types
        .iter()
        .zip(incoming_assignments.iter())
        .enumerate()
    {
        store_descriptor_entry_incoming_arg(
            ctx.emitter,
            ty,
            assignment,
            descriptor_entry_slot_offset(idx),
            incoming_stack_offsets[idx],
        );
    }
    if actual_overflow_bytes > 0 {
        abi::emit_reserve_temporary_stack(ctx.emitter, actual_overflow_bytes);
    }
    for (idx, (ty, assignment)) in actual_types
        .iter()
        .zip(actual_assignments.iter())
        .enumerate()
    {
        let source_idx = if idx == 0 {
            visible_arg_types.len()
        } else {
            idx - 1
        };
        load_descriptor_entry_actual_arg(
            ctx.emitter,
            ty,
            assignment,
            descriptor_entry_slot_offset(source_idx),
            actual_stack_offsets[idx],
        );
    }
    if let Some(slot) = dynamic_slot {
        emit_dynamic_static_method_call(ctx, slot);
    } else {
        abi::emit_call_label(ctx.emitter, &static_method_symbol(impl_class, method_key));
    }
    if actual_overflow_bytes > 0 {
        abi::emit_release_temporary_stack(ctx.emitter, actual_overflow_bytes);
    }
    if sig.return_type.codegen_repr() == PhpType::Str && !sig.by_ref_return {
        emit_late_static_owned_string_result(
            ctx, impl_class, method_key, dynamic_slot,
            descriptor_entry_slot_offset(visible_arg_types.len()),
        );
    }
    abi::emit_frame_restore(ctx.emitter, frame_size);
    abi::emit_return(ctx.emitter);
}

/// Gives every selected static implementation one owned result without copying an owned string.
/// Dynamic overrides select their own ownership proof using the saved called-class id.
fn emit_late_static_owned_string_result(
    ctx: &mut FunctionContext<'_>,
    impl_class: &str,
    method_key: &str,
    dynamic_slot: Option<usize>,
    called_class_offset: usize,
) {
    use crate::codegen::runtime_callable_invoker::method_returns_owned_string;
    if dynamic_slot.is_none() {
        if !method_returns_owned_string(ctx.module, impl_class, method_key, true) {
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
        }
        return;
    }
    let mut owned_classes: Vec<_> = ctx.module.class_infos.iter().filter_map(|(name, info)| {
        let implementation = info.static_method_impl_classes.get(method_key)
            .map(String::as_str).unwrap_or(name.as_str());
        method_returns_owned_string(ctx.module, implementation, method_key, true)
            .then_some(info.class_id)
    }).collect();
    owned_classes.sort_unstable();
    owned_classes.dedup();
    ctx.emitter.comment("normalize late-bound static descriptor string ownership");
    let owned = ctx.next_label("static_descriptor_string_owned");
    let class_reg = abi::secondary_scratch_reg(ctx.emitter);
    let expected_reg = abi::tertiary_scratch_reg(ctx.emitter);
    abi::load_at_offset(ctx.emitter, class_reg, called_class_offset);
    for class_id in owned_classes {
        abi::emit_load_int_immediate(ctx.emitter, expected_reg, class_id as i64);
        ctx.emitter.instruction(&format!("cmp {class_reg}, {expected_reg}"));   // compare the saved called class without overwriting the string pair
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("b.eq {owned}"));              // keep the selected implementation's existing owned string
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("je {owned}"));                // keep the selected implementation's existing owned string
            }
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    ctx.emitter.label(&owned);
}

/// Loads the concrete called-class id into a descriptor wrapper's outgoing ABI slot.
pub(super) fn load_descriptor_entry_static_class_id(
    emitter: &mut crate::codegen::emit::Emitter,
    class_id: u64,
    assignment: &abi::OutgoingArgAssignment,
    stack_offset: Option<usize>,
) {
    let reg = if assignment.in_register() {
        abi::int_arg_reg_name(emitter.target, assignment.start_reg)
    } else {
        abi::secondary_scratch_reg(emitter)
    };
    abi::emit_load_int_immediate(emitter, reg, class_id as i64);
    if let Some(out_offset) = stack_offset {
        abi::emit_store_to_sp(emitter, reg, out_offset);
    }
}

/// Returns the runtime receiver type threaded through the descriptor entry wrapper.
pub(super) fn descriptor_receiver_type(class_name: &str) -> PhpType {
    PhpType::Object(class_name.to_string())
}

/// Returns the wrapper incoming argument order: visible args followed by receiver.
pub(super) fn descriptor_entry_incoming_types(
    visible_arg_types: &[PhpType],
    receiver_ty: &PhpType,
) -> Vec<PhpType> {
    let mut types = visible_arg_types.to_vec();
    types.push(receiver_ty.clone());
    types
}

/// Returns the real method ABI argument order: receiver followed by visible args.
pub(super) fn descriptor_entry_actual_types(
    visible_arg_types: &[PhpType],
    receiver_ty: &PhpType,
) -> Vec<PhpType> {
    let mut types = Vec::with_capacity(visible_arg_types.len() + 1);
    types.push(receiver_ty.clone());
    types.extend_from_slice(visible_arg_types);
    types
}

/// Returns an aligned frame size for descriptor entry wrapper spill slots plus footer.
pub(super) fn descriptor_entry_frame_size(slot_count: usize) -> usize {
    align16((slot_count + 1) * 16)
}

/// Returns the frame offset for a descriptor entry wrapper spill slot.
pub(super) fn descriptor_entry_slot_offset(idx: usize) -> usize {
    (idx + 1) * 16
}

/// Returns the local/outgoing byte size used for one descriptor wrapper argument.
pub(super) fn descriptor_entry_arg_slot_size(ty: &PhpType) -> usize {
    match ty.codegen_repr() {
        PhpType::Void | PhpType::Never => 0,
        _ => 16,
    }
}

/// Returns stack offsets for ABI assignments that overflow their target registers.
pub(super) fn descriptor_entry_stack_offsets(
    assignments: &[abi::OutgoingArgAssignment],
) -> (Vec<Option<usize>>, usize) {
    let mut offsets = vec![None; assignments.len()];
    let mut next_offset = 0usize;
    for (idx, assignment) in assignments.iter().enumerate() {
        if assignment.in_register() {
            continue;
        }
        offsets[idx] = Some(next_offset);
        next_offset += descriptor_entry_arg_slot_size(&assignment.ty);
    }
    (offsets, next_offset)
}
