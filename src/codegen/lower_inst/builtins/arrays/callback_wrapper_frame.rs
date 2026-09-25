//! Purpose:
//! Callback ABI frame layout, argument boxing, and cleanup.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays`.
//!
//! Key details:
//! - Preserves callback ABI, target parity, array storage, and ownership contracts.

use super::*;
use crate::codegen_support::platform::Platform;

/// Stack frame layout used by generated callback ABI adapters.
pub(super) struct CallbackWrapperFrame {
    pub(super) hidden_offset: usize,
    pub(super) raw_offsets: Vec<usize>,
    pub(super) boxed_offsets: Vec<usize>,
    pub(super) return_offset: usize,
    pub(super) return_address_offset: usize,
    pub(super) total_bytes: usize,
}

/// Builds the stack layout for one generated callback wrapper.
pub(super) fn callback_wrapper_frame(param_types: &[PhpType], visible_arg_types: &[PhpType]) -> CallbackWrapperFrame {
    let mut offset = 0usize;
    let hidden_offset = offset;
    offset += 8;
    let mut raw_offsets = Vec::with_capacity(visible_arg_types.len());
    for ty in visible_arg_types {
        raw_offsets.push(offset);
        offset += callback_visible_arg_slot_size(ty);
    }
    let mut boxed_offsets = Vec::with_capacity(visible_arg_types.len());
    for (param_ty, visible_ty) in param_types.iter().zip(visible_arg_types.iter()) {
        if callback_arg_needs_mixed_box(param_ty, visible_ty) {
            boxed_offsets.push(offset);
            offset += 8;
        } else {
            boxed_offsets.push(NO_CALLBACK_BOX_OFFSET);
        }
    }
    let return_offset = offset;
    offset += 16;
    let return_address_offset = offset;
    offset += 8;
    CallbackWrapperFrame {
        hidden_offset,
        raw_offsets,
        boxed_offsets,
        return_offset,
        return_address_offset,
        total_bytes: align_callback_frame_bytes(offset),
    }
}

/// Rounds a wrapper frame size up to the stack alignment required before calls.
pub(super) fn align_callback_frame_bytes(bytes: usize) -> usize {
    (bytes + 15) & !15
}

/// Returns the stack bytes needed to preserve one incoming runtime callback argument.
pub(super) fn callback_visible_arg_slot_size(ty: &PhpType) -> usize {
    if matches!(ty.codegen_repr(), PhpType::Str) {
        16
    } else {
        8
    }
}

/// Returns true when the target method parameter needs a boxed Mixed argument.
pub(super) fn callback_arg_needs_mixed_box(param_ty: &PhpType, visible_ty: &PhpType) -> bool {
    param_ty.codegen_repr() == PhpType::Mixed
        && !matches!(visible_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
}

/// Returns true when any callback argument must be boxed for the target signature.
pub(super) fn callback_wrapper_has_boxed_args(frame: &CallbackWrapperFrame) -> bool {
    frame
        .boxed_offsets
        .iter()
        .any(|offset| *offset != NO_CALLBACK_BOX_OFFSET)
}

/// Counts integer ABI registers consumed by the visible callback argument list.
pub(super) fn callback_arg_abi_slots(visible_arg_types: &[PhpType]) -> usize {
    visible_arg_types
        .iter()
        .map(|ty| {
            if matches!(ty.codegen_repr(), PhpType::Str) {
                2
            } else {
                1
            }
        })
        .sum()
}

/// Loads the hidden callback environment after the visible raw callback words.
///
/// Windows runtime helpers cross from their SysV-shaped implementation ABI into
/// generated callbacks through `Emitter::emit_platform_callback_call`.  That
/// transition uses the native MSx64 positional layout: after `rcx`, `rdx`, `r8`,
/// and `r9`, the environment lives at `rbp + 48` and every later raw word advances
/// by eight bytes.  In particular, a two-string comparator consumes all four
/// registers and its environment is the fifth word on the caller stack.
pub(super) fn load_callback_environment_to_reg(
    ctx: &mut FunctionContext<'_>,
    visible_arg_types: &[PhpType],
    destination: &str,
) {
    let environment_word = callback_arg_abi_slots(visible_arg_types);
    if (ctx.emitter.target.platform, ctx.emitter.target.arch) == (Platform::Windows, Arch::X86_64)
        && environment_word >= 4
    {
        let stack_offset = 48 + (environment_word - 4) * 8;
        abi::load_from_caller_stack(ctx.emitter, destination, stack_offset);
        return;
    }

    let source = abi::int_arg_reg_name(ctx.emitter.target, environment_word);
    if source != destination {
        ctx.emitter.instruction(&format!("mov {}, {}", destination, source));   // preserve the in-register callback environment while wrapper setup reuses argument registers
    }
}

/// Saves the incoming runtime callback arguments before nested boxing calls can clobber them.
pub(super) fn save_callback_visible_args(
    ctx: &mut FunctionContext<'_>,
    frame: &CallbackWrapperFrame,
    visible_arg_types: &[PhpType],
) {
    if (ctx.emitter.target.platform, ctx.emitter.target.arch) == (Platform::Windows, Arch::X86_64) {
        save_windows_callback_visible_args(ctx, frame, visible_arg_types);
        return;
    }

    let mut reg_index = 0usize;
    for (index, ty) in visible_arg_types.iter().enumerate() {
        let offset = frame.raw_offsets[index];
        if matches!(ty.codegen_repr(), PhpType::Str) {
            let ptr_reg = abi::int_arg_reg_name(ctx.emitter.target, reg_index);
            let len_reg = abi::int_arg_reg_name(ctx.emitter.target, reg_index + 1);
            abi::emit_store_to_sp(ctx.emitter, ptr_reg, offset);
            abi::emit_store_to_sp(ctx.emitter, len_reg, offset + 8);
            reg_index += 2;
        } else {
            let reg = abi::int_arg_reg_name(ctx.emitter.target, reg_index);
            abi::emit_store_to_sp(ctx.emitter, reg, offset);
            reg_index += 1;
        }
    }
}

/// Saves raw MSx64 callback words from positional registers or overflow slots.
///
/// Unlike the generated PHP ABI, the runtime-to-callback bridge treats a string
/// as two adjacent native words.  Keep the two words independently addressable so
/// a string beginning in `r9` can continue at the first caller-stack slot.
fn save_windows_callback_visible_args(
    ctx: &mut FunctionContext<'_>,
    frame: &CallbackWrapperFrame,
    visible_arg_types: &[PhpType],
) {
    let mut word_index = 0usize;
    for (index, ty) in visible_arg_types.iter().enumerate() {
        let offset = frame.raw_offsets[index];
        save_windows_callback_word(ctx, word_index, offset);
        word_index += 1;
        if matches!(ty.codegen_repr(), PhpType::Str) {
            save_windows_callback_word(ctx, word_index, offset + 8);
            word_index += 1;
        }
    }
}

/// Saves one raw callback word from its MSx64 register or caller-stack location.
fn save_windows_callback_word(
    ctx: &mut FunctionContext<'_>,
    word_index: usize,
    destination_offset: usize,
) {
    if word_index < 4 {
        let source = abi::int_arg_reg_name(ctx.emitter.target, word_index);
        abi::emit_store_to_sp(ctx.emitter, source, destination_offset);
        return;
    }

    let stack_offset = 48 + (word_index - 4) * 8;
    abi::load_from_caller_stack(ctx.emitter, "r10", stack_offset);
    abi::emit_store_to_sp(ctx.emitter, "r10", destination_offset);
}

/// Saves an already materialized hidden receiver or called-class id into the wrapper frame.
pub(super) fn save_callback_hidden_arg(ctx: &mut FunctionContext<'_>, frame: &CallbackWrapperFrame, reg: &str) {
    abi::emit_store_to_sp(ctx.emitter, reg, frame.hidden_offset);
}

/// Boxes visible runtime arguments whose target method parameters are `Mixed`.
pub(super) fn box_callback_mixed_args(
    ctx: &mut FunctionContext<'_>,
    frame: &CallbackWrapperFrame,
    param_types: &[PhpType],
    visible_arg_types: &[PhpType],
) {
    for (index, (param_ty, visible_ty)) in param_types.iter().zip(visible_arg_types.iter()).enumerate() {
        let box_offset = frame.boxed_offsets[index];
        if box_offset == NO_CALLBACK_BOX_OFFSET || !callback_arg_needs_mixed_box(param_ty, visible_ty) {
            continue;
        }
        load_callback_raw_arg_to_result(ctx, frame.raw_offsets[index], visible_ty);
        emit_box_current_value_as_mixed(ctx.emitter, &visible_ty.codegen_repr());
        abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), box_offset);
    }
}

/// Loads a saved runtime callback argument into the normal result registers.
pub(super) fn load_callback_raw_arg_to_result(
    ctx: &mut FunctionContext<'_>,
    raw_offset: usize,
    visible_ty: &PhpType,
) {
    if matches!(visible_ty.codegen_repr(), PhpType::Str) {
        let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
        abi::emit_load_temporary_stack_slot(ctx.emitter, ptr_reg, raw_offset);
        abi::emit_load_temporary_stack_slot(ctx.emitter, len_reg, raw_offset + 8);
    } else {
        abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), raw_offset);
    }
}

/// Loads the hidden receiver/class id and visible method parameters into callee ABI registers.
pub(super) fn load_callback_target_args(
    ctx: &mut FunctionContext<'_>,
    frame: &CallbackWrapperFrame,
    visible_arg_types: &[PhpType],
) -> usize {
    let target_arg_types = callback_target_arg_types(frame, visible_arg_types);
    let assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, &target_arg_types, 0);
    if assignments.iter().any(|assignment| !assignment.in_register()) {
        stage_callback_target_args(ctx, frame, visible_arg_types);
        return abi::materialize_outgoing_args(ctx.emitter, &assignments);
    }

    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        frame.hidden_offset,
    );
    let mut reg_index = 1usize;
    for (index, visible_ty) in visible_arg_types.iter().enumerate() {
        let box_offset = frame.boxed_offsets[index];
        if box_offset != NO_CALLBACK_BOX_OFFSET {
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, reg_index),
                box_offset,
            );
            reg_index += 1;
        } else if matches!(visible_ty.codegen_repr(), PhpType::Str) {
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, reg_index),
                frame.raw_offsets[index],
            );
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, reg_index + 1),
                frame.raw_offsets[index] + 8,
            );
            reg_index += 2;
        } else {
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, reg_index),
                frame.raw_offsets[index],
            );
            reg_index += 1;
        }
    }
    0
}

/// Returns the actual generated-PHP ABI types after wrapper-only Mixed boxing.
fn callback_target_arg_types(
    frame: &CallbackWrapperFrame,
    visible_arg_types: &[PhpType],
) -> Vec<PhpType> {
    let mut types = Vec::with_capacity(visible_arg_types.len() + 1);
    types.push(PhpType::Int);
    for (index, visible_ty) in visible_arg_types.iter().enumerate() {
        if frame.boxed_offsets[index] != NO_CALLBACK_BOX_OFFSET {
            types.push(PhpType::Mixed);
        } else {
            types.push(visible_ty.codegen_repr());
        }
    }
    types
}

/// Stages the hidden receiver/class id and visible values for the regular
/// generated-PHP outgoing argument materializer.
fn stage_callback_target_args(
    ctx: &mut FunctionContext<'_>,
    frame: &CallbackWrapperFrame,
    visible_arg_types: &[PhpType],
) {
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        frame.hidden_offset,
    );
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter)); // stage the hidden receiver/class id before callback method arguments

    for (index, visible_ty) in visible_arg_types.iter().enumerate() {
        let box_offset = frame.boxed_offsets[index];
        if box_offset != NO_CALLBACK_BOX_OFFSET {
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                box_offset,
            );
            abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter)); // stage the wrapper-owned boxed Mixed callback argument
            continue;
        }

        match visible_ty.codegen_repr() {
            PhpType::Str => {
                let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
                abi::emit_load_temporary_stack_slot(ctx.emitter, ptr_reg, frame.raw_offsets[index]);
                abi::emit_load_temporary_stack_slot(ctx.emitter, len_reg, frame.raw_offsets[index] + 8);
                abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg); // stage the callback string pointer/length pair for ordinary call lowering
            }
            PhpType::Float => {
                abi::emit_load_temporary_stack_slot(
                    ctx.emitter,
                    abi::float_result_reg(ctx.emitter),
                    frame.raw_offsets[index],
                );
                abi::emit_push_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter)); // stage the callback float argument for ordinary call lowering
            }
            PhpType::Void | PhpType::Never => {}
            _ => {
                abi::emit_load_temporary_stack_slot(
                    ctx.emitter,
                    abi::int_result_reg(ctx.emitter),
                    frame.raw_offsets[index],
                );
                abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter)); // stage the raw callback scalar/pointer argument for ordinary call lowering
            }
        }
    }
}

/// Saves a callback return value while boxed argument temporaries are released.
pub(super) fn save_callback_return_value(
    ctx: &mut FunctionContext<'_>,
    frame: &CallbackWrapperFrame,
    return_ty: &PhpType,
) {
    match return_ty.codegen_repr() {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_store_to_sp(ctx.emitter, ptr_reg, frame.return_offset);
            abi::emit_store_to_sp(ctx.emitter, len_reg, frame.return_offset + 8);
        }
        PhpType::Float => {
            abi::emit_store_to_sp(ctx.emitter, abi::float_result_reg(ctx.emitter), frame.return_offset);
        }
        _ => {
            abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), frame.return_offset);
        }
    }
}

/// Restores a callback return value after boxed argument temporaries are released.
pub(super) fn restore_callback_return_value(
    ctx: &mut FunctionContext<'_>,
    frame: &CallbackWrapperFrame,
    return_ty: &PhpType,
) {
    match return_ty.codegen_repr() {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_load_temporary_stack_slot(ctx.emitter, ptr_reg, frame.return_offset);
            abi::emit_load_temporary_stack_slot(ctx.emitter, len_reg, frame.return_offset + 8);
        }
        PhpType::Float => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, abi::float_result_reg(ctx.emitter), frame.return_offset);
        }
        _ => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), frame.return_offset);
        }
    }
}

/// Releases boxed Mixed arguments allocated by the wrapper for target `Mixed` parameters.
pub(super) fn release_callback_boxed_args(ctx: &mut FunctionContext<'_>, frame: &CallbackWrapperFrame) {
    for offset in &frame.boxed_offsets {
        if *offset == NO_CALLBACK_BOX_OFFSET {
            continue;
        }
        abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), *offset);
        abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Mixed);
    }
}

/// Cleans up boxed callback arguments without losing the callback return value.
pub(super) fn cleanup_callback_boxed_args(
    ctx: &mut FunctionContext<'_>,
    frame: &CallbackWrapperFrame,
    return_ty: &PhpType,
) {
    if !callback_wrapper_has_boxed_args(frame) {
        return;
    }
    if callback_return_may_alias_boxed_args(return_ty) {
        return;
    }
    save_callback_return_value(ctx, frame, return_ty);
    release_callback_boxed_args(ctx, frame);
    restore_callback_return_value(ctx, frame, return_ty);
}

/// Returns true when the callback result may be one of the boxed wrapper arguments.
pub(super) fn callback_return_may_alias_boxed_args(return_ty: &PhpType) -> bool {
    matches!(
        return_ty.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_) | PhpType::TaggedScalar
    )
}
