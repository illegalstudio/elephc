use crate::codegen::context::DeferredFiberWrapper;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::{abi, runtime};
use crate::types::PhpType;

pub(crate) fn emit_fiber_wrapper(emitter: &mut Emitter, wrapper: &DeferredFiberWrapper) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64_stub(emitter, &wrapper.label);
        return;
    }

    let arg_types: Vec<PhpType> = wrapper
        .sig
        .params
        .iter()
        .map(|(_, ty)| ty.codegen_repr())
        .collect();
    let slot_count = arg_types.len().max(1);
    let frame_size = align16(slot_count * 16 + 32);
    let saved_callee_offset = frame_size - 32;

    emitter.blank();
    emitter.comment(&format!("fiber wrapper: {}", wrapper.label));
    emitter.raw(".align 2");
    emitter.label_global(&wrapper.label);
    abi::emit_frame_prologue(emitter, frame_size);
    emitter.instruction(&format!("stp x19, x20, [sp, #{}]", saved_callee_offset)); // preserve the fiber pointer and callable pointer across helper calls
    emitter.instruction("mov x19, x0");                                         // x19 = Fiber object passed by __rt_fiber_entry
    emitter.instruction(&format!("ldr x20, [x19, #{}]", runtime::FIBER_CALLABLE_OFFSET)); // x20 = original closure function pointer stored on the Fiber

    spill_wrapper_args(emitter, wrapper, &arg_types);
    let overflow_bytes = materialize_spilled_args_for_closure_call(emitter, &arg_types, frame_size);
    let call_stack_padding = if overflow_bytes > 0 { 16 } else { 0 };
    abi::emit_reserve_temporary_stack(emitter, call_stack_padding);             // leave the first spilled callback argument where the callee expects it

    emitter.instruction("blr x20");                                             // call the original closure with ABI-correct arguments
    abi::emit_release_temporary_stack(emitter, call_stack_padding);             // drop the wrapper-only caller-stack alignment pad
    abi::emit_release_temporary_stack(emitter, overflow_bytes);                 // drop stack-passed closure arguments after the Fiber callback returns
    box_wrapper_return(emitter, wrapper.sig.return_type.codegen_repr());

    emitter.instruction(&format!("ldp x19, x20, [sp, #{}]", saved_callee_offset)); // restore callee-saved wrapper registers
    abi::emit_frame_restore(emitter, frame_size);
    abi::emit_return(emitter);
}

fn spill_wrapper_args(emitter: &mut Emitter, wrapper: &DeferredFiberWrapper, arg_types: &[PhpType]) {
    let visible = wrapper.visible_param_count.min(arg_types.len());
    let user_int_regs = arg_types
        .iter()
        .take(visible)
        .filter(|ty| !ty.is_float_reg())
        .map(PhpType::register_count)
        .sum::<usize>();
    let user_float_regs = arg_types
        .iter()
        .take(visible)
        .filter(|ty| ty.is_float_reg())
        .count();

    for (idx, ty) in arg_types.iter().take(visible).enumerate() {
        spill_user_arg(emitter, idx, ty, idx * 16);
    }

    let mut int_capture_slot = user_int_regs;
    let mut float_capture_slot = user_float_regs;
    for (idx, ty) in arg_types.iter().enumerate().skip(visible) {
        let slot_offset = idx * 16;
        match ty {
            PhpType::Float => {
                let src_offset = runtime::FIBER_FLOAT_ARGS_OFFSET + (float_capture_slot as i32) * 8;
                emitter.instruction(&format!("ldr d0, [x19, #{}]", src_offset)); // load the captured float payload from the Fiber float slot file
                emitter.instruction(&format!("str d0, [sp, #{}]", slot_offset)); // spill the captured float until all helper calls are done
                float_capture_slot += 1;
            }
            PhpType::Str => {
                let src_lo = runtime::FIBER_START_ARGS_OFFSET + (int_capture_slot as i32) * 8;
                let src_hi = runtime::FIBER_START_ARGS_OFFSET + ((int_capture_slot + 1) as i32) * 8;
                emitter.instruction(&format!("ldr x9, [x19, #{}]", src_lo));    // load the captured string pointer from the Fiber int slot file
                emitter.instruction(&format!("ldr x10, [x19, #{}]", src_hi));   // load the captured string length from the Fiber int slot file
                emitter.instruction(&format!("stp x9, x10, [sp, #{}]", slot_offset)); // spill the captured string register pair for the final call
                int_capture_slot += 2;
            }
            PhpType::Void | PhpType::Never => {}
            _ => {
                let src_offset = runtime::FIBER_START_ARGS_OFFSET + (int_capture_slot as i32) * 8;
                emitter.instruction(&format!("ldr x9, [x19, #{}]", src_offset)); // load the captured scalar/pointer payload from the Fiber int slot file
                emitter.instruction(&format!("str x9, [sp, #{}]", slot_offset)); // spill the captured payload for the final closure call
                int_capture_slot += 1;
            }
        }
    }
}

fn spill_user_arg(emitter: &mut Emitter, param_idx: usize, ty: &PhpType, slot_offset: usize) {
    let src_offset = runtime::FIBER_START_ARGS_OFFSET + (param_idx as i32) * 8;
    emitter.instruction(&format!("ldr x0, [x19, #{}]", src_offset));            // load the boxed Mixed start() argument from the Fiber object

    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        emitter.instruction(&format!("str x0, [sp, #{}]", slot_offset));        // pass mixed parameters as their boxed cell pointer
        return;
    }

    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    match ty {
        PhpType::Float => {
            emitter.instruction("fmov d0, x1");                                 // reinterpret the unboxed float payload bits as d0
            emitter.instruction(&format!("str d0, [sp, #{}]", slot_offset));    // spill the normalized float argument for the final call
        }
        PhpType::Str => {
            emitter.instruction(&format!("stp x1, x2, [sp, #{}]", slot_offset)); // spill the unboxed string pointer and length for the final call
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            emitter.instruction(&format!("str x1, [sp, #{}]", slot_offset));    // spill the unboxed scalar/pointer payload for the final call
        }
    }
}

fn materialize_spilled_args_for_closure_call(
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
                abi::emit_push_float_reg(emitter, "d0");                         // push the prepared float argument onto the standard temporary call stack
            }
            PhpType::Str => {
                abi::load_at_offset(emitter, "x9", frame_slot_offset);
                abi::load_at_offset(emitter, "x10", frame_slot_offset - 8);
                abi::emit_push_reg_pair(emitter, "x9", "x10");                  // push the prepared string argument pair onto the standard temporary call stack
            }
            PhpType::Void | PhpType::Never => {}
            _ => {
                abi::load_at_offset(emitter, "x9", frame_slot_offset);
                abi::emit_push_reg(emitter, "x9");                              // push the prepared scalar/pointer argument onto the standard temporary call stack
            }
        }
    }
}

fn box_wrapper_return(emitter: &mut Emitter, return_ty: PhpType) {
    if matches!(return_ty, PhpType::Void | PhpType::Never) {
        emitter.instruction("mov x0, #0");                                      // normalize implicit/null closure returns before boxing as Mixed
    }
    crate::codegen::emit_box_current_value_as_mixed(emitter, &return_ty);
}

fn emit_x86_64_stub(emitter: &mut Emitter, label: &str) {
    emitter.blank();
    emitter.comment(&format!("fiber wrapper stub: {}", label));
    emitter.label_global(label);
    emitter.instruction("ret");                                                 // x86_64 fibers are runtime stubs in this feature
}

fn align16(n: usize) -> usize {
    (n + 15) & !15
}
