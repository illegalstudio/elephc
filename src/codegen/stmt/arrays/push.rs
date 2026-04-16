use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub(super) fn emit_array_push_stmt(
    array: &str,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment(&format!("${}[] = ...", array));
    let var = match ctx.variables.get(array) {
        Some(v) => v,
        None => {
            emitter.comment(&format!("WARNING: undefined variable ${}", array));
            return;
        }
    };
    let offset = var.stack_offset;
    let is_ref = ctx.ref_params.contains(array);
    if emitter.target.arch == Arch::X86_64 {
        emit_array_push_stmt_linux_x86_64(array, value, emitter, ctx, data, offset, is_ref);
        return;
    }

    if is_ref {
        abi::load_at_offset(emitter, "x9", offset);                                 // load ref pointer
        emitter.instruction("ldr x0, [x9]");                                    // dereference to get array heap pointer
    } else {
        abi::load_at_offset(emitter, "x0", offset);                                 // load array heap pointer from stack frame
    }
    emitter.instruction("str x0, [sp, #-16]!");                                 // push array pointer onto stack to preserve it
    let elem_ty = match ctx.variables.get(array) {
        Some(v) => match &v.ty {
            PhpType::Array(t) => *t.clone(),
            _ => PhpType::Int,
        },
        None => PhpType::Int,
    };
    let mut val_ty = emit_expr(value, emitter, ctx, data);
    if matches!(elem_ty, PhpType::Mixed) && !matches!(val_ty, PhpType::Mixed | PhpType::Union(_)) {
        crate::codegen::emit_box_current_value_as_mixed(emitter, &val_ty);
        val_ty = PhpType::Mixed;
    } else {
        super::super::helpers::retain_borrowed_heap_result(emitter, value, &val_ty);
    }
    emitter.instruction("ldr x9, [sp], #16");                                   // pop saved array pointer into x9
    if elem_ty != val_ty {
        let updated_ty = PhpType::Array(Box::new(val_ty.clone()));
        ctx.update_var_type_and_ownership(
            array,
            updated_ty.clone(),
            super::super::helpers::local_slot_ownership_after_store(&updated_ty),
        );
    }
    match &val_ty {
        PhpType::Int | PhpType::Bool => {
            emitter.instruction("mov x1, x0");                                  // move value to x1 (second arg for runtime call)
            emitter.instruction("mov x0, x9");                                  // move array pointer to x0 (first arg)
            emitter.instruction("bl __rt_array_push_int");                      // call runtime: append integer to dynamic array
        }
        PhpType::Float => {
            emitter.instruction("fmov x1, d0");                                 // move float bits to integer register
            emitter.instruction("mov x0, x9");                                  // move array pointer to x0 (first arg)
            emitter.instruction("bl __rt_array_push_int");                      // call runtime: append float bits as 8 bytes
        }
        PhpType::Str => {
            emitter.instruction("mov x0, x9");                                  // move array pointer to x0
            emitter.instruction("bl __rt_array_push_str");                      // call runtime: persist + append string to array
        }
        PhpType::Mixed | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
            emitter.instruction("mov x1, x0");                                  // move nested array/object pointer to x1
            emitter.instruction("mov x0, x9");                                  // move outer array pointer to x0
            emitter.instruction("bl __rt_array_push_refcounted");               // append retained pointer and stamp array metadata
        }
        PhpType::Callable => {
            emitter.instruction("mov x1, x0");                                  // move callable pointer to x1
            emitter.instruction("mov x0, x9");                                  // move outer array pointer to x0
            emitter.instruction("bl __rt_array_push_int");                      // append pointer bits without refcount ownership
        }
        _ => {}
    }
    if is_ref {
        abi::load_at_offset(emitter, "x9", offset);                                 // load ref pointer
        emitter.instruction("str x0, [x9]");                                    // store new array ptr through ref
    } else {
        abi::store_at_offset(emitter, "x0", offset);                                // save possibly-new array pointer
    }
}

fn emit_array_push_stmt_linux_x86_64(
    array: &str,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    offset: usize,
    is_ref: bool,
) {
    if is_ref {
        abi::load_at_offset(emitter, "r11", offset);                              // load the by-reference slot that points at the indexed-array local
        abi::emit_load_from_address(emitter, "rax", "r11", 0);                   // dereference the by-reference slot to get the current indexed-array pointer
    } else {
        abi::load_at_offset(emitter, "rax", offset);                              // load the current indexed-array pointer from the local slot
    }
    abi::emit_push_reg(emitter, "rax");                                           // preserve the indexed-array pointer while evaluating the appended value
    let elem_ty = match ctx.variables.get(array) {
        Some(v) => match &v.ty {
            PhpType::Array(t) => *t.clone(),
            _ => PhpType::Int,
        },
        None => PhpType::Int,
    };
    let mut val_ty = emit_expr(value, emitter, ctx, data);
    if matches!(elem_ty, PhpType::Mixed) && !matches!(val_ty, PhpType::Mixed | PhpType::Union(_)) {
        crate::codegen::emit_box_current_value_as_mixed(emitter, &val_ty);
        val_ty = PhpType::Mixed;
    } else {
        super::super::helpers::retain_borrowed_heap_result(emitter, value, &val_ty);
    }
    abi::emit_pop_reg(emitter, "r11");                                            // restore the indexed-array pointer after evaluating the appended value
    if elem_ty != val_ty {
        let updated_ty = PhpType::Array(Box::new(val_ty.clone()));
        ctx.update_var_type_and_ownership(
            array,
            updated_ty.clone(),
            super::super::helpers::local_slot_ownership_after_store(&updated_ty),
        );
    }
    match &val_ty {
        PhpType::Int | PhpType::Bool => {
            emitter.instruction("mov rsi, rax");                                // place the appended scalar payload in the x86_64 runtime value register
            emitter.instruction("mov rdi, r11");                                // place the indexed-array pointer in the x86_64 runtime receiver register
            abi::emit_call_label(emitter, "__rt_array_push_int");                 // append the scalar payload and return the possibly-grown indexed-array pointer
        }
        PhpType::Float => {
            emitter.instruction("movq rsi, xmm0");                              // move the floating-point payload bits into the scalar append register
            emitter.instruction("mov rdi, r11");                                // place the indexed-array pointer in the x86_64 runtime receiver register
            abi::emit_call_label(emitter, "__rt_array_push_int");                 // append the floating-point payload bits as an 8-byte scalar slot
        }
        PhpType::Str => {
            emitter.instruction("mov rsi, rax");                                // place the appended string pointer in the x86_64 runtime payload register
            emitter.instruction("mov rdi, r11");                                // place the indexed-array pointer in the x86_64 runtime receiver register
            abi::emit_call_label(emitter, "__rt_array_push_str");                 // persist and append the string payload, returning the possibly-grown indexed-array pointer
        }
        PhpType::Callable => {
            emitter.instruction("mov rsi, rax");                                // place the callable pointer bits in the x86_64 scalar append register
            emitter.instruction("mov rdi, r11");                                // place the indexed-array pointer in the x86_64 runtime receiver register
            abi::emit_call_label(emitter, "__rt_array_push_int");                 // append the callable pointer bits as a plain scalar slot
        }
        PhpType::Mixed | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
            emitter.instruction("mov rsi, rax");                                // place the retained refcounted payload pointer in the x86_64 runtime child register
            emitter.instruction("mov rdi, r11");                                // place the indexed-array pointer in the x86_64 runtime receiver register
            abi::emit_call_label(emitter, "__rt_array_push_refcounted");          // append the retained heap payload and stamp the indexed-array value_type metadata
        }
        _ => {}
    }
    if is_ref {
        abi::load_at_offset(emitter, "r11", offset);                              // reload the by-reference slot after the append helper may have reallocated the indexed array
        abi::emit_store_to_address(emitter, "rax", "r11", 0);                   // store the updated indexed-array pointer through the by-reference slot
    } else {
        abi::store_at_offset(emitter, "rax", offset);                              // save the possibly-grown indexed-array pointer back into the local slot
    }
}
