//! Purpose:
//! Lowers fiber-aware method dispatch and wrapper registration.
//! Shares receiver preparation and ABI call conventions with the object call dispatcher.
//!
//! Called from:
//! - `crate::codegen::expr::objects::dispatch`
//!
//! Key details:
//! - Receiver ownership, late/static binding, and vtable slot layout must match class metadata emission.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::super::super::{coerce_result_to_type, emit_expr};

pub(super) fn emit_fiber_static_method_dispatch(
    method: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment(&format!("Fiber::{}() — runtime dispatch", method));
    match method {
        "suspend" => {
            // Coerce the supplied value (or the implicit null) to Mixed so the
            // runtime always sees an 8-byte heap-cell pointer in transfer_value.
            if let Some(value_expr) = args.first() {
                let actual_ty = emit_expr(value_expr, emitter, ctx, data);
                coerce_result_to_type(emitter, ctx, data, &actual_ty, &PhpType::Mixed);
            } else {
                abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0); // default suspend value placeholder — coerced into Mixed below
                coerce_result_to_type(emitter, ctx, data, &PhpType::Void, &PhpType::Mixed);
            }
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));               // shuttle the boxed Mixed pointer through the stack to land it in arg-reg 0
            abi::emit_pop_reg(emitter, abi::int_arg_reg_name(emitter.target, 0));    // pop the Mixed pointer into the first integer argument register
            abi::emit_call_label(emitter, "__rt_fiber_suspend");
            PhpType::Mixed
        }
        "getcurrent" => {
            abi::emit_call_label(emitter, "__rt_fiber_get_current");
            PhpType::Mixed
        }
        other => {
            emitter.comment(&format!("WARNING: unknown Fiber static method {}", other));
            PhpType::Mixed
        }
    }
}

/// Codegen interception for instance method calls on `Fiber`. By the time we
/// reach this point, `$this` is already loaded into integer arg-reg 0 and any
/// declared arguments are in arg-regs 1..N. We bypass vtable dispatch and call
/// the matching `__rt_fiber_*` helper directly.
pub(super) fn emit_fiber_instance_method_dispatch(
    method: &str,
    assignments: &[abi::OutgoingArgAssignment],
    _overflow_bytes: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    use crate::codegen::platform::Arch;
    let arg1 = abi::int_arg_reg_name(emitter.target, 1);
    match method {
        "start" => {
            // start() takes up to 7 Mixed arguments which the codegen has just
            // loaded into integer arg-regs 1..7. Spill them into the Fiber
            // object's start_args[0..7] slots so the trampoline can hand them
            // to the closure on the fresh fiber stack.
            //
            // Honour `user_arg_max` so trailing slots that hold pre-loaded
            // closure captures (set by `new Fiber(function() use(...))`) are
            // not overwritten. user_arg_max defaults to 7, so fibers built
            // from a non-capturing callable still fill every slot.
            let max_arg_off = crate::codegen::runtime::FIBER_USER_ARG_MAX_OFFSET;
            let skip_label = ctx.next_label("fiber_start_args_done");
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction(&format!("ldr x9, [x0, #{}]", max_arg_off)); // x9 = how many start_args slots start() may write
                    for i in 0..crate::codegen::runtime::FIBER_START_ARGS_MAX {
                        let src = abi::int_arg_reg_name(emitter.target, (i as usize) + 1);
                        let off = crate::codegen::runtime::FIBER_START_ARGS_OFFSET + i * 8;
                        emitter.instruction(&format!("cmp x9, #{}", i + 1));    // is this slot index still within user_arg_max?
                        emitter.instruction(&format!("b.lt {}", skip_label));   // stop spilling once we hit the capture-reserved tail
                        emitter.instruction(&format!("str {}, [x0, #{}]", src, off)); // start_args[i] = caller-supplied Mixed value
                    }
                }
                Arch::X86_64 => {
                    emitter.instruction(&format!("mov r11, QWORD PTR [rdi + {}]", max_arg_off)); // r11 = how many start_args slots start() may write
                    let mut overflow_slot = 0usize;
                    for i in 0..crate::codegen::runtime::FIBER_START_ARGS_MAX {
                        let Some(assignment) = assignments.get(i as usize) else {
                            break;
                        };
                        let off = crate::codegen::runtime::FIBER_START_ARGS_OFFSET + i * 8;
                        emitter.instruction(&format!("cmp r11, {}", i + 1));    // is this slot index still within user_arg_max?
                        emitter.instruction(&format!("jl {}", skip_label));     // stop spilling once we hit the capture-reserved tail
                        if assignment.in_register() {
                            let src = abi::int_arg_reg_name(emitter.target, assignment.start_reg);
                            emitter.instruction(&format!("mov QWORD PTR [rdi + {}], {}", off, src)); // start_args[i] = caller-supplied Mixed value
                        } else {
                            let stack_offset = overflow_slot * 16;
                            if stack_offset == 0 {
                                emitter.instruction("mov r10, QWORD PTR [rsp]"); // load stack-passed start() Mixed argument from the top overflow slot
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
            abi::emit_call_label(emitter, "__rt_fiber_start");
            PhpType::Mixed
        }
        "resume" => {
            abi::emit_call_label(emitter, "__rt_fiber_resume");
            PhpType::Mixed
        }
        "throw" => {
            abi::emit_call_label(emitter, "__rt_fiber_throw");
            PhpType::Mixed
        }
        "getreturn" => {
            abi::emit_call_label(emitter, "__rt_fiber_get_return");
            PhpType::Mixed
        }
        "isstarted" => {
            // isStarted is true whenever the fiber state is NOT NotStarted (== 0).
            abi::emit_load_int_immediate(emitter, arg1, 0);                     // FIBER_STATE_NOT_STARTED
            abi::emit_call_label(emitter, "__rt_fiber_state_eq");
            match emitter.target.arch {
                Arch::AArch64 => emitter.instruction("eor x0, x0, #1"),         // invert: !(state == NotStarted)
                Arch::X86_64 => emitter.instruction("xor rax, 1"),              // invert the boolean predicate result
            }
            PhpType::Bool
        }
        "isrunning" => {
            abi::emit_load_int_immediate(emitter, arg1, 1);                     // FIBER_STATE_RUNNING
            abi::emit_call_label(emitter, "__rt_fiber_state_eq");
            PhpType::Bool
        }
        "issuspended" => {
            abi::emit_load_int_immediate(emitter, arg1, 2);                     // FIBER_STATE_SUSPENDED
            abi::emit_call_label(emitter, "__rt_fiber_state_eq");
            PhpType::Bool
        }
        "isterminated" => {
            abi::emit_load_int_immediate(emitter, arg1, 3);                     // FIBER_STATE_TERMINATED
            abi::emit_call_label(emitter, "__rt_fiber_state_eq");
            PhpType::Bool
        }
        other => {
            emitter.comment(&format!("WARNING: unknown Fiber method {}", other));
            PhpType::Mixed
        }
    }
}
