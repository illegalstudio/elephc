use super::late_bound;
use super::resolve;
use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::codegen::stmt::helpers;
use crate::names::static_property_symbol;
use crate::parser::ast::{Expr, ExprKind, StaticReceiver};
use crate::types::PhpType;

mod indexed;

pub(crate) fn emit_static_property_array_push_stmt(
    receiver: &StaticReceiver,
    property: &str,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment(&format!("::${}[] = ...", property));

    let Some((_, declaring_class, prop_ty, _)) =
        resolve::resolve_static_property(receiver, property, ctx, emitter)
    else {
        return;
    };
    let elem_ty = match &prop_ty {
        PhpType::Array(elem_ty) => *elem_ty.clone(),
        _ => {
            emitter.comment("WARNING: static property array push on non-array property");
            return;
        }
    };
    let branches =
        late_bound::dynamic_static_property_branches(receiver, property, &declaring_class, ctx);
    let class_id_saved = late_bound::emit_and_push_called_class_id_if_needed(
        &branches,
        emitter,
        ctx,
    );
    if class_id_saved {
        let class_id_reg = late_bound::class_id_work_reg(emitter);
        abi::emit_pop_reg(emitter, class_id_reg);                              // reload the called class id before selecting the static array slot
        abi::emit_push_reg(emitter, class_id_reg);                             // keep the called class id available for the final static array store
        late_bound::emit_dynamic_load_static_property_reg(
            property,
            class_id_reg,
            &declaring_class,
            &branches,
            abi::int_result_reg(emitter),
            emitter,
            ctx,
        );
    } else {
        let symbol = static_property_symbol(&declaring_class, property);
        abi::emit_load_symbol_to_reg(emitter, abi::int_result_reg(emitter), &symbol, 0);
    }
    emit_ensure_indexed_array_pointer(&elem_ty, emitter, ctx);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                 // preserve the static array pointer while evaluating the appended value

    let mut val_ty = emit_expr(value, emitter, ctx, data);
    let boxed_iterable =
        crate::codegen::emit_box_iterable_value_for_mixed_container(emitter, &mut val_ty);
    if !boxed_iterable
        && matches!(elem_ty, PhpType::Mixed)
        && !matches!(val_ty, PhpType::Mixed | PhpType::Union(_))
    {
        crate::codegen::emit_box_current_value_as_mixed(emitter, &val_ty);
        val_ty = PhpType::Mixed;
    } else if !boxed_iterable {
        helpers::retain_borrowed_heap_result(emitter, value, &val_ty);
    }

    let array_reg = abi::symbol_scratch_reg(emitter);
    abi::emit_pop_reg(emitter, array_reg);                                      // restore the static array pointer after evaluating the appended value
    emit_array_push_runtime_call(array_reg, &val_ty, emitter);
    if class_id_saved {
        let class_id_reg = late_bound::class_id_work_reg(emitter);
        abi::emit_pop_reg(emitter, class_id_reg);                              // restore the called class id for the late-bound static array store
        late_bound::emit_dynamic_store_reg_to_static_property(
            property,
            class_id_reg,
            abi::int_result_reg(emitter),
            &declaring_class,
            &branches,
            emitter,
            ctx,
        );
    } else {
        let symbol = static_property_symbol(&declaring_class, property);
        abi::emit_store_reg_to_symbol(emitter, abi::int_result_reg(emitter), &symbol, 0);
    }
}

pub(crate) fn emit_static_property_array_assign_stmt(
    receiver: &StaticReceiver,
    property: &str,
    index: &Expr,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment(&format!("::${}[...] = ...", property));
    if let Some((current, default)) =
        crate::codegen::stmt::null_coalesce_static_property_array_target(
            receiver, property, index, value,
        )
    {
        if matches!(default.kind, ExprKind::Null) {
            emitter.comment("literal null fallback leaves the static property array slot unchanged");
            return;
        }
        let current_ty = emit_expr(current, emitter, ctx, data);
        if current_ty != PhpType::Void {
            let keep_label = ctx.next_label("nca_keep");
            crate::codegen::stmt::emit_branch_if_result_non_null(
                &current_ty,
                &keep_label,
                emitter,
            );
            emit_static_property_array_assign_stmt(
                receiver, property, index, default, emitter, ctx, data,
            );
            emitter.label(&keep_label);
        } else {
            emit_static_property_array_assign_stmt(
                receiver, property, index, default, emitter, ctx, data,
            );
        }
        return;
    }

    let Some((_, declaring_class, prop_ty, _)) =
        resolve::resolve_static_property(receiver, property, ctx, emitter)
    else {
        return;
    };
    let elem_ty = match &prop_ty {
        PhpType::Array(elem_ty) => *elem_ty.clone(),
        _ => {
            emitter.comment("WARNING: static property array assign on non-array property");
            return;
        }
    };
    let branches =
        late_bound::dynamic_static_property_branches(receiver, property, &declaring_class, ctx);
    let class_id_saved = late_bound::emit_and_push_called_class_id_if_needed(
        &branches,
        emitter,
        ctx,
    );
    if class_id_saved {
        let class_id_reg = late_bound::class_id_work_reg(emitter);
        abi::emit_pop_reg(emitter, class_id_reg);                              // reload the called class id before selecting the static array slot
        abi::emit_push_reg(emitter, class_id_reg);                             // keep the called class id available for later static array stores
        late_bound::emit_dynamic_load_static_property_reg(
            property,
            class_id_reg,
            &declaring_class,
            &branches,
            abi::int_result_reg(emitter),
            emitter,
            ctx,
        );
    } else {
        let symbol = static_property_symbol(&declaring_class, property);
        abi::emit_load_symbol_to_reg(emitter, abi::int_result_reg(emitter), &symbol, 0);
    }
    emit_ensure_indexed_array_pointer(&elem_ty, emitter, ctx);
    indexed::emit_static_indexed_array_assign(
        property,
        &declaring_class,
        &branches,
        class_id_saved,
        &elem_ty,
        index,
        value,
        emitter,
        ctx,
        data,
    );
}

fn emit_array_push_runtime_call(array_reg: &str, val_ty: &PhpType, emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => match val_ty {
            PhpType::Int | PhpType::Bool => {
                emitter.instruction("mov x1, x0");                              // move the appended scalar payload into the runtime value register
                emitter.instruction(&format!("mov x0, {}", array_reg));         // move the static array pointer into the runtime receiver register
                emitter.instruction("bl __rt_array_push_int");                  // append the scalar payload and return the possibly-grown static array
            }
            PhpType::Float => {
                emitter.instruction("fmov x1, d0");                             // move the appended float payload bits into the runtime value register
                emitter.instruction(&format!("mov x0, {}", array_reg));         // move the static array pointer into the runtime receiver register
                emitter.instruction("bl __rt_array_push_int");                  // append the float payload bits and return the possibly-grown static array
            }
            PhpType::Str => {
                emitter.instruction(&format!("mov x0, {}", array_reg));         // move the static array pointer into the runtime receiver register
                emitter.instruction("bl __rt_array_push_str");                  // persist and append the string payload into the static array
            }
            PhpType::Callable => {
                emitter.instruction("mov x1, x0");                              // move the callable pointer bits into the runtime value register
                emitter.instruction(&format!("mov x0, {}", array_reg));         // move the static array pointer into the runtime receiver register
                emitter.instruction("bl __rt_array_push_int");                  // append the callable pointer bits as a scalar slot
            }
            PhpType::Mixed | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
                emitter.instruction("mov x1, x0");                              // move the retained heap payload into the runtime value register
                emitter.instruction(&format!("mov x0, {}", array_reg));         // move the static array pointer into the runtime receiver register
                emitter.instruction("bl __rt_array_push_refcounted");           // append the retained heap payload into the static array
            }
            _ => emitter.comment("WARNING: unsupported static property array push payload"),
        },
        Arch::X86_64 => match val_ty {
            PhpType::Int | PhpType::Bool => {
                emitter.instruction("mov rsi, rax");                            // move the appended scalar payload into the SysV value register
                emitter.instruction(&format!("mov rdi, {}", array_reg));        // move the static array pointer into the SysV receiver register
                abi::emit_call_label(emitter, "__rt_array_push_int");
            }
            PhpType::Float => {
                emitter.instruction("movq rsi, xmm0");                          // move the appended float payload bits into the SysV value register
                emitter.instruction(&format!("mov rdi, {}", array_reg));        // move the static array pointer into the SysV receiver register
                abi::emit_call_label(emitter, "__rt_array_push_int");
            }
            PhpType::Str => {
                emitter.instruction("mov rsi, rax");                            // move the appended string pointer into the SysV payload register
                emitter.instruction(&format!("mov rdi, {}", array_reg));        // move the static array pointer into the SysV receiver register
                abi::emit_call_label(emitter, "__rt_array_push_str");
            }
            PhpType::Callable => {
                emitter.instruction("mov rsi, rax");                            // move the callable pointer bits into the SysV value register
                emitter.instruction(&format!("mov rdi, {}", array_reg));        // move the static array pointer into the SysV receiver register
                abi::emit_call_label(emitter, "__rt_array_push_int");
            }
            PhpType::Mixed | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
                emitter.instruction("mov rsi, rax");                            // move the retained heap payload into the SysV value register
                emitter.instruction(&format!("mov rdi, {}", array_reg));        // move the static array pointer into the SysV receiver register
                abi::emit_call_label(emitter, "__rt_array_push_refcounted");
            }
            _ => emitter.comment("WARNING: unsupported static property array push payload"),
        },
    }
}

fn emit_ensure_indexed_array_pointer(elem_ty: &PhpType, emitter: &mut Emitter, ctx: &mut Context) {
    let ready = ctx.next_label("static_array_ready");
    let elem_size = if matches!(elem_ty.codegen_repr(), PhpType::Str) { 16 } else { 8 };
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // does the static array slot already point at heap storage?
            emitter.instruction(&format!("b.ne {}", ready));                    // reuse existing static array storage when it is already initialized
            emitter.instruction("mov x0, #4");                                  // use a small default capacity for an implicitly-created static array
            emitter.instruction(&format!("mov x1, #{}", elem_size));            // choose the element slot width for the implicit static array
            emitter.instruction("bl __rt_array_new");                           // allocate the implicit indexed array for the static property
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 0");                                  // does the static array slot already point at heap storage?
            emitter.instruction(&format!("jne {}", ready));                     // reuse existing static array storage when it is already initialized
            emitter.instruction("mov rdi, 4");                                  // use a small default capacity for an implicitly-created static array
            emitter.instruction(&format!("mov rsi, {}", elem_size));            // choose the element slot width for the implicit static array
            abi::emit_call_label(emitter, "__rt_array_new");
        }
    }
    emitter.label(&ready);
}
