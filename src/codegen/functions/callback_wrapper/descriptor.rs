//! Purpose:
//! Emits callback wrappers that adapt array-runtime callbacks to callable descriptor invokers.
//! Owns temporary Mixed argument-array construction and descriptor result casting.
//!
//! Called from:
//! - `crate::codegen::functions::callback_wrapper::emit_callback_wrapper()`.
//!
//! Key details:
//! - Runtime array helpers expect callee-saved loop registers to survive callback invocation.
//! - Descriptor invokers return boxed `Mixed`; wrappers cast or detach results before returning.

use crate::codegen::abi;
use crate::codegen::callable_descriptor;
use crate::codegen::context::{DeferredCallbackWrapper, DeferredExternCallbackTrampoline};
use crate::codegen::emit::Emitter;
use crate::codegen::expr::arrays::emit_array_value_type_stamp;
use crate::codegen::platform::Arch;
use crate::types::PhpType;

use super::{align16, frame_arg_slot_offset, incoming_env_reg, spill_visible_args};

/// Emits a descriptor-backed callback wrapper for the current target.
pub(super) fn emit_descriptor_callback_wrapper(
    emitter: &mut Emitter,
    wrapper: &DeferredCallbackWrapper,
    return_ty: &PhpType,
) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64_descriptor_callback_wrapper(emitter, wrapper, return_ty);
    } else {
        emit_aarch64_descriptor_callback_wrapper(emitter, wrapper, return_ty);
    }
}

/// Emits an ARM64 callback wrapper that adapts runtime descriptor dispatch to array callbacks.
fn emit_aarch64_descriptor_callback_wrapper(
    emitter: &mut Emitter,
    wrapper: &DeferredCallbackWrapper,
    return_ty: &PhpType,
) {
    let visible_count = wrapper.visible_arg_types.len();
    let slot_count = (visible_count + 1).max(1);
    let frame_size = align16(slot_count * 16 + 48);
    let saved_descriptor_offset = frame_size - 48;
    let saved_runtime_offset = frame_size - 32;

    emitter.blank();
    emitter.comment(&format!("descriptor callback wrapper: {}", wrapper.label));
    emitter.raw(".align 2");
    emitter.label_global(&wrapper.label);
    abi::emit_frame_prologue(emitter, frame_size);
    emitter.instruction(&format!("stp x19, x20, [sp, #{}]", saved_descriptor_offset)); // preserve descriptor-wrapper callee-saved registers
    emitter.instruction(&format!("stp x21, x22, [sp, #{}]", saved_runtime_offset)); // preserve runtime-loop callee-saved registers across descriptor invocation

    let env_reg = incoming_env_reg(emitter, &wrapper.visible_arg_types);
    emitter.instruction(&format!("mov x20, {}", env_reg));                      // keep the descriptor callback environment pointer across nested calls
    emitter.instruction("ldr x19, [x20]");                                      // load the selected callable descriptor from env slot zero

    spill_visible_args(emitter, &wrapper.visible_arg_types);
    emit_build_descriptor_invoker_arg_array(emitter, wrapper, frame_size, "x20");
    emit_box_descriptor_arg_array_as_mixed(emitter, frame_size, visible_count);
    emit_call_descriptor_invoker_from_wrapper(emitter, "x19");
    emit_cast_descriptor_mixed_result_for_callback(emitter, return_ty);

    emitter.instruction(&format!("ldp x21, x22, [sp, #{}]", saved_runtime_offset)); // restore runtime-loop callee-saved registers after descriptor invocation
    emitter.instruction(&format!("ldp x19, x20, [sp, #{}]", saved_descriptor_offset)); // restore descriptor-wrapper callee-saved registers
    abi::emit_frame_restore(emitter, frame_size);
    abi::emit_return(emitter);
}

/// Emits an x86_64 callback wrapper that adapts runtime descriptor dispatch to array callbacks.
fn emit_x86_64_descriptor_callback_wrapper(
    emitter: &mut Emitter,
    wrapper: &DeferredCallbackWrapper,
    return_ty: &PhpType,
) {
    let visible_count = wrapper.visible_arg_types.len();
    let slot_count = (visible_count + 1).max(1);
    let frame_size = align16(slot_count * 16 + 64);
    let saved_descriptor_offset = slot_count * 16 + 16;
    let saved_env_offset = slot_count * 16 + 24;
    let saved_runtime_index_offset = slot_count * 16 + 32;
    let saved_runtime_count_offset = slot_count * 16 + 40;

    emitter.blank();
    emitter.comment(&format!("descriptor callback wrapper: {}", wrapper.label));
    emitter.raw(".align 16");
    emitter.label_global(&wrapper.label);
    abi::emit_frame_prologue(emitter, frame_size);
    abi::store_at_offset(emitter, "r12", saved_descriptor_offset);
    abi::store_at_offset(emitter, "r13", saved_env_offset);
    abi::store_at_offset(emitter, "r14", saved_runtime_index_offset);
    abi::store_at_offset(emitter, "r15", saved_runtime_count_offset);

    let env_reg = incoming_env_reg(emitter, &wrapper.visible_arg_types);
    emitter.instruction(&format!("mov r13, {}", env_reg));                      // keep the descriptor callback environment pointer across nested calls
    emitter.instruction("mov r12, QWORD PTR [r13]");                            // load the selected callable descriptor from env slot zero

    spill_visible_args(emitter, &wrapper.visible_arg_types);
    emit_build_descriptor_invoker_arg_array(emitter, wrapper, frame_size, "r13");
    emit_box_descriptor_arg_array_as_mixed(emitter, frame_size, visible_count);
    emit_call_descriptor_invoker_from_wrapper(emitter, "r12");
    emit_cast_descriptor_mixed_result_for_callback(emitter, return_ty);

    abi::load_at_offset(emitter, "r15", saved_runtime_count_offset);
    abi::load_at_offset(emitter, "r14", saved_runtime_index_offset);
    abi::load_at_offset(emitter, "r13", saved_env_offset);
    abi::load_at_offset(emitter, "r12", saved_descriptor_offset);
    abi::emit_frame_restore(emitter, frame_size);
    abi::emit_return(emitter);
}

/// Emits a C-ABI extern callback trampoline backed by a global descriptor slot.
pub(super) fn emit_extern_callback_trampoline(
    emitter: &mut Emitter,
    trampoline: &DeferredExternCallbackTrampoline,
) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64_extern_callback_trampoline(emitter, trampoline);
    } else {
        emit_aarch64_extern_callback_trampoline(emitter, trampoline);
    }
}

/// Emits an ARM64 extern callback trampoline for descriptor-backed FFI callbacks.
fn emit_aarch64_extern_callback_trampoline(
    emitter: &mut Emitter,
    trampoline: &DeferredExternCallbackTrampoline,
) {
    let wrapper = extern_trampoline_wrapper_view(trampoline);
    let visible_count = wrapper.visible_arg_types.len();
    let slot_count = (visible_count + 1).max(1);
    let frame_size = align16(slot_count * 16 + 48);
    let saved_descriptor_offset = frame_size - 48;
    let saved_runtime_offset = frame_size - 32;

    emitter.blank();
    emitter.comment(&format!("extern descriptor callback trampoline: {}", trampoline.label));
    emitter.raw(".align 2");
    emitter.label_global(&trampoline.label);
    abi::emit_frame_prologue(emitter, frame_size);
    emitter.instruction(&format!("stp x19, x20, [sp, #{}]", saved_descriptor_offset)); // preserve descriptor trampoline registers across invoker dispatch
    emitter.instruction(&format!("stp x21, x22, [sp, #{}]", saved_runtime_offset)); // preserve runtime-loop registers across descriptor invocation

    abi::emit_load_symbol_to_reg(
        emitter,
        "x19",
        &trampoline.descriptor_slot_label,
        0,
    );
    spill_visible_args(emitter, &wrapper.visible_arg_types);
    emit_build_descriptor_invoker_arg_array(emitter, &wrapper, frame_size, "x20");
    emit_box_descriptor_arg_array_as_mixed(emitter, frame_size, visible_count);
    emit_call_descriptor_invoker_from_wrapper(emitter, "x19");
    emit_cast_descriptor_mixed_result_for_callback(emitter, &trampoline.return_type);

    emitter.instruction(&format!("ldp x21, x22, [sp, #{}]", saved_runtime_offset)); // restore runtime-loop registers after descriptor invocation
    emitter.instruction(&format!("ldp x19, x20, [sp, #{}]", saved_descriptor_offset)); // restore descriptor trampoline registers
    abi::emit_frame_restore(emitter, frame_size);
    abi::emit_return(emitter);
}

/// Emits an x86_64 extern callback trampoline for descriptor-backed FFI callbacks.
fn emit_x86_64_extern_callback_trampoline(
    emitter: &mut Emitter,
    trampoline: &DeferredExternCallbackTrampoline,
) {
    let wrapper = extern_trampoline_wrapper_view(trampoline);
    let visible_count = wrapper.visible_arg_types.len();
    let slot_count = (visible_count + 1).max(1);
    let frame_size = align16(slot_count * 16 + 64);
    let saved_descriptor_offset = slot_count * 16 + 16;
    let saved_env_offset = slot_count * 16 + 24;
    let saved_runtime_index_offset = slot_count * 16 + 32;
    let saved_runtime_count_offset = slot_count * 16 + 40;

    emitter.blank();
    emitter.comment(&format!("extern descriptor callback trampoline: {}", trampoline.label));
    emitter.raw(".align 16");
    emitter.label_global(&trampoline.label);
    abi::emit_frame_prologue(emitter, frame_size);
    abi::store_at_offset(emitter, "r12", saved_descriptor_offset);
    abi::store_at_offset(emitter, "r13", saved_env_offset);
    abi::store_at_offset(emitter, "r14", saved_runtime_index_offset);
    abi::store_at_offset(emitter, "r15", saved_runtime_count_offset);

    abi::emit_load_symbol_to_reg(
        emitter,
        "r12",
        &trampoline.descriptor_slot_label,
        0,
    );
    spill_visible_args(emitter, &wrapper.visible_arg_types);
    emit_build_descriptor_invoker_arg_array(emitter, &wrapper, frame_size, "r13");
    emit_box_descriptor_arg_array_as_mixed(emitter, frame_size, visible_count);
    emit_call_descriptor_invoker_from_wrapper(emitter, "r12");
    emit_cast_descriptor_mixed_result_for_callback(emitter, &trampoline.return_type);

    abi::load_at_offset(emitter, "r15", saved_runtime_count_offset);
    abi::load_at_offset(emitter, "r14", saved_runtime_index_offset);
    abi::load_at_offset(emitter, "r13", saved_env_offset);
    abi::load_at_offset(emitter, "r12", saved_descriptor_offset);
    abi::emit_frame_restore(emitter, frame_size);
    abi::emit_return(emitter);
}

/// Builds the descriptor-wrapper view reused by extern callback trampolines.
fn extern_trampoline_wrapper_view(
    trampoline: &DeferredExternCallbackTrampoline,
) -> DeferredCallbackWrapper {
    DeferredCallbackWrapper {
        label: trampoline.label.clone(),
        visible_arg_types: trampoline.visible_arg_types.clone(),
        target_visible_arg_types: None,
        capture_types: Vec::new(),
        descriptor_prefix_types: Vec::new(),
        descriptor_return_type: Some(trampoline.return_type.clone()),
    }
}

/// Builds the boxed-Mixed indexed argument array consumed by descriptor invokers.
fn emit_build_descriptor_invoker_arg_array(
    emitter: &mut Emitter,
    wrapper: &DeferredCallbackWrapper,
    frame_size: usize,
    env_reg: &str,
) {
    let visible_arg_types = &wrapper.visible_arg_types;
    let visible_count = visible_arg_types.len();
    let prefix_count = wrapper.descriptor_prefix_types.len();
    let total_count = prefix_count + visible_count;
    let array_frame_offset = descriptor_array_frame_offset(emitter, frame_size, visible_count);

    emit_allocate_descriptor_arg_array(emitter, total_count);
    abi::store_at_offset(emitter, abi::int_result_reg(emitter), array_frame_offset);
    emit_array_value_type_stamp(emitter, abi::int_result_reg(emitter), &PhpType::Mixed);

    for (idx, ty) in wrapper.descriptor_prefix_types.iter().enumerate() {
        load_descriptor_prefix_arg_to_result(emitter, env_reg, idx, ty);
        emit_box_visible_arg_as_mixed(emitter, ty);
        emit_store_current_mixed_arg_array_element(emitter, array_frame_offset, idx);
    }

    for (idx, ty) in visible_arg_types.iter().enumerate() {
        load_spilled_visible_arg_to_result(emitter, frame_size, idx, ty);
        emit_box_visible_arg_as_mixed(emitter, ty);
        emit_store_current_mixed_arg_array_element(
            emitter,
            array_frame_offset,
            prefix_count + idx,
        );
    }
}

/// Loads a descriptor-prefix argument stored in the callback environment.
fn load_descriptor_prefix_arg_to_result(
    emitter: &mut Emitter,
    env_reg: &str,
    idx: usize,
    ty: &PhpType,
) {
    let slot_offset = (idx + 1) * 16;
    match ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_load_from_address(emitter, abi::float_result_reg(emitter), env_reg, slot_offset);
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_load_from_address(emitter, ptr_reg, env_reg, slot_offset);
            abi::emit_load_from_address(emitter, len_reg, env_reg, slot_offset + 8);
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), env_reg, slot_offset);
        }
    }
}

/// Allocates the temporary indexed array used as the descriptor invoker argument container.
fn emit_allocate_descriptor_arg_array(emitter: &mut Emitter, visible_count: usize) {
    let capacity = visible_count.max(4) as i64;
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(emitter, "x0", capacity);
            abi::emit_load_int_immediate(emitter, "x1", 8);
            abi::emit_call_label(emitter, "__rt_array_new");                   // allocate the descriptor invoker argument array
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(emitter, "rdi", capacity);
            abi::emit_load_int_immediate(emitter, "rsi", 8);
            abi::emit_call_label(emitter, "__rt_array_new");                   // allocate the descriptor invoker argument array
        }
    }
}

/// Loads a spilled callback-visible argument back into the ABI result registers.
fn load_spilled_visible_arg_to_result(
    emitter: &mut Emitter,
    frame_size: usize,
    idx: usize,
    ty: &PhpType,
) {
    let slot_offset = visible_arg_frame_offset(emitter, frame_size, idx);
    match ty.codegen_repr() {
        PhpType::Float => {
            abi::load_at_offset(emitter, abi::float_result_reg(emitter), slot_offset);
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::load_at_offset(emitter, ptr_reg, slot_offset);
            abi::load_at_offset(emitter, len_reg, slot_offset - 8);
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            abi::load_at_offset(emitter, abi::int_result_reg(emitter), slot_offset);
        }
    }
}

/// Converts the current visible argument result to an owned boxed-Mixed argument cell.
fn emit_box_visible_arg_as_mixed(emitter: &mut Emitter, ty: &PhpType) {
    match ty.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => {
            abi::emit_incref_if_refcounted(emitter, &PhpType::Mixed);
        }
        lowered => crate::codegen::emit_box_current_value_as_mixed(emitter, &lowered),
    }
}

/// Stores the boxed-Mixed current result into the indexed invoker argument array.
fn emit_store_current_mixed_arg_array_element(
    emitter: &mut Emitter,
    array_frame_offset: usize,
    idx: usize,
) {
    let array_reg = match emitter.target.arch {
        Arch::AArch64 => "x9",
        Arch::X86_64 => "r10",
    };
    let len_reg = match emitter.target.arch {
        Arch::AArch64 => "x10",
        Arch::X86_64 => "r11",
    };
    let elem_offset = 24 + idx * 8;

    abi::load_at_offset(emitter, array_reg, array_frame_offset);
    abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), array_reg, elem_offset);
    abi::emit_load_int_immediate(emitter, len_reg, (idx + 1) as i64);
    abi::emit_store_to_address(emitter, len_reg, array_reg, 0);
}

/// Boxes the prepared indexed argument array as the second descriptor-invoker argument.
fn emit_box_descriptor_arg_array_as_mixed(
    emitter: &mut Emitter,
    frame_size: usize,
    visible_count: usize,
) {
    let array_arg_reg = abi::int_arg_reg_name(emitter.target, 1);
    let array_frame_offset = descriptor_array_frame_offset(emitter, frame_size, visible_count);
    let container_ty = PhpType::Array(Box::new(PhpType::Mixed));

    abi::load_at_offset(emitter, array_arg_reg, array_frame_offset);
    crate::codegen::builtins::arrays::call_user_func_array::emit_box_invoker_arg_clone_as_mixed(
        array_arg_reg,
        &container_ty,
        emitter,
    );
}

/// Calls the selected descriptor invoker and releases the boxed argument container afterward.
fn emit_call_descriptor_invoker_from_wrapper(emitter: &mut Emitter, descriptor_reg: &str) {
    let descriptor_arg_reg = abi::int_arg_reg_name(emitter.target, 0);
    let array_arg_reg = abi::int_arg_reg_name(emitter.target, 1);
    let invoker_reg = abi::symbol_scratch_reg(emitter);

    if descriptor_reg != descriptor_arg_reg {
        emitter.instruction(&format!("mov {}, {}", descriptor_arg_reg, descriptor_reg)); // pass the selected callable descriptor to the uniform invoker
    }
    callable_descriptor::emit_load_invoker_from_descriptor(
        emitter,
        invoker_reg,
        descriptor_arg_reg,
    );
    abi::emit_push_reg(emitter, array_arg_reg);                                 // preserve the boxed argument container for release after descriptor invocation
    abi::emit_call_reg(emitter, invoker_reg);
    emit_release_preserved_mixed_argument_after_result(emitter);
}

/// Releases the preserved boxed argument container while keeping the Mixed call result live.
fn emit_release_preserved_mixed_argument_after_result(emitter: &mut Emitter) {
    abi::emit_push_result_value(emitter, &PhpType::Mixed);
    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 16);
    abi::emit_decref_if_refcounted(emitter, &PhpType::Mixed);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
    abi::emit_release_temporary_stack(emitter, 16);
}

/// Converts the descriptor invoker's boxed-Mixed result to the callback runtime return type.
fn emit_cast_descriptor_mixed_result_for_callback(emitter: &mut Emitter, return_ty: &PhpType) {
    match return_ty.codegen_repr() {
        PhpType::Bool => {
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));          // preserve the owned Mixed callback result while casting to bool
            abi::emit_call_label(emitter, "__rt_mixed_cast_bool");             // convert the boxed callback result to PHP truthiness
            emit_release_preserved_mixed_result_after_cast(emitter, &PhpType::Bool);
        }
        PhpType::Int | PhpType::Resource(_) | PhpType::Pointer(_) => {
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));          // preserve the owned Mixed callback result while casting to int
            abi::emit_call_label(emitter, "__rt_mixed_cast_int");              // convert the boxed callback result to an integer value
            emit_release_preserved_mixed_result_after_cast(emitter, return_ty);
        }
        PhpType::Float => {
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));          // preserve the owned Mixed callback result while casting to float
            abi::emit_call_label(emitter, "__rt_mixed_cast_float");            // convert the boxed callback result to a floating-point value
            emit_release_preserved_mixed_result_after_cast(emitter, &PhpType::Float);
        }
        PhpType::Str => {
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));          // preserve the owned Mixed callback result while casting to string
            abi::emit_call_label(emitter, "__rt_mixed_cast_string");           // convert the boxed callback result to a string payload
            emit_release_preserved_mixed_result_after_cast(emitter, &PhpType::Str);
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_decref_if_refcounted(emitter, &PhpType::Mixed);
        }
        _ => {}
    }
}

/// Releases the preserved boxed-Mixed result after a scalar cast and restores the cast value.
fn emit_release_preserved_mixed_result_after_cast(emitter: &mut Emitter, cast_ty: &PhpType) {
    if matches!(cast_ty.codegen_repr(), PhpType::Str) {
        let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
        abi::emit_call_label(emitter, "__rt_str_persist");                     // detach the string result from the boxed Mixed owner before release
        abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);                    // preserve the detached string while releasing the boxed Mixed result
        abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 16);
        abi::emit_decref_if_refcounted(emitter, &PhpType::Mixed);
        abi::emit_pop_reg_pair(emitter, ptr_reg, len_reg);
        abi::emit_release_temporary_stack(emitter, 16);
        return;
    }

    abi::emit_push_result_value(emitter, cast_ty);
    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 16);
    abi::emit_decref_if_refcounted(emitter, &PhpType::Mixed);
    match cast_ty.codegen_repr() {
        PhpType::Float => abi::emit_pop_float_reg(emitter, abi::float_result_reg(emitter)),
        _ => abi::emit_pop_reg(emitter, abi::int_result_reg(emitter)),
    }
    abi::emit_release_temporary_stack(emitter, 16);
}

/// Returns the local-frame offset for a spilled visible callback argument.
fn visible_arg_frame_offset(emitter: &Emitter, frame_size: usize, idx: usize) -> usize {
    match emitter.target.arch {
        Arch::AArch64 => frame_size - 16 - idx * 16,
        Arch::X86_64 => frame_arg_slot_offset(idx),
    }
}

/// Returns the local-frame offset for the temporary descriptor-invoker argument array.
fn descriptor_array_frame_offset(
    emitter: &Emitter,
    frame_size: usize,
    visible_count: usize,
) -> usize {
    match emitter.target.arch {
        Arch::AArch64 => frame_size - 16 - visible_count * 16,
        Arch::X86_64 => frame_arg_slot_offset(visible_count),
    }
}
