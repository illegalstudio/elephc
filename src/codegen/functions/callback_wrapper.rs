use crate::codegen::context::DeferredCallbackWrapper;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::abi;
use crate::types::PhpType;

pub(crate) fn emit_callback_wrapper(emitter: &mut Emitter, wrapper: &DeferredCallbackWrapper) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64_callback_wrapper(emitter, wrapper);
        return;
    }

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

    spill_visible_args(emitter, &wrapper.visible_arg_types);
    spill_captures(
        emitter,
        wrapper.visible_arg_types.len(),
        &wrapper.capture_types,
        "x20",
    );

    let overflow_bytes = materialize_spilled_args_for_callback(emitter, &arg_types, frame_size);
    abi::emit_call_reg(emitter, "x19");
    abi::emit_release_temporary_stack(emitter, overflow_bytes);                 // drop stack-passed closure arguments after the adapted callback returns

    emitter.instruction(&format!("ldp x19, x20, [sp, #{}]", saved_callee_offset)); // restore wrapper callee-saved registers
    abi::emit_frame_restore(emitter, frame_size);
    abi::emit_return(emitter);
}

fn emit_x86_64_callback_wrapper(emitter: &mut Emitter, wrapper: &DeferredCallbackWrapper) {
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

    spill_visible_args(emitter, &wrapper.visible_arg_types);
    spill_captures(
        emitter,
        wrapper.visible_arg_types.len(),
        &wrapper.capture_types,
        "r13",
    );

    let overflow_bytes = materialize_spilled_args_for_callback_x86_64(emitter, &arg_types);
    abi::emit_call_reg(emitter, "r12");
    abi::emit_release_temporary_stack(emitter, overflow_bytes);                 // drop stack-passed closure arguments after the adapted callback returns

    abi::load_at_offset(emitter, "r13", saved_env_offset);
    abi::load_at_offset(emitter, "r12", saved_callback_offset);
    abi::emit_frame_restore(emitter, frame_size);
    abi::emit_return(emitter);
}

fn wrapper_arg_types(wrapper: &DeferredCallbackWrapper) -> Vec<PhpType> {
    wrapper
        .visible_arg_types
        .iter()
        .chain(wrapper.capture_types.iter())
        .map(PhpType::codegen_repr)
        .collect()
}

fn incoming_env_reg(emitter: &Emitter, visible_arg_types: &[PhpType]) -> &'static str {
    let mut incoming_types: Vec<PhpType> =
        visible_arg_types.iter().map(PhpType::codegen_repr).collect();
    incoming_types.push(PhpType::Pointer(None));
    let assignments = abi::build_outgoing_arg_assignments_for_target(emitter.target, &incoming_types, 0);
    let env_assignment = assignments
        .last()
        .expect("callback wrapper always has an environment pointer argument");
    debug_assert!(env_assignment.in_register());
    abi::int_arg_reg_name(emitter.target, env_assignment.start_reg)
}

fn spill_visible_args(emitter: &mut Emitter, visible_arg_types: &[PhpType]) {
    let visible_types: Vec<PhpType> = visible_arg_types.iter().map(PhpType::codegen_repr).collect();
    let assignments = abi::build_outgoing_arg_assignments_for_target(emitter.target, &visible_types, 0);
    for (idx, (ty, assignment)) in visible_types.iter().zip(assignments.iter()).enumerate() {
        debug_assert!(assignment.in_register());
        match (emitter.target.arch, ty) {
            (Arch::AArch64, PhpType::Float) => {
                let reg = abi::float_arg_reg_name(emitter.target, assignment.start_reg);
                emitter.instruction(&format!("str {}, [sp, #{}]", reg, idx * 16)); // spill the incoming float callback argument before loading captures
            }
            (Arch::AArch64, PhpType::Str) => {
                let ptr_reg = abi::int_arg_reg_name(emitter.target, assignment.start_reg);
                let len_reg = abi::int_arg_reg_name(emitter.target, assignment.start_reg + 1);
                emitter.instruction(&format!("stp {}, {}, [sp, #{}]", ptr_reg, len_reg, idx * 16)); // spill the incoming string callback argument before loading captures
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
                emitter.instruction(&format!("movsd xmm0, QWORD PTR [{} + {}]", env_reg, env_offset)); // load a captured float from the callback environment
                abi::store_at_offset(emitter, "xmm0", frame_arg_slot_offset(arg_idx));
            }
            (Arch::X86_64, PhpType::Str) => {
                emitter.instruction(&format!("mov r10, QWORD PTR [{} + {}]", env_reg, env_offset)); // load the captured string pointer from the callback environment
                emitter.instruction(&format!("mov r11, QWORD PTR [{} + {}]", env_reg, env_offset + 8)); // load the captured string length from the callback environment
                abi::store_at_offset(emitter, "r10", frame_arg_slot_offset(arg_idx));
                abi::store_at_offset(emitter, "r11", frame_arg_slot_offset(arg_idx) - 8);
            }
            (Arch::X86_64, PhpType::Void | PhpType::Never) => {}
            (Arch::X86_64, _) => {
                emitter.instruction(&format!("mov r10, QWORD PTR [{} + {}]", env_reg, env_offset)); // load a captured scalar/pointer from the callback environment
                abi::store_at_offset(emitter, "r10", frame_arg_slot_offset(arg_idx));
            }
        }
    }
}

fn materialize_spilled_args_for_callback(
    emitter: &mut Emitter,
    arg_types: &[PhpType],
    frame_size: usize,
) -> usize {
    push_spilled_args_as_call_temporaries(emitter, arg_types, frame_size);
    let assignments = abi::build_outgoing_arg_assignments_for_target(emitter.target, arg_types, 0);
    abi::materialize_outgoing_args(emitter, &assignments)
}

fn push_spilled_args_as_call_temporaries(
    emitter: &mut Emitter,
    arg_types: &[PhpType],
    frame_size: usize,
) {
    for (idx, ty) in arg_types.iter().enumerate() {
        let slot_offset = idx * 16;
        let frame_slot_offset = frame_size - 16 - slot_offset;
        match ty.codegen_repr() {
            PhpType::Float => {
                abi::load_at_offset(emitter, "d0", frame_slot_offset);
                abi::emit_push_float_reg(emitter, "d0");                       // push the prepared float argument onto the standard temporary call stack
            }
            PhpType::Str => {
                abi::load_at_offset(emitter, "x9", frame_slot_offset);
                abi::load_at_offset(emitter, "x10", frame_slot_offset - 8);
                abi::emit_push_reg_pair(emitter, "x9", "x10");                 // push the prepared string argument pair onto the standard temporary call stack
            }
            PhpType::Void | PhpType::Never => {}
            _ => {
                abi::load_at_offset(emitter, "x9", frame_slot_offset);
                abi::emit_push_reg(emitter, "x9");                              // push the prepared scalar/pointer argument onto the standard temporary call stack
            }
        }
    }
}

fn materialize_spilled_args_for_callback_x86_64(
    emitter: &mut Emitter,
    arg_types: &[PhpType],
) -> usize {
    push_spilled_args_as_call_temporaries_x86_64(emitter, arg_types);
    let assignments = abi::build_outgoing_arg_assignments_for_target(emitter.target, arg_types, 0);
    abi::materialize_outgoing_args(emitter, &assignments)
}

fn push_spilled_args_as_call_temporaries_x86_64(emitter: &mut Emitter, arg_types: &[PhpType]) {
    for (idx, ty) in arg_types.iter().enumerate() {
        let slot_offset = frame_arg_slot_offset(idx);
        match ty.codegen_repr() {
            PhpType::Float => {
                abi::load_at_offset(emitter, "xmm0", slot_offset);
                abi::emit_push_float_reg(emitter, "xmm0");                     // push the prepared float argument onto the standard temporary call stack
            }
            PhpType::Str => {
                abi::load_at_offset(emitter, "r10", slot_offset);
                abi::load_at_offset(emitter, "r11", slot_offset - 8);
                abi::emit_push_reg_pair(emitter, "r10", "r11");                // push the prepared string argument pair onto the standard temporary call stack
            }
            PhpType::Void | PhpType::Never => {}
            _ => {
                abi::load_at_offset(emitter, "r10", slot_offset);
                abi::emit_push_reg(emitter, "r10");                            // push the prepared scalar/pointer argument onto the standard temporary call stack
            }
        }
    }
}

fn frame_arg_slot_offset(idx: usize) -> usize {
    (idx + 1) * 16
}

fn align16(n: usize) -> usize {
    (n + 15) & !15
}
