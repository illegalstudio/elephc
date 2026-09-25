//! Purpose:
//! Emits native callback wrappers that adapt external callbacks into PHP-callable function bodies.
//! Moves callback arguments through compiler ABI slots and returns runtime-compatible values.
//!
//! Called from:
//! - `crate::codegen` and `crate::codegen_support::driver_support` when callback metadata is required.
//!
//! Key details:
//! - Wrapper signatures must satisfy both the external ABI and the internal PHP function lowering contract.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::{Arch, Platform};
use crate::types::PhpType;

mod descriptor;

use super::{DeferredCallbackWrapper, DeferredExternCallbackTrampoline};

/// Emits a native callback wrapper that adapts an external ABI caller into a PHP-callable
/// function body. Dispatches to the x86_64 variant; ARM64 uses the general path below.
/// The wrapper preserves callee-saved registers, spills incoming arguments and captures
/// from the environment struct, then calls the original closure entry point before returning.
pub(crate) fn emit_callback_wrapper(emitter: &mut Emitter, wrapper: &DeferredCallbackWrapper) {
    if let Some(return_ty) = &wrapper.descriptor_return_type {
        descriptor::emit_descriptor_callback_wrapper(emitter, wrapper, return_ty);
        return;
    }

    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64_callback_wrapper(emitter, wrapper);
        return;
    }

    let target_visible_arg_types = wrapper_target_visible_arg_types(wrapper);
    let arg_types = wrapper_arg_types(wrapper);
    let slot_count = arg_types.len().max(1);
    let frame_size = align16(slot_count * 16 + 32);
    let saved_callee_offset = frame_size - 32;

    emitter.blank();
    emitter.comment(&format!("callback wrapper: {}", wrapper.label));
    emitter.raw(".align 2");
    emitter.label_global(&wrapper.label);
    abi::emit_frame_prologue(emitter, frame_size);
    emitter.instruction(&format!("stp x19, x20, [sp, #{}]", saved_callee_offset)); // preserve wrapper callee-saved registers

    let env_reg = incoming_env_reg(emitter, &wrapper.visible_arg_types);
    emitter.instruction(&format!("mov x20, {}", env_reg));                      // keep the callback environment pointer across argument reshuffling
    emitter.instruction("ldr x19, [x20]");                                      // load the original captured closure entry point from env slot zero

    spill_visible_args(emitter, &wrapper.visible_arg_types, false);
    spill_captures(
        emitter,
        wrapper.visible_arg_types.len(),
        &wrapper.capture_types,
        "x20",
    );

    let overflow_bytes = materialize_spilled_args_for_callback(
        emitter,
        &wrapper.visible_arg_types,
        &target_visible_arg_types,
        &wrapper.capture_types,
        frame_size,
    );
    let call_pad_bytes = abi::outgoing_call_stack_pad_bytes(emitter.target, 0);
    abi::emit_reserve_temporary_stack(emitter, call_pad_bytes);
    abi::emit_call_reg(emitter, "x19");
    abi::emit_release_temporary_stack(emitter, call_pad_bytes);
    abi::emit_release_temporary_stack(emitter, overflow_bytes); // drop stack-passed closure arguments after the adapted callback returns

    emitter.instruction(&format!("ldp x19, x20, [sp, #{}]", saved_callee_offset)); // restore wrapper callee-saved registers
    abi::emit_frame_restore(emitter, frame_size);
    abi::emit_return(emitter);
}

/// Emits a C-ABI trampoline that reloads a descriptor from global storage.
///
/// The generated symbol has the callback signature expected by the extern C API,
/// boxes incoming scalar/pointer arguments for the descriptor invoker, and casts
/// the boxed result back to the C-compatible callback return type.
pub(crate) fn emit_extern_callback_trampoline(
    emitter: &mut Emitter,
    trampoline: &DeferredExternCallbackTrampoline,
) {
    descriptor::emit_extern_callback_trampoline(emitter, trampoline);
}

/// Emits the x86_64-specific callback wrapper. Follows the same general pattern as the ARM64
/// path but uses x86_64 callee-saved registers (r12, r13), different frame layout, and
/// stdarg-style argument push for overflow parameters.
fn emit_x86_64_callback_wrapper(emitter: &mut Emitter, wrapper: &DeferredCallbackWrapper) {
    let target_visible_arg_types = wrapper_target_visible_arg_types(wrapper);
    let arg_types = wrapper_arg_types(wrapper);
    let slot_count = arg_types.len().max(1);
    let frame_size = align16(slot_count * 16 + 48);
    let saved_callback_offset = slot_count * 16 + 16;
    let saved_env_offset = slot_count * 16 + 24;

    emitter.blank();
    emitter.comment(&format!("callback wrapper: {}", wrapper.label));
    emitter.raw(".align 16");
    emitter.label_global(&wrapper.label);
    abi::emit_frame_prologue(emitter, frame_size);
    abi::store_at_offset(emitter, "r12", saved_callback_offset);
    abi::store_at_offset(emitter, "r13", saved_env_offset);

    let env_reg = incoming_env_reg(emitter, &wrapper.visible_arg_types);
    emitter.instruction(&format!("mov r13, {}", env_reg));                      // keep the callback environment pointer across argument reshuffling
    emitter.instruction("mov r12, QWORD PTR [r13]");                            // load the original captured closure entry point from env slot zero

    spill_visible_args(emitter, &wrapper.visible_arg_types, false);
    spill_captures(
        emitter,
        wrapper.visible_arg_types.len(),
        &wrapper.capture_types,
        "r13",
    );

    let overflow_bytes = materialize_spilled_args_for_callback_x86_64(
        emitter,
        &wrapper.visible_arg_types,
        &target_visible_arg_types,
        &wrapper.capture_types,
    );
    let call_pad_bytes = abi::outgoing_call_stack_pad_bytes(emitter.target, 0);
    abi::emit_reserve_temporary_stack(emitter, call_pad_bytes);
    abi::emit_call_reg(emitter, "r12");
    abi::emit_release_temporary_stack(emitter, call_pad_bytes);
    abi::emit_release_temporary_stack(emitter, overflow_bytes); // drop stack-passed closure arguments after the adapted callback returns

    abi::load_at_offset(emitter, "r13", saved_env_offset);
    abi::load_at_offset(emitter, "r12", saved_callback_offset);
    abi::emit_frame_restore(emitter, frame_size);
    abi::emit_return(emitter);
}

/// Returns the ordered list of PHP types for all arguments the wrapper will pass to the
/// adapted callback: visible arg types first (in incoming ABI order), then capture types.
fn wrapper_arg_types(wrapper: &DeferredCallbackWrapper) -> Vec<PhpType> {
    wrapper_target_visible_arg_types(wrapper)
        .iter()
        .chain(wrapper.capture_types.iter())
        .map(PhpType::codegen_repr)
        .collect()
}

/// Provides the Wrapper target visible arg types helper used by the callback wrapper module.
fn wrapper_target_visible_arg_types(wrapper: &DeferredCallbackWrapper) -> Vec<PhpType> {
    wrapper
        .target_visible_arg_types
        .clone()
        .unwrap_or_else(|| wrapper.visible_arg_types.clone())
}

/// Returns the register containing the incoming environment pointer (the closure struct passed
/// by the callback runtime). The environment pointer is the last argument in the incoming type
/// list; this function reverses the outgoing assignment logic to find its register or loads it
/// from the caller stack when the target ABI has no register left.
///
/// Windows x86_64 callback calls are made by [`Emitter::emit_platform_callback_call`]. That
/// adapter reserves the mandatory 32-byte shadow space and packs overflow integer words into
/// consecutive eight-byte slots. Consequently a string pair consumes two positional slots,
/// while the callback wrapper's local frame does not change the caller-argument offset from
/// `rbp + 48` (return address + saved `rbp` + shadow space).
fn incoming_env_reg(emitter: &mut Emitter, visible_arg_types: &[PhpType]) -> &'static str {
    let mut incoming_types: Vec<PhpType> = visible_arg_types
        .iter()
        .map(PhpType::codegen_repr)
        .collect();
    incoming_types.push(PhpType::Pointer(None));
    // Runtime helpers enter generated wrappers through `emit_platform_callback_call`.
    // On Windows that bridge translates its SysV staging into native MSx64 *positional*
    // slots: an incoming string consumes two of the four rcx/rdx/r8/r9 slots, and the
    // environment follows on the caller stack once those four slots are full. This is
    // deliberately different from the PHP-to-PHP planner, which keeps the float and
    // integer register streams independent.
    let assignments = if is_windows_x86_64_callback_abi(emitter) {
        abi::build_c_abi_outgoing_arg_assignments_for_target(emitter.target, &incoming_types)
    } else {
        abi::build_outgoing_arg_assignments_for_target(emitter.target, &incoming_types, 0)
    };
    let env_assignment = assignments
        .last()
        .expect("callback wrapper always has an environment pointer argument");
    if env_assignment.in_register() {
        return abi::int_arg_reg_name(emitter.target, env_assignment.start_reg);
    }

    let stack_offset = incoming_env_stack_offset(emitter, &incoming_types);
    let scratch = match emitter.target.arch {
        Arch::AArch64 => "x9",
        Arch::X86_64 => "r10",
    };
    abi::load_from_caller_stack(emitter, scratch, stack_offset);
    scratch
}

/// Computes the caller-frame offset of a stack-passed callback environment.
fn incoming_env_stack_offset(emitter: &Emitter, incoming_types: &[PhpType]) -> usize {
    let base = abi::IncomingArgCursor::for_target(emitter.target, 0).caller_stack_offset;

    if (emitter.target.platform, emitter.target.arch) == (Platform::Windows, Arch::X86_64) {
        // `emit_platform_callback_call` uses the native MSx64 overflow layout: after the four
        // register slots, every remaining integer word occupies one eight-byte stack slot.
        let positional_words: usize = incoming_types
            .iter()
            .take(incoming_types.len() - 1)
            .map(PhpType::register_count)
            .sum();
        return base + positional_words.saturating_sub(4) * 8;
    }

    // The generic Elephc callback ABI uses one 16-byte temporary slot per overflow argument.
    // Keep this path target-neutral for AArch64 and the existing Unix x86_64 wrappers.
    let assignments = abi::build_outgoing_arg_assignments_for_target(emitter.target, incoming_types, 0);
    let mut offset = base;
    for (ty, assignment) in incoming_types.iter().zip(assignments.iter()).take(assignments.len() - 1) {
        if !assignment.in_register() && !matches!(ty, PhpType::Void | PhpType::Never) {
            offset += 16;
        }
    }
    offset
}

/// Returns whether a wrapper receives its visible arguments through the native MSx64
/// positional-slot callback boundary instead of the ordinary generated PHP ABI.
fn is_windows_x86_64_callback_abi(emitter: &Emitter) -> bool {
    (emitter.target.platform, emitter.target.arch) == (Platform::Windows, Arch::X86_64)
}

/// Spills every incoming visible argument from ABI registers to fixed stack slots in the
/// wrapper frame. This must happen before `spill_captures` loads from the environment struct,
/// because the environment pointer lives in a register that may clobber one of the arg regs.
fn spill_visible_args(
    emitter: &mut Emitter,
    visible_arg_types: &[PhpType],
    native_c_abi: bool,
) {
    let visible_types: Vec<PhpType> = visible_arg_types
        .iter()
        .map(PhpType::codegen_repr)
        .collect();
    let assignments = if native_c_abi || is_windows_x86_64_callback_abi(emitter) {
        abi::build_c_abi_outgoing_arg_assignments_for_target(emitter.target, &visible_types)
    } else {
        abi::build_outgoing_arg_assignments_for_target(emitter.target, &visible_types, 0)
    };
    let windows_positional_callback_abi = is_windows_x86_64_callback_abi(emitter);
    let mut stack_offset = if windows_positional_callback_abi {
        48
    } else {
        abi::IncomingArgCursor::for_target(emitter.target, 0).caller_stack_offset
    };
    for (idx, (ty, assignment)) in visible_types.iter().zip(assignments.iter()).enumerate() {
        if !assignment.in_register() {
            let scalar_scratch = match emitter.target.arch {
                Arch::AArch64 => "x9",
                Arch::X86_64 => "r10",
            };
            let float_scratch = match emitter.target.arch {
                Arch::AArch64 => "d15",
                Arch::X86_64 => "xmm15",
            };
            match ty {
                PhpType::Float => {
                    abi::load_from_caller_stack(emitter, float_scratch, stack_offset);
                    abi::store_at_offset(emitter, float_scratch, frame_arg_slot_offset(idx));
                }
                PhpType::Str => {
                    let high_scratch = match emitter.target.arch {
                        Arch::AArch64 => "x10",
                        Arch::X86_64 => "r11",
                    };
                    abi::load_from_caller_stack(emitter, scalar_scratch, stack_offset);
                    abi::load_from_caller_stack(emitter, high_scratch, stack_offset + 8);
                    abi::store_at_offset(emitter, scalar_scratch, frame_arg_slot_offset(idx));
                    abi::store_at_offset(
                        emitter,
                        high_scratch,
                        frame_arg_slot_offset(idx) - 8,
                    );
                }
                PhpType::Void | PhpType::Never => {}
                _ => {
                    abi::load_from_caller_stack(emitter, scalar_scratch, stack_offset);
                    abi::store_at_offset(emitter, scalar_scratch, frame_arg_slot_offset(idx));
                }
            }
            stack_offset += if windows_positional_callback_abi {
                ty.register_count() * 8
            } else {
                16
            };
            continue;
        }
        match (emitter.target.arch, ty) {
            (Arch::AArch64, PhpType::Float) => {
                let reg = abi::float_arg_reg_name(emitter.target, assignment.start_reg);
                emitter.instruction(&format!("str {}, [sp, #{}]", reg, idx * 16)); // spill the incoming float callback argument before loading captures
            }
            (Arch::AArch64, PhpType::Str) => {
                let ptr_reg = abi::int_arg_reg_name(emitter.target, assignment.start_reg);
                let len_reg = abi::int_arg_reg_name(emitter.target, assignment.start_reg + 1);
                emitter.instruction(&format!(                                   // spill the incoming string callback argument before loading captures
                    "stp {}, {}, [sp, #{}]",
                    ptr_reg,
                    len_reg,
                    idx * 16
                )); // spill the incoming string callback argument before loading captures
            }
            (Arch::AArch64, _) => {
                let reg = abi::int_arg_reg_name(emitter.target, assignment.start_reg);
                emitter.instruction(&format!("str {}, [sp, #{}]", reg, idx * 16)); // spill the incoming scalar callback argument before loading captures
            }
            (Arch::X86_64, PhpType::Float) => {
                let reg = abi::float_arg_reg_name(emitter.target, assignment.start_reg);
                abi::store_at_offset(emitter, reg, frame_arg_slot_offset(idx));
            }
            (Arch::X86_64, PhpType::Str) => {
                let ptr_reg = abi::int_arg_reg_name(emitter.target, assignment.start_reg);
                let len_reg = abi::int_arg_reg_name(emitter.target, assignment.start_reg + 1);
                abi::store_at_offset(emitter, ptr_reg, frame_arg_slot_offset(idx));
                abi::store_at_offset(emitter, len_reg, frame_arg_slot_offset(idx) - 8);
            }
            (Arch::X86_64, _) => {
                let reg = abi::int_arg_reg_name(emitter.target, assignment.start_reg);
                abi::store_at_offset(emitter, reg, frame_arg_slot_offset(idx));
            }
        }
    }
}

/// Loads captured values from the closure environment struct (starting at offset 16, slot 0
/// is the entry point) and spills them to stack slots after the visible args. `env_reg`
/// holds the pointer to the environment struct.
fn spill_captures(
    emitter: &mut Emitter,
    visible_count: usize,
    capture_types: &[PhpType],
    env_reg: &str,
) {
    for (idx, ty) in capture_types.iter().map(PhpType::codegen_repr).enumerate() {
        let arg_idx = visible_count + idx;
        let env_offset = (idx + 1) * 16;
        match (emitter.target.arch, ty) {
            (Arch::AArch64, PhpType::Float) => {
                emitter.instruction(&format!("ldr d0, [{}, #{}]", env_reg, env_offset)); // load a captured float from the callback environment
                emitter.instruction(&format!("str d0, [sp, #{}]", arg_idx * 16)); // spill the captured float for the final closure call
            }
            (Arch::AArch64, PhpType::Str) => {
                emitter.instruction(&format!("ldr x9, [{}, #{}]", env_reg, env_offset)); // load the captured string pointer from the callback environment
                emitter.instruction(&format!("ldr x10, [{}, #{}]", env_reg, env_offset + 8)); // load the captured string length from the callback environment
                emitter.instruction(&format!("stp x9, x10, [sp, #{}]", arg_idx * 16)); // spill the captured string pair for the final closure call
            }
            (Arch::AArch64, PhpType::Void | PhpType::Never) => {}
            (Arch::AArch64, _) => {
                emitter.instruction(&format!("ldr x9, [{}, #{}]", env_reg, env_offset)); // load a captured scalar/pointer from the callback environment
                emitter.instruction(&format!("str x9, [sp, #{}]", arg_idx * 16)); // spill the captured scalar/pointer for the final closure call
            }
            (Arch::X86_64, PhpType::Float) => {
                emitter.instruction(&format!(                                   // load a captured float from the callback environment
                    "movsd xmm0, QWORD PTR [{} + {}]",
                    env_reg, env_offset
                )); // load a captured float from the callback environment
                abi::store_at_offset(emitter, "xmm0", frame_arg_slot_offset(arg_idx));
            }
            (Arch::X86_64, PhpType::Str) => {
                emitter.instruction(&format!(                                   // load the captured string pointer from the callback environment
                    "mov r10, QWORD PTR [{} + {}]",
                    env_reg, env_offset
                )); // load the captured string pointer from the callback environment
                emitter.instruction(&format!(                                   // load the captured string length from the callback environment
                    "mov r11, QWORD PTR [{} + {}]",
                    env_reg,
                    env_offset + 8
                )); // load the captured string length from the callback environment
                abi::store_at_offset(emitter, "r10", frame_arg_slot_offset(arg_idx));
                abi::store_at_offset(emitter, "r11", frame_arg_slot_offset(arg_idx) - 8);
            }
            (Arch::X86_64, PhpType::Void | PhpType::Never) => {}
            (Arch::X86_64, _) => {
                emitter.instruction(&format!(                                   // load a captured scalar or pointer from the callback environment
                    "mov r10, QWORD PTR [{} + {}]",
                    env_reg, env_offset
                )); // load a captured scalar/pointer from the callback environment
                abi::store_at_offset(emitter, "r10", frame_arg_slot_offset(arg_idx));
            }
        }
    }
}

/// Takes the spilled arguments and pushes them onto the standard temporary call stack in
/// preparation for the adapted callback call. Returns the number of overflow bytes pushed
/// so the caller can release them after the call returns.
fn materialize_spilled_args_for_callback(
    emitter: &mut Emitter,
    incoming_visible_arg_types: &[PhpType],
    target_visible_arg_types: &[PhpType],
    capture_types: &[PhpType],
    frame_size: usize,
) -> usize {
    let arg_types = callback_target_arg_types(target_visible_arg_types, capture_types);
    push_spilled_args_as_call_temporaries(
        emitter,
        incoming_visible_arg_types,
        target_visible_arg_types,
        capture_types,
        frame_size,
    );
    let assignments = abi::build_outgoing_arg_assignments_for_target(emitter.target, &arg_types, 0);
    abi::materialize_outgoing_args(emitter, &assignments)
}

/// ARM64 path: pushes each spilled argument (float, string pair, or scalar) onto the
/// standard temporary call stack for the adapted closure invocation. Arguments are pushed
/// in reverse order so the called function can consume them as overflow parameters.
fn push_spilled_args_as_call_temporaries(
    emitter: &mut Emitter,
    incoming_visible_arg_types: &[PhpType],
    target_visible_arg_types: &[PhpType],
    capture_types: &[PhpType],
    frame_size: usize,
) {
    for (idx, (incoming_ty, target_ty)) in incoming_visible_arg_types
        .iter()
        .zip(target_visible_arg_types.iter())
        .enumerate()
    {
        let slot_offset = idx * 16;
        let frame_slot_offset = frame_size - 16 - slot_offset;
        push_aarch64_visible_arg_as_target(emitter, frame_slot_offset, incoming_ty, target_ty);
    }
    for (capture_idx, ty) in capture_types.iter().enumerate() {
        let idx = incoming_visible_arg_types.len() + capture_idx;
        let slot_offset = idx * 16;
        let frame_slot_offset = frame_size - 16 - slot_offset;
        push_aarch64_prepared_arg(emitter, frame_slot_offset, ty);
    }
}

/// x86_64 path: materializes spilled arguments for the adapted callback call. Returns
/// the number of overflow bytes pushed so the caller can release them after the call.
fn materialize_spilled_args_for_callback_x86_64(
    emitter: &mut Emitter,
    incoming_visible_arg_types: &[PhpType],
    target_visible_arg_types: &[PhpType],
    capture_types: &[PhpType],
) -> usize {
    let arg_types = callback_target_arg_types(target_visible_arg_types, capture_types);
    push_spilled_args_as_call_temporaries_x86_64(
        emitter,
        incoming_visible_arg_types,
        target_visible_arg_types,
        capture_types,
    );
    let assignments = abi::build_outgoing_arg_assignments_for_target(emitter.target, &arg_types, 0);
    abi::materialize_outgoing_args(emitter, &assignments)
}

/// x86_64 path: pushes each spilled argument onto the standard temporary call stack
/// so the called function consumes them as overflow parameters. Visible arguments may
/// be coerced to target callback types before capture arguments are appended.
fn push_spilled_args_as_call_temporaries_x86_64(
    emitter: &mut Emitter,
    incoming_visible_arg_types: &[PhpType],
    target_visible_arg_types: &[PhpType],
    capture_types: &[PhpType],
) {
    for (idx, (incoming_ty, target_ty)) in incoming_visible_arg_types
        .iter()
        .zip(target_visible_arg_types.iter())
        .enumerate()
    {
        let slot_offset = frame_arg_slot_offset(idx);
        push_x86_64_visible_arg_as_target(emitter, slot_offset, incoming_ty, target_ty);
    }
    for (capture_idx, ty) in capture_types.iter().enumerate() {
        let idx = incoming_visible_arg_types.len() + capture_idx;
        push_x86_64_prepared_arg(emitter, frame_arg_slot_offset(idx), ty);
    }
}

/// Provides the Callback target arg types helper used by the callback wrapper module.
fn callback_target_arg_types(
    target_visible_arg_types: &[PhpType],
    capture_types: &[PhpType],
) -> Vec<PhpType> {
    target_visible_arg_types
        .iter()
        .chain(capture_types.iter())
        .map(PhpType::codegen_repr)
        .collect()
}

/// Pushes AArch64 visible arg as target onto the temporary call stack or synthetic metadata list.
fn push_aarch64_visible_arg_as_target(
    emitter: &mut Emitter,
    frame_slot_offset: usize,
    incoming_ty: &PhpType,
    target_ty: &PhpType,
) {
    if incoming_ty.codegen_repr() == target_ty.codegen_repr() {
        push_aarch64_prepared_arg(emitter, frame_slot_offset, target_ty);
        return;
    }
    if incoming_ty.codegen_repr() != PhpType::Mixed {
        push_aarch64_prepared_arg(emitter, frame_slot_offset, incoming_ty);
        return;
    }

    abi::load_at_offset(emitter, "x0", frame_slot_offset);
    match target_ty.codegen_repr() {
        PhpType::Bool => {
            abi::emit_call_label(emitter, "__rt_mixed_cast_bool"); // cast boxed callback argument to bool for the target closure
            abi::emit_push_reg(emitter, "x0"); // push the converted bool callback argument
        }
        PhpType::Int | PhpType::Resource(_) => {
            abi::emit_call_label(emitter, "__rt_mixed_cast_int"); // cast boxed callback argument to int for the target closure
            abi::emit_push_reg(emitter, "x0"); // push the converted int callback argument
        }
        PhpType::Float => {
            abi::emit_call_label(emitter, "__rt_mixed_cast_float"); // cast boxed callback argument to float for the target closure
            abi::emit_push_float_reg(emitter, "d0"); // push the converted float callback argument
        }
        PhpType::Str => {
            abi::emit_call_label(emitter, "__rt_mixed_cast_string"); // cast boxed callback argument to string for the target closure
            abi::emit_push_reg_pair(emitter, "x1", "x2"); // push the converted string callback argument
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            abi::emit_call_label(emitter, "__rt_mixed_unbox"); // unwrap boxed callback argument for pointer-like target parameters
            abi::emit_push_reg(emitter, "x1"); // push the unboxed callback payload pointer
        }
    }
}

/// Pushes AArch64 prepared arg onto the temporary call stack or synthetic metadata list.
fn push_aarch64_prepared_arg(emitter: &mut Emitter, frame_slot_offset: usize, ty: &PhpType) {
    match ty.codegen_repr() {
        PhpType::Float => {
            abi::load_at_offset(emitter, "d0", frame_slot_offset);
            abi::emit_push_float_reg(emitter, "d0"); // push the prepared float argument onto the standard temporary call stack
        }
        PhpType::Str => {
            abi::load_at_offset(emitter, "x9", frame_slot_offset);
            abi::load_at_offset(emitter, "x10", frame_slot_offset - 8);
            abi::emit_push_reg_pair(emitter, "x9", "x10"); // push the prepared string argument pair onto the standard temporary call stack
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            abi::load_at_offset(emitter, "x9", frame_slot_offset);
            abi::emit_push_reg(emitter, "x9"); // push the prepared scalar/pointer argument onto the standard temporary call stack
        }
    }
}

/// Pushes x86 64 visible arg as target onto the temporary call stack or synthetic metadata list.
fn push_x86_64_visible_arg_as_target(
    emitter: &mut Emitter,
    slot_offset: usize,
    incoming_ty: &PhpType,
    target_ty: &PhpType,
) {
    if incoming_ty.codegen_repr() == target_ty.codegen_repr() {
        push_x86_64_prepared_arg(emitter, slot_offset, target_ty);
        return;
    }
    if incoming_ty.codegen_repr() != PhpType::Mixed {
        push_x86_64_prepared_arg(emitter, slot_offset, incoming_ty);
        return;
    }

    abi::load_at_offset(emitter, "rax", slot_offset);
    match target_ty.codegen_repr() {
        PhpType::Bool => {
            abi::emit_call_label(emitter, "__rt_mixed_cast_bool"); // cast boxed callback argument to bool for the target closure
            abi::emit_push_reg(emitter, "rax"); // push the converted bool callback argument
        }
        PhpType::Int | PhpType::Resource(_) => {
            abi::emit_call_label(emitter, "__rt_mixed_cast_int"); // cast boxed callback argument to int for the target closure
            abi::emit_push_reg(emitter, "rax"); // push the converted int callback argument
        }
        PhpType::Float => {
            abi::emit_call_label(emitter, "__rt_mixed_cast_float"); // cast boxed callback argument to float for the target closure
            abi::emit_push_float_reg(emitter, "xmm0"); // push the converted float callback argument
        }
        PhpType::Str => {
            abi::emit_call_label(emitter, "__rt_mixed_cast_string"); // cast boxed callback argument to string for the target closure
            abi::emit_push_reg_pair(emitter, "rax", "rdx"); // push the converted string callback argument
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            abi::emit_call_label(emitter, "__rt_mixed_unbox"); // unwrap boxed callback argument for pointer-like target parameters
            abi::emit_push_reg(emitter, "rdi"); // push the unboxed callback payload pointer
        }
    }
}

/// Pushes x86 64 prepared arg onto the temporary call stack or synthetic metadata list.
fn push_x86_64_prepared_arg(emitter: &mut Emitter, slot_offset: usize, ty: &PhpType) {
    match ty.codegen_repr() {
        PhpType::Float => {
            abi::load_at_offset(emitter, "xmm0", slot_offset);
            abi::emit_push_float_reg(emitter, "xmm0"); // push the prepared float argument onto the standard temporary call stack
        }
        PhpType::Str => {
            abi::load_at_offset(emitter, "r10", slot_offset);
            abi::load_at_offset(emitter, "r11", slot_offset - 8);
            abi::emit_push_reg_pair(emitter, "r10", "r11"); // push the prepared string argument pair onto the standard temporary call stack
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            abi::load_at_offset(emitter, "r10", slot_offset);
            abi::emit_push_reg(emitter, "r10"); // push the prepared scalar/pointer argument onto the standard temporary call stack
        }
    }
}

/// Returns the fixed stack slot offset for the idx-th incoming frame argument on x86_64.
/// Each slot occupies 16 bytes, and slot 0 is reserved for the return address.
fn frame_arg_slot_offset(idx: usize) -> usize {
    (idx + 1) * 16
}

/// Rounds `n` up to the nearest 16-byte boundary for stack alignment purposes.
fn align16(n: usize) -> usize {
    (n + 15) & !15
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies a Windows callback environment after two string arguments is loaded from the
    /// first MSx64 overflow slot, including the mandatory shadow-space offset.
    #[test]
    fn windows_callback_environment_after_string_pair_is_loaded_from_shadow_space() {
        let target = crate::codegen_support::platform::Target::new(Platform::Windows, Arch::X86_64);
        let mut emitter = Emitter::new(target);

        let env_reg = incoming_env_reg(&mut emitter, &[PhpType::Str, PhpType::Str]);
        let output = emitter.output();

        assert_eq!(env_reg, "r10");
        assert!(
            output.contains("mov r10, QWORD PTR [rbp + 48]"),
            "{}",
            output
        );
    }

    /// Verifies that an additional scalar overflow word advances the MSx64 environment slot by
    /// eight bytes rather than by the generic 16-byte Elephc temporary-slot width.
    #[test]
    fn windows_callback_environment_stack_offset_counts_multiword_visible_arguments() {
        let target = crate::codegen_support::platform::Target::new(Platform::Windows, Arch::X86_64);
        let mut emitter = Emitter::new(target);

        let env_reg = incoming_env_reg(&mut emitter, &[PhpType::Str, PhpType::Str, PhpType::Int]);
        let output = emitter.output();

        assert_eq!(env_reg, "r10");
        assert!(
            output.contains("mov r10, QWORD PTR [rbp + 56]"),
            "{}",
            output
        );
    }

    /// Verifies that an environment still in the fourth Windows integer register does not emit
    /// an unnecessary caller-stack load.
    #[test]
    fn windows_callback_environment_uses_fourth_integer_register_when_available() {
        let target = crate::codegen_support::platform::Target::new(Platform::Windows, Arch::X86_64);
        let mut emitter = Emitter::new(target);

        assert_eq!(incoming_env_reg(&mut emitter, &[PhpType::Int, PhpType::Int, PhpType::Int]), "r9");
    }

    /// Verifies a runtime callback wrapper reads two string descriptors from all
    /// four native MSx64 positional registers instead of treating the second
    /// descriptor as a fifth generated-PHP register argument.
    #[test]
    fn windows_callback_spills_two_string_descriptors_from_four_positional_slots() {
        let target = crate::codegen_support::platform::Target::new(Platform::Windows, Arch::X86_64);
        let mut emitter = Emitter::new(target);

        spill_visible_args(&mut emitter, &[PhpType::Str, PhpType::Str], false);
        let output = emitter.output();

        assert!(output.contains("mov QWORD PTR [rbp - 16], rcx"), "{output}");
        assert!(output.contains("mov QWORD PTR [rbp - 8], rdx"), "{output}");
        assert!(output.contains("mov QWORD PTR [rbp - 32], r8"), "{output}");
        assert!(output.contains("mov QWORD PTR [rbp - 24], r9"), "{output}");
    }
}
