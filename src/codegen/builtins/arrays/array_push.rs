//! Purpose:
//! Emits PHP `array_push` builtin calls that mutate array arguments in place.
//! Handles COW preparation and writes any replacement array pointer back to caller storage.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Mutating/ref-like arguments must avoid value-temp preevaluation so PHP-visible storage is updated.

use super::store_mutating_arg::emit_store_mutating_arg;
use crate::codegen::abi;
use crate::codegen::context::{Context, HeapOwnership};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{emit_expr, expr_result_heap_ownership};
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_push()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    if emitter.target.arch == Arch::X86_64 {
        emit_array_push_linux_x86_64(args, &arr_ty, emitter, ctx, data);
        return Some(PhpType::Void);
    }

    // -- save array pointer, evaluate value to push --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push array pointer onto stack
    let elem_ty = indexed_array_elem_type(&arr_ty);
    let source_owned = expr_result_heap_ownership(&args[1]) == HeapOwnership::Owned;
    let mut val_ty = emit_expr(&args[1], emitter, ctx, data);
    let boxed_iterable =
        crate::codegen::emit_box_iterable_value_for_mixed_container(emitter, &mut val_ty);
    let effective_elem_ty = effective_indexed_push_type(&elem_ty, &val_ty, ctx);
    let converted_to_mixed =
        matches!(effective_elem_ty, PhpType::Mixed) && !matches!(elem_ty, PhpType::Mixed);
    let mut boxed_value_for_mixed = false;
    if matches!(effective_elem_ty, PhpType::Mixed)
        && !matches!(val_ty, PhpType::Mixed | PhpType::Union(_))
    {
        crate::codegen::emit_box_current_expr_value_as_mixed_for_container(
            emitter, &args[1], &val_ty,
        );
        val_ty = PhpType::Mixed;
        boxed_value_for_mixed = true;
    } else if matches!(effective_elem_ty, PhpType::Mixed) && matches!(val_ty, PhpType::Union(_)) {
        val_ty = PhpType::Mixed;
    }
    let release_after_refcounted_push = boxed_value_for_mixed
        || boxed_iterable
        || (source_owned
            && matches!(
                val_ty,
                PhpType::Mixed
                    | PhpType::Union(_)
                    | PhpType::Array(_)
                    | PhpType::AssocArray { .. }
                    | PhpType::Object(_)
            ));
    emitter.instruction("ldr x9, [sp], #16");                                   // pop saved array pointer into x9
    if elem_ty != effective_elem_ty {
        update_array_push_arg_type(&args[0], &effective_elem_ty, ctx);
    }
    if converted_to_mixed {
        emitter.instruction("str x0, [sp, #-16]!");                             // preserve the boxed pushed value across mixed-array conversion
        emitter.instruction("mov x0, x9");                                      // pass the current indexed-array pointer to the mixed conversion helper
        abi::emit_load_int_immediate(
            emitter,
            "x1",
            crate::codegen::runtime_value_tag(&elem_ty) as i64,
        );
        abi::emit_call_label(emitter, "__rt_array_to_mixed");                   // box existing typed slots before array_push stores a heterogeneous value
        emitter.instruction("mov x9, x0");                                      // keep the converted indexed-array pointer as the push receiver
        emitter.instruction("ldr x0, [sp], #16");                               // restore the boxed pushed value after conversion
    }
    match &val_ty {
        PhpType::Int | PhpType::Bool => {
            // -- push integer/bool value onto array --
            emitter.instruction("mov x1, x0");                                  // move integer value to x1 (second arg)
            emitter.instruction("mov x0, x9");                                  // move array pointer to x0 (first arg)
            emitter.instruction("bl __rt_array_push_int");                      // call runtime: append integer to array
        }
        PhpType::Float => {
            // -- push float value onto array (store as 8-byte int via bit cast) --
            emitter.instruction("fmov x1, d0");                                 // move float bits to integer register
            emitter.instruction("mov x0, x9");                                  // move array pointer to x0 (first arg)
            emitter.instruction("bl __rt_array_push_int");                      // call runtime: append float bits as 8 bytes
        }
        PhpType::Str => {
            // -- push string to array (push_str persists to heap internally) --
            emitter.instruction("mov x0, x9");                                  // move array pointer to x0
            emitter.instruction("bl __rt_array_push_str");                      // call runtime: persist + append string to array
        }
        PhpType::Mixed | PhpType::Union(_) | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
            // -- push nested refcounted pointer onto array --
            if release_after_refcounted_push {
                abi::emit_push_reg(emitter, "x0");
            }
            emitter.instruction("mov x1, x0");                                  // move pointer value to x1
            emitter.instruction("mov x0, x9");                                  // move outer array pointer to x0
            emitter.instruction("bl __rt_array_push_refcounted");               // append retained pointer and stamp array metadata
            if release_after_refcounted_push {
                crate::codegen::emit_release_pushed_refcounted_temp_after_array_push(emitter, &val_ty);
            }
        }
        PhpType::Callable => {
            // -- push callable pointer onto array as a plain 8-byte scalar --
            emitter.instruction("mov x1, x0");                                  // move callable pointer value to x1
            emitter.instruction("mov x0, x9");                                  // move outer array pointer to x0
            emitter.instruction("bl __rt_array_push_int");                      // append function pointer bits as a plain scalar slot
        }
        _ => {}
    }

    // -- update stored array pointer (may have changed due to COW splitting or reallocation) --
    emit_store_mutating_arg(emitter, ctx, &args[0]);

    Some(PhpType::Void)
}

fn emit_array_push_linux_x86_64(
    args: &[Expr],
    arr_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    abi::emit_push_reg(emitter, "rax");                                          // preserve the indexed-array pointer while evaluating the appended value
    let elem_ty = indexed_array_elem_type(arr_ty);
    let source_owned = expr_result_heap_ownership(&args[1]) == HeapOwnership::Owned;
    let mut val_ty = emit_expr(&args[1], emitter, ctx, data);
    let boxed_iterable =
        crate::codegen::emit_box_iterable_value_for_mixed_container(emitter, &mut val_ty);
    let effective_elem_ty = effective_indexed_push_type(&elem_ty, &val_ty, ctx);
    let converted_to_mixed =
        matches!(effective_elem_ty, PhpType::Mixed) && !matches!(elem_ty, PhpType::Mixed);
    let mut boxed_value_for_mixed = false;
    if matches!(effective_elem_ty, PhpType::Mixed)
        && !matches!(val_ty, PhpType::Mixed | PhpType::Union(_))
    {
        crate::codegen::emit_box_current_expr_value_as_mixed_for_container(
            emitter, &args[1], &val_ty,
        );
        val_ty = PhpType::Mixed;
        boxed_value_for_mixed = true;
    } else if matches!(effective_elem_ty, PhpType::Mixed) && matches!(val_ty, PhpType::Union(_)) {
        val_ty = PhpType::Mixed;
    }
    let release_after_refcounted_push = boxed_value_for_mixed
        || boxed_iterable
        || (source_owned
            && matches!(
                val_ty,
                PhpType::Mixed
                    | PhpType::Union(_)
                    | PhpType::Array(_)
                    | PhpType::AssocArray { .. }
                    | PhpType::Object(_)
            ));
    abi::emit_pop_reg(emitter, "r11");                                           // restore the indexed-array pointer after evaluating the appended value
    if elem_ty != effective_elem_ty {
        update_array_push_arg_type(&args[0], &effective_elem_ty, ctx);
    }
    if converted_to_mixed {
        abi::emit_push_reg(emitter, "rax");                                      // preserve the boxed pushed value across mixed-array conversion
        emitter.instruction("mov rdi, r11");                                    // pass the current indexed-array pointer to the mixed conversion helper
        abi::emit_load_int_immediate(
            emitter,
            "rsi",
            crate::codegen::runtime_value_tag(&elem_ty) as i64,
        );
        abi::emit_call_label(emitter, "__rt_array_to_mixed");                   // box existing typed slots before array_push stores a heterogeneous value
        emitter.instruction("mov r11, rax");                                    // keep the converted indexed-array pointer as the push receiver
        abi::emit_pop_reg(emitter, "rax");                                      // restore the boxed pushed value after conversion
    }
    match &val_ty {
        PhpType::Int | PhpType::Bool => {
            emitter.instruction("mov rsi, rax");                                // place the appended scalar payload in the x86_64 runtime value register
            emitter.instruction("mov rdi, r11");                                // place the indexed-array pointer in the x86_64 runtime receiver register
            abi::emit_call_label(emitter, "__rt_array_push_int");                // append the scalar payload and return the possibly-grown indexed-array pointer
        }
        PhpType::Float => {
            emitter.instruction("movq rsi, xmm0");                              // move the floating-point payload bits into the scalar append register
            emitter.instruction("mov rdi, r11");                                // place the indexed-array pointer in the x86_64 runtime receiver register
            abi::emit_call_label(emitter, "__rt_array_push_int");                // append the floating-point payload bits as an 8-byte scalar slot
        }
        PhpType::Str => {
            emitter.instruction("mov rsi, rax");                                // place the appended string pointer in the x86_64 runtime payload register
            emitter.instruction("mov rdi, r11");                                // place the indexed-array pointer in the x86_64 runtime receiver register
            abi::emit_call_label(emitter, "__rt_array_push_str");                // persist and append the string payload, returning the possibly-grown indexed-array pointer
        }
        PhpType::Callable => {
            emitter.instruction("mov rsi, rax");                                // place the callable pointer bits in the x86_64 scalar append register
            emitter.instruction("mov rdi, r11");                                // place the indexed-array pointer in the x86_64 runtime receiver register
            abi::emit_call_label(emitter, "__rt_array_push_int");                // append the callable pointer bits as a plain scalar slot
        }
        PhpType::Mixed | PhpType::Union(_) | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
            if release_after_refcounted_push {
                abi::emit_push_reg(emitter, "rax");
            }
            emitter.instruction("mov rsi, rax");                                // place the retained refcounted payload pointer in the x86_64 runtime child register
            emitter.instruction("mov rdi, r11");                                // place the indexed-array pointer in the x86_64 runtime receiver register
            abi::emit_call_label(emitter, "__rt_array_push_refcounted");         // append the retained heap payload and stamp the indexed-array value_type metadata
            if release_after_refcounted_push {
                crate::codegen::emit_release_pushed_refcounted_temp_after_array_push(emitter, &val_ty);
            }
        }
        _ => {}
    }

    emit_store_mutating_arg(emitter, ctx, &args[0]);                             // publish the possibly-grown indexed-array pointer back through the mutating argument slot
}

fn indexed_array_elem_type(arr_ty: &PhpType) -> PhpType {
    match arr_ty {
        PhpType::Array(elem_ty) => *elem_ty.clone(),
        _ => PhpType::Int,
    }
}

fn effective_indexed_push_type(existing: &PhpType, value: &PhpType, ctx: &Context) -> PhpType {
    if matches!(existing, PhpType::Never) {
        return if matches!(value, PhpType::Union(_)) {
            PhpType::Mixed
        } else {
            value.clone()
        };
    }
    if matches!(value, PhpType::Never) {
        return existing.clone();
    }
    if matches!(existing, PhpType::Mixed) || matches!(value, PhpType::Mixed | PhpType::Union(_)) {
        PhpType::Mixed
    } else if existing == value {
        existing.clone()
    } else if let (PhpType::Object(left), PhpType::Object(right)) = (existing, value) {
        ctx.common_object_type(left, right).unwrap_or(PhpType::Mixed)
    } else {
        PhpType::Mixed
    }
}

fn update_array_push_arg_type(arg: &Expr, elem_ty: &PhpType, ctx: &mut Context) {
    if let crate::parser::ast::ExprKind::Variable(name) = &arg.kind {
        let updated_ty = PhpType::Array(Box::new(elem_ty.clone()));
        ctx.update_var_type_and_ownership(
            name,
            updated_ty.clone(),
            crate::codegen::context::HeapOwnership::local_owner_for_type(&updated_ty),
        );
    }
}
