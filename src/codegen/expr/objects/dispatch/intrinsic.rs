//! Purpose:
//! Lowers runtime-managed intrinsic method calls for core objects such as Fiber and Generator.
//! Keeps direct runtime-helper interception behind a shared `IntrinsicCall` registry.
//!
//! Called from:
//! - `crate::codegen::expr::objects::dispatch`
//!
//! Key details:
//! - Receivers and visible arguments are prepared by the normal method-call path before this file dispatches.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::intrinsics::{IntrinsicCall, IntrinsicCallForm, IntrinsicCallKind};
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::super::super::{
    coerce_result_to_type, emit_expr, restore_concat_offset_after_nested_call,
    restore_concat_offset_after_owned_string_call, save_concat_offset_before_nested_call,
};

pub(super) fn return_type_for(intrinsic: IntrinsicCall) -> PhpType {
    match intrinsic.kind() {
        IntrinsicCallKind::FiberIsStarted
        | IntrinsicCallKind::FiberIsRunning
        | IntrinsicCallKind::FiberIsSuspended
        | IntrinsicCallKind::FiberIsTerminated
        | IntrinsicCallKind::GeneratorValid => PhpType::Bool,
        IntrinsicCallKind::GeneratorNext | IntrinsicCallKind::GeneratorRewind => PhpType::Void,
        _ => PhpType::Mixed,
    }
}

pub(super) fn emit_static_intrinsic_call(
    intrinsic: IntrinsicCall,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    debug_assert_eq!(intrinsic.form(), IntrinsicCallForm::Static);
    emitter.comment(&format!(
        "{}::{}() intrinsic runtime dispatch",
        intrinsic.class_name(),
        intrinsic.method_key()
    ));

    match intrinsic.kind() {
        IntrinsicCallKind::FiberSuspend => {
            if let Some(value_expr) = args.first() {
                let actual_ty = emit_expr(value_expr, emitter, ctx, data);
                coerce_result_to_type(emitter, ctx, data, &actual_ty, &PhpType::Mixed);
            } else {
                abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
                coerce_result_to_type(emitter, ctx, data, &PhpType::Void, &PhpType::Mixed);
            }
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));           // shuttle the boxed Mixed pointer through the temporary stack
            abi::emit_pop_reg(emitter, abi::int_arg_reg_name(emitter.target, 0)); // pass the suspend value as runtime helper argument 1
            abi::emit_call_label(
                emitter,
                intrinsic
                    .runtime_helper()
                    .expect("Fiber::suspend intrinsic must have a runtime helper"),
            );                                                                  // suspend the current Fiber and return the resumed Mixed payload
            PhpType::Mixed
        }
        IntrinsicCallKind::FiberGetCurrent => {
            abi::emit_call_label(
                emitter,
                intrinsic
                    .runtime_helper()
                    .expect("Fiber::getCurrent intrinsic must have a runtime helper"),
            );                                                                  // read the currently running Fiber from runtime state
            PhpType::Mixed
        }
        other => {
            emitter.comment(&format!(
                "WARNING: unsupported static intrinsic {:?} for {}::{}",
                other,
                intrinsic.class_name(),
                intrinsic.method_key()
            ));
            return_type_for(intrinsic)
        }
    }
}

pub(super) fn emit_instance_intrinsic_with_loaded_args(
    intrinsic: IntrinsicCall,
    assignments: &[abi::OutgoingArgAssignment],
    _overflow_bytes: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    debug_assert_eq!(intrinsic.form(), IntrinsicCallForm::Instance);
    match intrinsic.kind() {
        IntrinsicCallKind::FiberStart => emit_fiber_start_intrinsic(intrinsic, assignments, emitter, ctx),
        IntrinsicCallKind::FiberResume
        | IntrinsicCallKind::FiberThrow
        | IntrinsicCallKind::FiberGetReturn => emit_simple_runtime_intrinsic(intrinsic, emitter),
        IntrinsicCallKind::FiberIsStarted
        | IntrinsicCallKind::FiberIsRunning
        | IntrinsicCallKind::FiberIsSuspended
        | IntrinsicCallKind::FiberIsTerminated => emit_fiber_state_intrinsic(intrinsic, emitter),
        IntrinsicCallKind::GeneratorCurrent
        | IntrinsicCallKind::GeneratorKey
        | IntrinsicCallKind::GeneratorNext
        | IntrinsicCallKind::GeneratorValid
        | IntrinsicCallKind::GeneratorRewind
        | IntrinsicCallKind::GeneratorSend
        | IntrinsicCallKind::GeneratorThrow
        | IntrinsicCallKind::GeneratorGetReturn => emit_generator_intrinsic(intrinsic, emitter, ctx),
        IntrinsicCallKind::FiberSuspend | IntrinsicCallKind::FiberGetCurrent => {
            emitter.comment(&format!(
                "WARNING: static intrinsic used as instance call {:?}",
                intrinsic.kind()
            ));
            return_type_for(intrinsic)
        }
    }
}

fn emit_fiber_start_intrinsic(
    intrinsic: IntrinsicCall,
    assignments: &[abi::OutgoingArgAssignment],
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    let max_arg_off = crate::codegen::runtime::FIBER_USER_ARG_MAX_OFFSET;
    let skip_label = ctx.next_label("fiber_start_args_done");
    let supplied_arg_count = assignments
        .len()
        .min(crate::codegen::runtime::FIBER_START_ARGS_MAX as usize);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x9, [x0, #{}]", max_arg_off));    // x9 = how many start_args slots start() may write
            for i in 0..supplied_arg_count {
                let src = abi::int_arg_reg_name(emitter.target, assignments[i].start_reg);
                let off = crate::codegen::runtime::FIBER_START_ARGS_OFFSET + (i as i32) * 8;
                emitter.instruction(&format!("cmp x9, #{}", i + 1));            // is this supplied argument still within user_arg_max?
                emitter.instruction(&format!("b.lt {}", skip_label));           // stop spilling once we hit the capture-reserved tail
                emitter.instruction(&format!("str {}, [x0, #{}]", src, off));   // start_args[i] = caller-supplied Mixed value
            }
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov r11, QWORD PTR [rdi + {}]", max_arg_off)); // r11 = how many start_args slots start() may write
            let mut overflow_slot = 0usize;
            for (i, assignment) in assignments.iter().take(supplied_arg_count).enumerate() {
                let off = crate::codegen::runtime::FIBER_START_ARGS_OFFSET + (i as i32) * 8;
                emitter.instruction(&format!("cmp r11, {}", i + 1));            // is this slot index still within user_arg_max?
                emitter.instruction(&format!("jl {}", skip_label));             // stop spilling once we hit the capture-reserved tail
                if assignment.in_register() {
                    let src = abi::int_arg_reg_name(emitter.target, assignment.start_reg);
                    emitter.instruction(&format!("mov QWORD PTR [rdi + {}], {}", off, src)); // start_args[i] = caller-supplied Mixed value
                } else {
                    let stack_offset = overflow_slot * 16;
                    if stack_offset == 0 {
                        emitter.instruction("mov r10, QWORD PTR [rsp]");        // load stack-passed start() Mixed argument from the top overflow slot
                    } else {
                        emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", stack_offset)); // load stack-passed start() Mixed argument from its overflow slot
                    }
                    emitter.instruction(&format!("mov QWORD PTR [rdi + {}], r10", off)); // start_args[i] = caller-supplied stack-passed Mixed value
                    overflow_slot += 1;
                }
            }
        }
    }
    emitter.label(&skip_label);
    abi::emit_call_label(
        emitter,
        intrinsic
            .runtime_helper()
            .expect("Fiber::start intrinsic must have a runtime helper"),
    );                                                                          // switch into the Fiber runtime and return the yielded Mixed value
    PhpType::Mixed
}

fn emit_simple_runtime_intrinsic(intrinsic: IntrinsicCall, emitter: &mut Emitter) -> PhpType {
    abi::emit_call_label(
        emitter,
        intrinsic
            .runtime_helper()
            .expect("simple intrinsic must have a runtime helper"),
    );                                                                          // call the runtime helper with the already materialized receiver and args
    return_type_for(intrinsic)
}

fn emit_fiber_state_intrinsic(intrinsic: IntrinsicCall, emitter: &mut Emitter) -> PhpType {
    let arg1 = abi::int_arg_reg_name(emitter.target, 1);
    let expected_state = match intrinsic.kind() {
        IntrinsicCallKind::FiberIsStarted => 0,
        IntrinsicCallKind::FiberIsRunning => 1,
        IntrinsicCallKind::FiberIsSuspended => 2,
        IntrinsicCallKind::FiberIsTerminated => 3,
        _ => unreachable!("fiber state intrinsic called with non-state kind"),
    };
    abi::emit_load_int_immediate(emitter, arg1, expected_state);                // pass the Fiber state value to compare against
    abi::emit_call_label(
        emitter,
        intrinsic
            .runtime_helper()
            .expect("Fiber state intrinsic must have a runtime helper"),
    );                                                                          // test the Fiber state through the shared runtime predicate
    if matches!(intrinsic.kind(), IntrinsicCallKind::FiberIsStarted) {
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("eor x0, x0, #1"),             // invert: isStarted means state is not NotStarted
            Arch::X86_64 => emitter.instruction("xor rax, 1"),                  // invert the boolean predicate result
        }
    }
    PhpType::Bool
}

fn emit_generator_intrinsic(
    intrinsic: IntrinsicCall,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    let ret_ty = return_type_for(intrinsic);
    save_concat_offset_before_nested_call(emitter, ctx);
    abi::emit_call_label(
        emitter,
        intrinsic
            .runtime_helper()
            .expect("Generator intrinsic must have a runtime helper"),
    );                                                                          // call directly into the Generator runtime helper
    if ret_ty == PhpType::Str {
        restore_concat_offset_after_owned_string_call(emitter, ctx);
    } else {
        restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
    }
    ret_ty
}
