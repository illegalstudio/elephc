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

/// Maps an `IntrinsicCallKind` to its PHP return type.
///
/// Used by both static and instance intrinsic lowering to determine the result type
/// when emitting calls or falling back after an unsupported intrinsic warning.
pub(super) fn return_type_for(intrinsic: IntrinsicCall) -> PhpType {
    match intrinsic.kind() {
        IntrinsicCallKind::FiberIsStarted
        | IntrinsicCallKind::FiberIsRunning
        | IntrinsicCallKind::FiberIsSuspended
        | IntrinsicCallKind::FiberIsTerminated
        | IntrinsicCallKind::GeneratorValid
        | IntrinsicCallKind::CallbackFilterAccept
        | IntrinsicCallKind::SplDllIsEmpty
        | IntrinsicCallKind::SplDllOffsetExists
        | IntrinsicCallKind::SplDllValid
        | IntrinsicCallKind::SplFixedOffsetExists => PhpType::Bool,
        IntrinsicCallKind::SplRecursiveAssumeIterator => {
            PhpType::Object("RecursiveIterator".to_string())
        }
        IntrinsicCallKind::SplDllCount
        | IntrinsicCallKind::SplDllGetIteratorMode
        | IntrinsicCallKind::SplFixedCount
        | IntrinsicCallKind::SplFixedGetSize => PhpType::Int,
        IntrinsicCallKind::SplDllSerialize => PhpType::Str,
        IntrinsicCallKind::SplFixedToArray | IntrinsicCallKind::SplFixedJsonSerialize => {
            PhpType::Array(Box::new(PhpType::Mixed))
        }
        IntrinsicCallKind::SplDllSerializeArray => PhpType::Array(Box::new(PhpType::Mixed)),
        IntrinsicCallKind::SplFixedFromArray => {
            PhpType::Object("SplFixedArray".to_string())
        }
        IntrinsicCallKind::GeneratorNext
        | IntrinsicCallKind::GeneratorRewind
        | IntrinsicCallKind::SplDllAdd
        | IntrinsicCallKind::SplDllPush
        | IntrinsicCallKind::SplDllUnshift
        | IntrinsicCallKind::SplDllSetIteratorMode
        | IntrinsicCallKind::SplDllUnserialize
        | IntrinsicCallKind::SplDllOffsetSet
        | IntrinsicCallKind::SplDllOffsetUnset
        | IntrinsicCallKind::SplDllRewind
        | IntrinsicCallKind::SplDllPrev
        | IntrinsicCallKind::SplDllNext
        | IntrinsicCallKind::SplQueueEnqueue
        | IntrinsicCallKind::SplFixedConstruct
        | IntrinsicCallKind::SplFixedSetSize
        | IntrinsicCallKind::SplFixedUnserialize
        | IntrinsicCallKind::SplFixedOffsetSet
        | IntrinsicCallKind::SplFixedOffsetUnset => PhpType::Void,
        _ => PhpType::Mixed,
    }
}

/// Lowers a static intrinsic call such as `Fiber::suspend(...)` or `SplFixedArray::fromArray(...)`.
///
/// Arguments are emitted and coerced before the runtime helper is called. For `Fiber::suspend`,
/// the argument (or `null`) is shuttled through the temporary stack before being passed as the
/// first integer argument to the helper. For `SplFixedArray::fromArray`, the source array and
/// optional `preserveKeys` boolean are passed as arguments 2 and 3 after the class ID in arg 0.
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
        IntrinsicCallKind::SplFixedFromArray => {
            let Some(array_expr) = args.first() else {
                emitter.comment("WARNING: SplFixedArray::fromArray() intrinsic missing array argument");
                return return_type_for(intrinsic);
            };
            let array_ty = emit_expr(array_expr, emitter, ctx, data);
            coerce_result_to_type(emitter, ctx, data, &array_ty, &PhpType::Array(Box::new(PhpType::Mixed)));
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));          // preserve the source array while optional arguments are evaluated
            if let Some(preserve_expr) = args.get(1) {
                let preserve_ty = emit_expr(preserve_expr, emitter, ctx, data);
                coerce_result_to_type(emitter, ctx, data, &preserve_ty, &PhpType::Bool);
                abi::emit_push_reg(emitter, abi::int_result_reg(emitter));      // preserve the runtime preserveKeys flag for the SPL helper
            } else {
                abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 1);
                abi::emit_push_reg(emitter, abi::int_result_reg(emitter));      // default preserveKeys=true, matching PHP
            }
            let class_id = ctx
                .classes
                .get("SplFixedArray")
                .map(|info| info.class_id)
                .unwrap_or(u64::MAX);
            abi::emit_pop_reg(emitter, abi::int_arg_reg_name(emitter.target, 2)); // pass preserveKeys as runtime helper argument 3
            abi::emit_pop_reg(emitter, abi::int_arg_reg_name(emitter.target, 1)); // pass the source PHP array as runtime helper argument 2
            abi::emit_load_int_immediate(
                emitter,
                abi::int_arg_reg_name(emitter.target, 0),
                class_id as i64,
            );
            abi::emit_call_label(
                emitter,
                intrinsic
                    .runtime_helper()
                    .expect("SplFixedArray::fromArray intrinsic must have a runtime helper"),
            );                                                                  // build a SplFixedArray from the source PHP array
            PhpType::Object("SplFixedArray".to_string())
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

/// Lowers an instance intrinsic call where arguments are already materialized in registers.
///
/// The receiver is in `x0` (ARM64) or `rdi` (x86_64). Additional arguments are sourced from
/// `assignments` which describes where each argument currently lives (register or stack overflow).
/// `overflow_bytes` describes how many bytes of stack arguments were passed beyond the ABI limit.
/// Falls through to `emit_simple_runtime_intrinsic` for most kinds; handles `Fiber::start` specially.
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
        IntrinsicCallKind::CallbackFilterAccept => {
            emit_callback_filter_accept_intrinsic(intrinsic, emitter, ctx)
        }
        IntrinsicCallKind::SplRecursiveAssumeIterator => {
            emit_recursive_assume_iterator_intrinsic(emitter, ctx)
        }
        IntrinsicCallKind::SplDllAdd
        | IntrinsicCallKind::SplDllPop
        | IntrinsicCallKind::SplDllShift
        | IntrinsicCallKind::SplDllPush
        | IntrinsicCallKind::SplDllUnshift
        | IntrinsicCallKind::SplDllTop
        | IntrinsicCallKind::SplDllBottom
        | IntrinsicCallKind::SplDllCount
        | IntrinsicCallKind::SplDllIsEmpty
        | IntrinsicCallKind::SplDllSetIteratorMode
        | IntrinsicCallKind::SplDllGetIteratorMode
        | IntrinsicCallKind::SplDllSerialize
        | IntrinsicCallKind::SplDllUnserialize
        | IntrinsicCallKind::SplDllSerializeArray
        | IntrinsicCallKind::SplDllOffsetExists
        | IntrinsicCallKind::SplDllOffsetGet
        | IntrinsicCallKind::SplDllOffsetSet
        | IntrinsicCallKind::SplDllOffsetUnset
        | IntrinsicCallKind::SplDllRewind
        | IntrinsicCallKind::SplDllCurrent
        | IntrinsicCallKind::SplDllKey
        | IntrinsicCallKind::SplDllPrev
        | IntrinsicCallKind::SplDllNext
        | IntrinsicCallKind::SplDllValid
        | IntrinsicCallKind::SplQueueEnqueue
        | IntrinsicCallKind::SplQueueDequeue
        | IntrinsicCallKind::SplFixedConstruct
        | IntrinsicCallKind::SplFixedCount
        | IntrinsicCallKind::SplFixedToArray
        | IntrinsicCallKind::SplFixedGetSize
        | IntrinsicCallKind::SplFixedSetSize
        | IntrinsicCallKind::SplFixedOffsetExists
        | IntrinsicCallKind::SplFixedOffsetGet
        | IntrinsicCallKind::SplFixedOffsetSet
        | IntrinsicCallKind::SplFixedOffsetUnset
        | IntrinsicCallKind::SplFixedJsonSerialize
        | IntrinsicCallKind::SplFixedUnserialize => emit_simple_runtime_intrinsic(intrinsic, emitter),
        IntrinsicCallKind::FiberSuspend
        | IntrinsicCallKind::FiberGetCurrent
        | IntrinsicCallKind::SplFixedFromArray => {
            emitter.comment(&format!(
                "WARNING: static intrinsic used as instance call {:?}",
                intrinsic.kind()
            ));
            return_type_for(intrinsic)
        }
    }
}

/// Lowers `Fiber::start` by copying user-supplied start arguments into the Fiber's start_args buffer
/// before invoking the runtime helper.
///
/// Uses `FIBER_USER_ARG_MAX_OFFSET` to limit how many slots the callee may write, and
/// `FIBER_START_ARGS_OFFSET` as the base for the start_args array. Only arguments up to
/// `assignments.len()` are copied, capped by `FIBER_START_ARGS_MAX`. ARM64 spills registers
/// directly; x86_64 handles stack overflow slots by loading from the known overflow area.
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

/// Lowers simple instance intrinsics that only need a runtime helper call with no special setup.
///
/// Receiver and arguments are already materialized in registers per the ABI. Calls the
/// runtime helper directly and returns the type for the intrinsic kind.
fn emit_simple_runtime_intrinsic(intrinsic: IntrinsicCall, emitter: &mut Emitter) -> PhpType {
    abi::emit_call_label(
        emitter,
        intrinsic
            .runtime_helper()
            .expect("simple intrinsic must have a runtime helper"),
    );                                                                          // call the runtime helper with the already materialized receiver and args
    return_type_for(intrinsic)
}

/// Lowers Fiber state-query intrinsics (`Fiber::isStarted`, `isRunning`, `isSuspended`, `isTerminated`).
///
/// Sets argument 1 to the expected state enum value (0–3), calls the shared runtime predicate helper,
/// then inverts the result for `Fiber::isStarted` since the runtime encodes `NotStarted` as the
/// absence of the Started state (state == 0 means not started).
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

/// Lowers Generator intrinsics (`Generator::current`, `key`, `next`, `valid`, `rewind`, `send`, `throw`, `getReturn`).
///
/// Saves concat offsets before the call to preserve nested string operations, calls the Generator
/// runtime helper directly, then restores offsets after based on whether the result type is `Str`.
/// Returns the PHP return type for the intrinsic kind.
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

/// Emits assembly for callback filter accept intrinsic.
fn emit_callback_filter_accept_intrinsic(
    intrinsic: IntrinsicCall,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    let Some(class_info) = ctx.classes.get(intrinsic.class_name()) else {
        emitter.comment("WARNING: missing CallbackFilterIterator metadata for callback accept");
        return PhpType::Bool;
    };
    let callback_offset = class_info.property_offsets.get("callback").copied().unwrap_or(24);
    let callback_env_offset = class_info
        .property_offsets
        .get("callbackEnv")
        .copied()
        .unwrap_or(40);
    let direct_call = ctx.next_label("callback_filter_direct_call");
    let done = ctx.next_label("callback_filter_call_done");

    save_concat_offset_before_nested_call(emitter, ctx);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x9, [x0, #{}]", callback_offset)); // load the stored callback descriptor pointer
            emitter.instruction(&format!("ldr x10, [x0, #{}]", callback_env_offset)); // load the optional persistent callback environment
            crate::codegen::callable_descriptor::emit_load_entry_from_descriptor(
                emitter,
                "x9",
                "x9",
            );
            emitter.instruction("mov x0, x1");                                  // shift current value into callback argument 1
            emitter.instruction("mov x1, x2");                                  // shift current key into callback argument 2
            emitter.instruction("mov x2, x3");                                  // shift inner iterator into callback argument 3
            emitter.instruction(&format!("cbz x10, {}", direct_call));          // call the original callback directly when no env is stored
            emitter.instruction("mov x3, x10");                                 // pass persistent capture env as the wrapper's hidden argument
            emitter.instruction("blr x9");                                      // invoke the stored callback wrapper with captures
            emitter.instruction(&format!("b {}", done));                        // skip the direct-call path after wrapper dispatch
            emitter.label(&direct_call);
            emitter.instruction("blr x9");                                      // invoke the stored callback without hidden captures
            emitter.label(&done);
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov r10, QWORD PTR [rdi + {}]", callback_offset)); // load the stored callback descriptor pointer
            emitter.instruction(&format!("mov r11, QWORD PTR [rdi + {}]", callback_env_offset)); // load the optional persistent callback environment
            crate::codegen::callable_descriptor::emit_load_entry_from_descriptor(
                emitter,
                "r10",
                "r10",
            );
            emitter.instruction("mov rdi, rsi");                                // shift current value into callback argument 1
            emitter.instruction("mov rsi, rdx");                                // shift current key into callback argument 2
            emitter.instruction("mov rdx, rcx");                                // shift inner iterator into callback argument 3
            emitter.instruction("test r11, r11");                               // check whether a persistent callback environment exists
            emitter.instruction(&format!("je {}", direct_call));                // call the original callback directly when no env is stored
            emitter.instruction("mov rcx, r11");                                // pass persistent capture env as the wrapper's hidden argument
            emitter.instruction("call r10");                                    // invoke the stored callback wrapper with captures
            emitter.instruction(&format!("jmp {}", done));                      // skip the direct-call path after wrapper dispatch
            emitter.label(&direct_call);
            emitter.instruction("call r10");                                    // invoke the stored callback without hidden captures
            emitter.label(&done);
        }
    }
    restore_concat_offset_after_nested_call(emitter, ctx, &PhpType::Bool);
    PhpType::Bool
}

/// Emits assembly for recursive assume iterator intrinsic.
fn emit_recursive_assume_iterator_intrinsic(
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    save_concat_offset_before_nested_call(emitter, ctx);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, x1");                                  // move the boxed candidate iterator into the mixed-unbox helper input
            emitter.instruction("bl __rt_mixed_unbox");                         // unwrap the candidate so the raw object pointer can be returned
            emitter.instruction("mov x0, x1");                                  // return the unboxed object payload as RecursiveIterator
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, rsi");                                // move the boxed candidate iterator into the mixed-unbox helper input
            emitter.instruction("call __rt_mixed_unbox");                       // unwrap the candidate so the raw object pointer can be returned
            emitter.instruction("mov rax, rdi");                                // return the unboxed object payload as RecursiveIterator
        }
    }
    let ret_ty = PhpType::Object("RecursiveIterator".to_string());
    restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
    ret_ty
}
