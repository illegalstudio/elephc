//! Purpose:
//! Emits deferred fiber wrapper functions for callable bodies that execute inside runtime fibers.
//! Stitches closure captures, parameters, and resume results into normal function emission.
//!
//! Called from:
//! - `crate::codegen::functions` after deferred fiber wrappers are registered
//!
//! Key details:
//! - Wrapper frames must preserve captured values and follow the same cleanup rules as user functions.

use crate::codegen::context::DeferredFiberWrapper;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::arrays::emit_array_value_type_stamp;
use crate::codegen::platform::Arch;
use crate::codegen::{abi, callable_descriptor, runtime};
use crate::types::PhpType;

/// Emits a fiber wrapper that adapts a closure to run inside a runtime Fiber.
pub(crate) fn emit_fiber_wrapper(emitter: &mut Emitter, wrapper: &DeferredFiberWrapper) {
    if wrapper.use_descriptor_invoker {
        emit_descriptor_invoker_wrapper(emitter, &wrapper.label);
        return;
    }

    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64_wrapper(emitter, wrapper);
        return;
    }

    let arg_types = wrapper_arg_types(wrapper);
    let slot_count = arg_types.len().max(1);
    let frame_size = align16(slot_count * 16 + 48);
    let saved_callee_offset = frame_size - 48;

    emitter.blank();
    emitter.comment(&format!("fiber wrapper: {}", wrapper.label));
    emitter.raw(".align 2");
    emitter.label_global(&wrapper.label);
    abi::emit_frame_prologue(emitter, frame_size);
    emitter.instruction(&format!("stp x19, x20, [sp, #{}]", saved_callee_offset)); // preserve the fiber pointer and callable entry across helper calls
    emitter.instruction(&format!("str x21, [sp, #{}]", saved_callee_offset + 16)); // preserve the callable descriptor across helper calls
    emitter.instruction("mov x19, x0");                                         // x19 = Fiber object passed by __rt_fiber_entry
    emitter.instruction(&format!("ldr x20, [x19, #{}]", runtime::FIBER_CALLABLE_OFFSET)); // x20 = callable descriptor stored on the Fiber
    emitter.instruction("mov x21, x20");                                        // x21 = descriptor pointer kept for hidden capture reloads
    callable_descriptor::emit_load_entry_from_descriptor(emitter, "x20", "x20");

    spill_wrapper_args(emitter, wrapper, &arg_types, "x21");
    let overflow_bytes = materialize_spilled_args_for_closure_call(emitter, &arg_types, frame_size);
    let call_stack_padding = if overflow_bytes > 0 { 16 } else { 0 };
    abi::emit_reserve_temporary_stack(emitter, call_stack_padding);             // leave the first spilled callback argument where the callee expects it

    emitter.instruction("blr x20");                                             // call the original closure with ABI-correct arguments
    abi::emit_release_temporary_stack(emitter, call_stack_padding);             // drop the wrapper-only caller-stack alignment pad
    abi::emit_release_temporary_stack(emitter, overflow_bytes);                 // drop stack-passed closure arguments after the Fiber callback returns
    box_wrapper_return(emitter, wrapper.sig.return_type.codegen_repr());

    emitter.instruction(&format!("ldr x21, [sp, #{}]", saved_callee_offset + 16)); // restore the caller's descriptor scratch register
    emitter.instruction(&format!("ldp x19, x20, [sp, #{}]", saved_callee_offset)); // restore callee-saved wrapper registers
    abi::emit_frame_restore(emitter, frame_size);
    abi::emit_return(emitter);
}

/// Emits a generic Fiber wrapper that invokes the callable descriptor's uniform invoker.
fn emit_descriptor_invoker_wrapper(emitter: &mut Emitter, label: &str) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64_descriptor_invoker_wrapper(emitter, label);
    } else {
        emit_aarch64_descriptor_invoker_wrapper(emitter, label);
    }
}

/// Emits the ARM64 descriptor-invoker Fiber wrapper.
fn emit_aarch64_descriptor_invoker_wrapper(emitter: &mut Emitter, label: &str) {
    let frame_size = 96;
    let missing_label = format!("{}_missing_invoker", label);
    let loop_label = format!("{}_copy_args", label);
    let copy_done_label = format!("{}_args_done", label);
    let return_label = format!("{}_return", label);

    emitter.blank();
    emitter.comment(&format!("fiber descriptor invoker wrapper: {}", label));
    emitter.raw(".align 2");
    emitter.label_global(label);
    abi::emit_frame_prologue(emitter, frame_size);
    emitter.instruction("stp x19, x20, [sp, #0]");                              // preserve Fiber and descriptor registers across nested helper calls
    emitter.instruction("stp x21, x22, [sp, #16]");                             // preserve start-argument count and array registers
    emitter.instruction("stp x23, x24, [sp, #32]");                             // preserve loop index and invoker/result scratch registers
    emitter.instruction("str x25, [sp, #48]");                                  // preserve the boxed argument-container register

    emitter.instruction("mov x19, x0");                                         // x19 = Fiber object passed by __rt_fiber_entry
    emitter.instruction(&format!("ldr x20, [x19, #{}]", runtime::FIBER_CALLABLE_OFFSET)); // x20 = callable descriptor stored on the Fiber
    emitter.instruction(&format!("ldr x21, [x19, #{}]", runtime::FIBER_START_ARG_COUNT_OFFSET)); // x21 = number of boxed start() values to forward
    emit_allocate_descriptor_start_arg_array_aarch64(emitter);
    emit_copy_fiber_start_args_to_array_aarch64(emitter, &loop_label, &copy_done_label);
    emit_box_descriptor_start_arg_array(emitter, "x22", "x1");

    emitter.instruction("mov x25, x1");                                         // keep the boxed argument array alive across the descriptor invocation
    emitter.instruction("mov x0, x20");                                         // pass callable descriptor as invoker argument 1
    callable_descriptor::emit_load_invoker_from_descriptor(emitter, "x24", "x20");
    emitter.instruction(&format!("cbz x24, {}", missing_label));                // reject descriptors that do not expose the uniform invoker slot
    emitter.instruction("mov x1, x25");                                         // pass boxed start-argument array as invoker argument 2
    emitter.instruction("blr x24");                                             // invoke descriptor adapter; x0 = boxed Mixed return value
    emitter.instruction("mov x24, x0");                                         // preserve the Fiber callback return while releasing the argument container
    emitter.instruction("mov x0, x25");                                         // move the boxed argument container into the decref helper input
    emitter.instruction("bl __rt_decref_mixed");                                // release the temporary boxed argument container
    emitter.instruction("mov x0, x24");                                         // restore the callback return value for __rt_fiber_entry
    emitter.instruction(&format!("b {}", return_label));                        // skip the missing-invoker diagnostic path

    emitter.label(&missing_label);
    emitter.instruction("mov x0, x25");                                         // move the boxed argument container into the decref helper before throwing
    emitter.instruction("bl __rt_decref_mixed");                                // release the temporary boxed argument container on the error path
    abi::emit_symbol_address(emitter, "x0", "_fiber_msg_unsupported_callable"); // x0 = pointer to the unsupported-callable diagnostic
    emitter.instruction("mov x1, #48");                                         // x1 = diagnostic byte length
    emitter.instruction("bl __rt_fiber_throw_state_error");                     // raise FiberError through the fiber boundary handler
    emitter.instruction("brk #0xfffe");                                         // defensive trap: the throw helper must not return

    emitter.label(&return_label);
    emitter.instruction("ldr x25, [sp, #48]");                                  // restore the caller's x25 register
    emitter.instruction("ldp x23, x24, [sp, #32]");                             // restore loop/index scratch callee-saved registers
    emitter.instruction("ldp x21, x22, [sp, #16]");                             // restore count/array callee-saved registers
    emitter.instruction("ldp x19, x20, [sp, #0]");                              // restore Fiber/descriptor callee-saved registers
    abi::emit_frame_restore(emitter, frame_size);
    abi::emit_return(emitter);
}

/// Allocates the Mixed-pointer argument array used by an ARM64 descriptor invoker.
fn emit_allocate_descriptor_start_arg_array_aarch64(emitter: &mut Emitter) {
    emitter.instruction("mov x0, #4");                                          // default descriptor argument-array capacity
    emitter.instruction("cmp x21, #4");                                         // does the actual start() arity exceed the small-array default?
    emitter.instruction("csel x0, x21, x0, hi");                                // use the actual arity when it is larger than four
    emitter.instruction("mov x1, #8");                                          // descriptor argument arrays store boxed Mixed pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate the descriptor invoker argument array
    emitter.instruction("mov x22, x0");                                         // keep the argument array pointer across element retains
    emit_array_value_type_stamp(emitter, "x22", &PhpType::Mixed);
}

/// Copies Fiber start arguments into an ARM64 Mixed-pointer array, retaining each cell.
fn emit_copy_fiber_start_args_to_array_aarch64(
    emitter: &mut Emitter,
    loop_label: &str,
    done_label: &str,
) {
    emitter.instruction("mov x23, #0");                                         // start copying at start_args[0]
    emitter.label(loop_label);
    emitter.instruction("cmp x23, x21");                                        // have all supplied start() arguments been copied?
    emitter.instruction(&format!("b.hs {}", done_label));                       // leave the copy loop once index >= count
    emitter.instruction("lsl x9, x23, #3");                                     // convert the argument index into an 8-byte slot offset
    emitter.instruction(&format!("add x10, x19, #{}", runtime::FIBER_START_ARGS_OFFSET)); // x10 = base of Fiber start_args storage
    emitter.instruction("ldr x0, [x10, x9]");                                   // load the boxed Mixed start argument
    emitter.instruction("bl __rt_incref");                                      // retain the boxed Mixed cell for the temporary invoker array
    emitter.instruction("lsl x9, x23, #3");                                     // recompute the element offset after the retain helper clobbers scratch regs
    emitter.instruction("add x11, x22, #24");                                   // x11 = first payload slot of the descriptor argument array
    emitter.instruction("add x11, x11, x9");                                    // x11 = destination slot for the current boxed argument
    emitter.instruction("str x0, [x11]");                                       // store the retained boxed Mixed pointer into the argument array
    emitter.instruction("add x23, x23, #1");                                    // advance to the next supplied start() argument
    emitter.instruction(&format!("b {}", loop_label));                          // continue copying boxed start() arguments
    emitter.label(done_label);
    emitter.instruction("str x21, [x22]");                                      // publish the argument array length after all payload slots are initialized
}

/// Boxes the descriptor start-argument array into the invoker's target argument register.
fn emit_box_descriptor_start_arg_array(emitter: &mut Emitter, source_reg: &str, dest_reg: &str) {
    let array_ty = PhpType::Array(Box::new(PhpType::Mixed));
    emitter.instruction(&format!("mov {}, {}", dest_reg, source_reg));          // move the argument array pointer into the invoker argument register
    crate::codegen::builtins::arrays::call_user_func_array::emit_box_invoker_arg_clone_as_mixed(
        dest_reg,
        &array_ty,
        emitter,
    );
}

/// Emits the x86_64 descriptor-invoker Fiber wrapper.
fn emit_x86_64_descriptor_invoker_wrapper(emitter: &mut Emitter, label: &str) {
    let frame_size = 96;
    let saved_fiber_offset = 16;
    let saved_descriptor_offset = 24;
    let saved_count_offset = 32;
    let saved_array_offset = 40;
    let saved_argbox_offset = 48;
    let missing_label = format!("{}_missing_invoker", label);
    let loop_label = format!("{}_copy_args", label);
    let copy_done_label = format!("{}_args_done", label);
    let return_label = format!("{}_return", label);

    emitter.blank();
    emitter.comment(&format!("fiber descriptor invoker wrapper: {}", label));
    emitter.raw(".align 16");
    emitter.label_global(label);
    abi::emit_frame_prologue(emitter, frame_size);
    abi::store_at_offset(emitter, "r12", saved_fiber_offset);
    abi::store_at_offset(emitter, "r13", saved_descriptor_offset);
    abi::store_at_offset(emitter, "r14", saved_count_offset);
    abi::store_at_offset(emitter, "r15", saved_array_offset);
    abi::store_at_offset(emitter, "rbx", saved_argbox_offset);

    emitter.instruction("mov r12, rdi");                                        // r12 = Fiber object passed by __rt_fiber_entry
    emitter.instruction(&format!("mov r13, QWORD PTR [r12 + {}]", runtime::FIBER_CALLABLE_OFFSET)); // r13 = callable descriptor stored on the Fiber
    emitter.instruction(&format!("mov r14, QWORD PTR [r12 + {}]", runtime::FIBER_START_ARG_COUNT_OFFSET)); // r14 = number of boxed start() values to forward
    emit_allocate_descriptor_start_arg_array_x86_64(emitter);
    emit_copy_fiber_start_args_to_array_x86_64(emitter, &loop_label, &copy_done_label);
    emit_box_descriptor_start_arg_array(emitter, "r15", "rsi");

    emitter.instruction("mov rbx, rsi");                                        // keep the boxed argument array alive across the descriptor invocation
    emitter.instruction("mov rdi, r13");                                        // pass callable descriptor as invoker argument 1
    callable_descriptor::emit_load_invoker_from_descriptor(emitter, "r10", "r13");
    emitter.instruction(&format!("test r10, r10"));                             // check whether the descriptor exposes a uniform invoker slot
    emitter.instruction(&format!("je {}", missing_label));                      // reject descriptors that cannot be called through the generic path
    emitter.instruction("mov rsi, rbx");                                        // pass boxed start-argument array as invoker argument 2
    emitter.instruction("call r10");                                            // invoke descriptor adapter; rax = boxed Mixed return value
    emitter.instruction("mov r15, rax");                                        // preserve the Fiber callback return while releasing the argument container
    emitter.instruction("mov rax, rbx");                                        // move the boxed argument container into the decref helper input
    emitter.instruction("call __rt_decref_mixed");                              // release the temporary boxed argument container
    emitter.instruction("mov rax, r15");                                        // restore the callback return value for __rt_fiber_entry
    emitter.instruction(&format!("jmp {}", return_label));                      // skip the missing-invoker diagnostic path

    emitter.label(&missing_label);
    emitter.instruction("mov rax, rbx");                                        // move the boxed argument container into the decref helper before throwing
    emitter.instruction("call __rt_decref_mixed");                              // release the temporary boxed argument container on the error path
    abi::emit_symbol_address(emitter, "rdi", "_fiber_msg_unsupported_callable"); // rdi = pointer to the unsupported-callable diagnostic
    emitter.instruction("mov esi, 48");                                         // rsi = diagnostic byte length
    emitter.instruction("call __rt_fiber_throw_state_error");                   // raise FiberError through the fiber boundary handler
    emitter.instruction("ud2");                                                 // defensive trap: the throw helper must not return

    emitter.label(&return_label);
    abi::load_at_offset(emitter, "rbx", saved_argbox_offset);
    abi::load_at_offset(emitter, "r15", saved_array_offset);
    abi::load_at_offset(emitter, "r14", saved_count_offset);
    abi::load_at_offset(emitter, "r13", saved_descriptor_offset);
    abi::load_at_offset(emitter, "r12", saved_fiber_offset);
    abi::emit_frame_restore(emitter, frame_size);
    abi::emit_return(emitter);
}

/// Allocates the Mixed-pointer argument array used by an x86_64 descriptor invoker.
fn emit_allocate_descriptor_start_arg_array_x86_64(emitter: &mut Emitter) {
    emitter.instruction("mov rdi, 4");                                          // default descriptor argument-array capacity
    emitter.instruction("cmp r14, 4");                                          // does the actual start() arity exceed the small-array default?
    emitter.instruction("cmova rdi, r14");                                      // use the actual arity when it is larger than four
    emitter.instruction("mov rsi, 8");                                          // descriptor argument arrays store boxed Mixed pointers
    emitter.instruction("call __rt_array_new");                                 // allocate the descriptor invoker argument array
    emitter.instruction("mov r15, rax");                                        // keep the argument array pointer across element retains
    emit_array_value_type_stamp(emitter, "r15", &PhpType::Mixed);
}

/// Copies Fiber start arguments into an x86_64 Mixed-pointer array, retaining each cell.
fn emit_copy_fiber_start_args_to_array_x86_64(
    emitter: &mut Emitter,
    loop_label: &str,
    done_label: &str,
) {
    emitter.instruction("xor ebx, ebx");                                        // start copying at start_args[0]
    emitter.label(loop_label);
    emitter.instruction("cmp rbx, r14");                                        // have all supplied start() arguments been copied?
    emitter.instruction(&format!("jae {}", done_label));                        // leave the copy loop once index >= count
    emitter.instruction(&format!("mov rax, QWORD PTR [r12 + rbx * 8 + {}]", runtime::FIBER_START_ARGS_OFFSET)); // load the boxed Mixed start argument
    emitter.instruction("call __rt_incref");                                    // retain the boxed Mixed cell for the temporary invoker array
    emitter.instruction("mov QWORD PTR [r15 + 24 + rbx * 8], rax");             // store the retained boxed Mixed pointer into the argument array
    emitter.instruction("add rbx, 1");                                          // advance to the next supplied start() argument
    emitter.instruction(&format!("jmp {}", loop_label));                        // continue copying boxed start() arguments
    emitter.label(done_label);
    emitter.instruction("mov QWORD PTR [r15], r14");                            // publish the argument array length after all payload slots are initialized
}

/// Spills visible parameters and hidden arguments from the Fiber's argument storage
/// into the wrapper's stack frame, calculating how many integer/float registers each
/// argument consumes so the caller's ABI expectations are met at the final call site.
/// Visible params are read directly from the Fiber's start arguments; hidden args come
/// after and use the same offset scheme.
fn spill_wrapper_args(
    emitter: &mut Emitter,
    wrapper: &DeferredFiberWrapper,
    arg_types: &[PhpType],
    descriptor_reg: &str,
) {
    let visible = wrapper.visible_param_count.min(arg_types.len());

    for (idx, ty) in arg_types.iter().take(visible).enumerate() {
        spill_user_arg(emitter, idx, ty, idx * 16);
    }

    for (idx, ty) in arg_types.iter().enumerate().skip(visible) {
        let slot_offset = idx * 16;
        spill_descriptor_hidden_arg(emitter, descriptor_reg, idx - visible, ty, slot_offset);
    }
}

/// Spills one hidden Fiber callback argument from the callable descriptor's runtime capture slots.
fn spill_descriptor_hidden_arg(
    emitter: &mut Emitter,
    descriptor_reg: &str,
    capture_index: usize,
    ty: &PhpType,
    slot_offset: usize,
) {
    callable_descriptor::emit_load_runtime_capture_to_result(
        emitter,
        descriptor_reg,
        capture_index,
        ty,
    );
    match ty {
        PhpType::Float => {
            emitter.instruction(&format!("str d0, [sp, #{}]", slot_offset));    // spill the descriptor-captured float for the final call
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            emitter.instruction(&format!("stp {}, {}, [sp, #{}]", ptr_reg, len_reg, slot_offset)); // spill the descriptor-captured string pair for the final call
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            emitter.instruction(&format!("str {}, [sp, #{}]", abi::int_result_reg(emitter), slot_offset)); // spill the descriptor-captured payload for the final call
            retain_refcounted_capture_for_closure_frame(emitter, ty, abi::int_result_reg(emitter));
        }
    }
}

/// Retains a refcounted hidden capture for the closure parameter frame.
///
/// The callable descriptor remains the persistent owner of its capture slots. The
/// invoked closure cleans up hidden parameters like ordinary arguments, so the wrapper
/// must hand it a separate retained owner for refcounted values.
fn retain_refcounted_capture_for_closure_frame(
    emitter: &mut Emitter,
    ty: &PhpType,
    value_reg: &str,
) {
    if !ty.is_refcounted() && !matches!(ty.codegen_repr(), PhpType::Callable) {
        return;
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, {}", value_reg));             // pass the descriptor capture to the retain helper
            emitter.instruction("bl __rt_incref");                              // retain it for the closure frame's normal parameter cleanup
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, {}", value_reg));            // pass the descriptor capture to the retain helper
            emitter.instruction("call __rt_incref");                            // retain it for the closure frame's normal parameter cleanup
        }
    }
}

/// Builds the argument type list for a fiber wrapper by mapping the wrapper's visible
/// parameters and hidden arguments to their codegen representations, in order.
fn wrapper_arg_types(wrapper: &DeferredFiberWrapper) -> Vec<PhpType> {
    wrapper
        .sig
        .params
        .iter()
        .map(|(_, ty)| ty.codegen_repr())
        .chain(wrapper.hidden_arg_types.iter().map(PhpType::codegen_repr))
        .collect()
}

/// Loads a visible user parameter from the Fiber object's argument area and spills it
/// into the wrapper's stack frame at the slot corresponding to its parameter index.
/// Unboxes the boxed Mixed argument from the Fiber's start area; for Float and Str
/// types also handles the ABI register layout transformation for the final call.
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

/// Materializes the spilled wrapper arguments back into ABI registers/stack slots for
/// the closure call. First pushes all spilled args as call temporaries onto the
/// temporary stack, then builds outgoing argument assignments for the target and
/// materializes them. Returns the number of overflow bytes that must be cleaned up
/// after the call returns.
fn materialize_spilled_args_for_closure_call(
    emitter: &mut Emitter,
    arg_types: &[PhpType],
    frame_size: usize,
) -> usize {
    push_spilled_args_as_call_temporaries(emitter, arg_types, frame_size);
    let assignments = abi::build_outgoing_arg_assignments_for_target(emitter.target, arg_types, 0);
    abi::materialize_outgoing_args(emitter, &assignments)
}

/// Pushes each spilled wrapper argument from its frame slot onto the temporary call
/// stack in preparation for the closure call. Arguments are pushed in reverse order
/// so they land at the correct stack offsets for the callee; Float, Str, and scalar
/// types each use their appropriate register-pair or single-register push sequence.
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
                let reg = match emitter.target.arch {
                    Arch::AArch64 => "d0",
                    Arch::X86_64 => "xmm0",
                };
                abi::load_at_offset(emitter, reg, frame_slot_offset);
                abi::emit_push_float_reg(emitter, reg);                         // push the prepared float argument onto the standard temporary call stack
            }
            PhpType::Str => {
                let (ptr_reg, len_reg) = match emitter.target.arch {
                    Arch::AArch64 => ("x9", "x10"),
                    Arch::X86_64 => ("r10", "r11"),
                };
                abi::load_at_offset(emitter, ptr_reg, frame_slot_offset);
                abi::load_at_offset(emitter, len_reg, frame_slot_offset - 8);
                abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);            // push the prepared string argument pair onto the standard temporary call stack
            }
            PhpType::Void | PhpType::Never => {}
            _ => {
                let reg = match emitter.target.arch {
                    Arch::AArch64 => "x9",
                    Arch::X86_64 => "r10",
                };
                abi::load_at_offset(emitter, reg, frame_slot_offset);
                abi::emit_push_reg(emitter, reg);                              // push the prepared scalar/pointer argument onto the standard temporary call stack
            }
        }
    }
}

/// Boxes the closure's raw return value into a Mixed cell for the Fiber's result slot.
/// For Void/Never return types, normalizes the implicit null to 0/NULL so the boxed
/// representation is consistent before the wrapper returns to the fiber entry point.
fn box_wrapper_return(emitter: &mut Emitter, return_ty: PhpType) {
    if matches!(return_ty, PhpType::Void | PhpType::Never) {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x0, #0");                              // normalize implicit/null closure returns before boxing as Mixed
            }
            Arch::X86_64 => {
                emitter.instruction("xor eax, eax");                            // normalize implicit/null closure returns before boxing as Mixed
            }
        }
    }
    crate::codegen::emit_box_current_value_as_mixed(emitter, &return_ty);
}

/// x86_64-specific fiber wrapper emission. Uses the System V AMD64 ABI for both the
/// wrapper frame and the closure call; preserves r12/r13 as callee-saved Fiber/callable
/// pointers across the call. Differs from ARM64 in register layout and frame slot indexing.
fn emit_x86_64_wrapper(emitter: &mut Emitter, wrapper: &DeferredFiberWrapper) {
    let arg_types = wrapper_arg_types(wrapper);
    let slot_count = arg_types.len().max(1);
    let frame_size = align16(slot_count * 16 + 64);
    let saved_fiber_offset = slot_count * 16 + 16;
    let saved_callable_offset = slot_count * 16 + 24;
    let saved_descriptor_offset = slot_count * 16 + 32;

    emitter.blank();
    emitter.comment(&format!("fiber wrapper: {}", wrapper.label));
    emitter.raw(".align 16");
    emitter.label_global(&wrapper.label);
    abi::emit_frame_prologue(emitter, frame_size);
    abi::store_at_offset(emitter, "r12", saved_fiber_offset);                  // preserve the caller's r12 before caching the Fiber pointer
    abi::store_at_offset(emitter, "r13", saved_callable_offset);               // preserve the caller's r13 before caching the callable entry
    abi::store_at_offset(emitter, "r14", saved_descriptor_offset);             // preserve the caller's r14 before caching the descriptor
    emitter.instruction("mov r12, rdi");                                        // r12 = Fiber object passed by __rt_fiber_entry
    emitter.instruction(&format!("mov r13, QWORD PTR [r12 + {}]", runtime::FIBER_CALLABLE_OFFSET)); // r13 = callable descriptor stored on the Fiber
    emitter.instruction("mov r14, r13");                                        // r14 = descriptor pointer kept for hidden capture reloads
    callable_descriptor::emit_load_entry_from_descriptor(emitter, "r13", "r13");

    spill_wrapper_args_x86_64(emitter, wrapper, &arg_types, "r14");
    let overflow_bytes = materialize_spilled_args_for_closure_call_x86_64(emitter, &arg_types);
    abi::emit_call_reg(emitter, "r13");
    abi::emit_release_temporary_stack(emitter, overflow_bytes);                 // drop stack-passed closure arguments after the Fiber callback returns
    box_wrapper_return(emitter, wrapper.sig.return_type.codegen_repr());

    abi::load_at_offset(emitter, "r14", saved_descriptor_offset);
    abi::load_at_offset(emitter, "r13", saved_callable_offset);
    abi::load_at_offset(emitter, "r12", saved_fiber_offset);
    abi::emit_frame_restore(emitter, frame_size);
    abi::emit_return(emitter);
}

/// x86_64-specific spilling of visible parameters and hidden arguments from the Fiber's
/// argument storage into the wrapper frame. Uses r12 to address the Fiber and accesses
/// float/string/scalar slots via the same offset scheme as ARM64 but with x86_64 load
/// instructions and frame slot offsets computed by frame_arg_slot_offset().
fn spill_wrapper_args_x86_64(
    emitter: &mut Emitter,
    wrapper: &DeferredFiberWrapper,
    arg_types: &[PhpType],
    descriptor_reg: &str,
) {
    let visible = wrapper.visible_param_count.min(arg_types.len());

    for (idx, ty) in arg_types.iter().take(visible).enumerate() {
        spill_user_arg_x86_64(emitter, idx, ty, frame_arg_slot_offset(idx));
    }

    for (idx, ty) in arg_types.iter().enumerate().skip(visible) {
        let slot_offset = frame_arg_slot_offset(idx);
        spill_descriptor_hidden_arg_x86_64(emitter, descriptor_reg, idx - visible, ty, slot_offset);
    }
}

/// x86_64-specific spill of one hidden argument from descriptor runtime capture storage.
fn spill_descriptor_hidden_arg_x86_64(
    emitter: &mut Emitter,
    descriptor_reg: &str,
    capture_index: usize,
    ty: &PhpType,
    slot_offset: usize,
) {
    callable_descriptor::emit_load_runtime_capture_to_result(
        emitter,
        descriptor_reg,
        capture_index,
        ty,
    );
    match ty {
        PhpType::Float => {
            abi::store_at_offset(emitter, "xmm0", slot_offset);
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::store_at_offset(emitter, ptr_reg, slot_offset);
            abi::store_at_offset(emitter, len_reg, slot_offset - 8);
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            abi::store_at_offset(emitter, abi::int_result_reg(emitter), slot_offset);
            retain_refcounted_capture_for_closure_frame(emitter, ty, abi::int_result_reg(emitter));
        }
    }
}

/// x86_64-specific loading and unboxing of a visible user parameter from the Fiber
/// object's start argument area into the wrapper's frame slot. Unboxes via the same
/// __rt_mixed_unbox helper; Float requires movq from rdi to xmm0 for bit reinterpretation.
fn spill_user_arg_x86_64(emitter: &mut Emitter, param_idx: usize, ty: &PhpType, slot_offset: usize) {
    let src_offset = runtime::FIBER_START_ARGS_OFFSET + (param_idx as i32) * 8;
    emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", src_offset)); // load the boxed Mixed start() argument from the Fiber object

    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::store_at_offset(emitter, "rax", slot_offset);
        return;
    }

    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    match ty {
        PhpType::Float => {
            emitter.instruction("movq xmm0, rdi");                              // reinterpret the unboxed float payload bits as xmm0
            abi::store_at_offset(emitter, "xmm0", slot_offset);
        }
        PhpType::Str => {
            abi::store_at_offset(emitter, "rdi", slot_offset);
            abi::store_at_offset(emitter, "rdx", slot_offset - 8);
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            abi::store_at_offset(emitter, "rdi", slot_offset);
        }
    }
}

/// x86_64-specific materialization of spilled wrapper arguments into ABI registers/stack
/// for the closure call. Differs from ARM64 in that it does not pass frame_size since
/// x86_64 frame slot offsets are computed directly from the slot index.
fn materialize_spilled_args_for_closure_call_x86_64(
    emitter: &mut Emitter,
    arg_types: &[PhpType],
) -> usize {
    push_spilled_args_as_call_temporaries_x86_64(emitter, arg_types);
    let assignments = abi::build_outgoing_arg_assignments_for_target(emitter.target, arg_types, 0);
    abi::materialize_outgoing_args(emitter, &assignments)
}

/// x86_64-specific pushing of spilled wrapper arguments from frame slots onto the temporary
/// call stack. Arguments are pushed in reverse order; uses xmm0 for float push and
/// r10/r11 register pair for string push, matching the System V AMD64 ABI conventions.
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

/// Computes the x86_64 frame slot offset for argument slot `idx`. Each slot occupies
/// 16 bytes; slot 0 is reserved (holds the return address), so argument i uses slot i+1.
fn frame_arg_slot_offset(idx: usize) -> usize {
    (idx + 1) * 16
}

/// Rounds `n` up to the nearest 16-byte aligned value. Used to compute frame sizes
/// that satisfy the ABI requirement for callee-saved register spill space and stack
/// alignment at calls.
fn align16(n: usize) -> usize {
    (n + 15) & !15
}
