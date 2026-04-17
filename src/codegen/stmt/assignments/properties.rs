mod magic_set;
mod storage;
mod target;

use super::super::super::abi;
use super::super::super::context::Context;
use super::super::super::data_section::DataSection;
use super::super::super::emit::Emitter;
use super::super::super::expr::emit_expr;
use crate::parser::ast::Expr;

pub(crate) fn emit_property_assign_stmt(
    object: &Expr,
    property: &str,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment(&format!("->{}  = ...", property));

    let magic_set_class = magic_set::resolve_magic_set_target(object, property, ctx);
    let val_ty = emit_expr(value, emitter, ctx, data);
    if magic_set_class.is_none() {
        super::super::helpers::retain_borrowed_heap_result(emitter, value, &val_ty);
    }
    abi::emit_push_result_value(emitter, &val_ty);

    let obj_ty = emit_expr(object, emitter, ctx, data);
    let target = match target::resolve_property_assign_target(
        &obj_ty,
        property,
        magic_set_class.as_deref(),
        emitter,
        ctx,
    ) {
        target::PropertyAssignResolution::Resolved(target) => target,
        target::PropertyAssignResolution::UseMagicSet(class_name) => {
            magic_set::emit_magic_set_call(&class_name, property, &val_ty, emitter, ctx, data);
            return;
        }
        target::PropertyAssignResolution::Abort => return,
    };

    if target.needs_deref {
        abi::emit_call_label(emitter, "__rt_ptr_check_nonnull");
        emitter.comment(&format!(
            "store extern field {}::{} at offset {}",
            target.class_name, property, target.offset
        ));
    }

    let object_reg = abi::symbol_scratch_reg(emitter);
    emitter.instruction(&format!("mov {}, {}", object_reg, abi::int_result_reg(emitter))); // keep the object pointer in a scratch register while property storage is updated
    if !target.needs_deref {
        storage::release_previous_property_value(emitter, object_reg, &target.prop_ty, target.offset);
    }

    storage::store_property_value(emitter, object_reg, &val_ty, target.offset);
}

pub(crate) fn emit_property_array_push_stmt(
    object: &Expr,
    property: &str,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment(&format!("->{}[] = ...", property));

    let obj_ty = emit_expr(object, emitter, ctx, data);
    let target = match target::resolve_property_assign_target(&obj_ty, property, None, emitter, ctx) {
        target::PropertyAssignResolution::Resolved(target) => target,
        target::PropertyAssignResolution::UseMagicSet(_) | target::PropertyAssignResolution::Abort => {
            emitter.comment("WARNING: property array push requires a concrete array property");
            return;
        }
    };
    let elem_ty = match &target.prop_ty {
        crate::types::PhpType::Array(elem_ty) => *elem_ty.clone(),
        _ => {
            emitter.comment("WARNING: property array push on non-array property");
            return;
        }
    };

    if target.needs_deref {
        abi::emit_call_label(emitter, "__rt_ptr_check_nonnull");
        emitter.comment(&format!(
            "append to extern field {}::{} at offset {}",
            target.class_name, property, target.offset
        ));
    }

    let object_reg = abi::symbol_scratch_reg(emitter);
    emitter.instruction(&format!("mov {}, {}", object_reg, abi::int_result_reg(emitter))); // preserve the owning object pointer while the append helper evaluates the value and may reallocate the array
    abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), object_reg, target.offset);
    abi::emit_push_reg(emitter, object_reg);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));

    let mut val_ty = emit_expr(value, emitter, ctx, data);
    if matches!(elem_ty, crate::types::PhpType::Mixed)
        && !matches!(val_ty, crate::types::PhpType::Mixed | crate::types::PhpType::Union(_))
    {
        crate::codegen::emit_box_current_value_as_mixed(emitter, &val_ty);
        val_ty = crate::types::PhpType::Mixed;
    } else {
        super::super::helpers::retain_borrowed_heap_result(emitter, value, &val_ty);
    }

    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            abi::emit_pop_reg(emitter, "x9");
            match &val_ty {
                crate::types::PhpType::Int | crate::types::PhpType::Bool => {
                    emitter.instruction("mov x1, x0");                                  // move the appended scalar payload into the runtime helper value register
                    emitter.instruction("mov x0, x9");                                  // move the current array pointer into the runtime helper receiver register
                    emitter.instruction("bl __rt_array_push_int");                      // append the scalar payload and return the possibly-grown array pointer
                }
                crate::types::PhpType::Float => {
                    emitter.instruction("fmov x1, d0");                                 // move the appended float payload bits into the runtime helper value register
                    emitter.instruction("mov x0, x9");                                  // move the current array pointer into the runtime helper receiver register
                    emitter.instruction("bl __rt_array_push_int");                      // append the float payload bits as an 8-byte scalar slot
                }
                crate::types::PhpType::Str => {
                    emitter.instruction("mov x0, x9");                                  // move the current array pointer into the runtime helper receiver register
                    emitter.instruction("bl __rt_array_push_str");                      // persist and append the string payload, returning the possibly-grown array pointer
                }
                crate::types::PhpType::Callable => {
                    emitter.instruction("mov x1, x0");                                  // move the callable pointer bits into the runtime helper value register
                    emitter.instruction("mov x0, x9");                                  // move the current array pointer into the runtime helper receiver register
                    emitter.instruction("bl __rt_array_push_int");                      // append the callable pointer bits as a plain scalar slot
                }
                crate::types::PhpType::Mixed
                | crate::types::PhpType::Array(_)
                | crate::types::PhpType::AssocArray { .. }
                | crate::types::PhpType::Object(_) => {
                    emitter.instruction("mov x1, x0");                                  // move the retained heap payload pointer into the runtime helper child register
                    emitter.instruction("mov x0, x9");                                  // move the current array pointer into the runtime helper receiver register
                    emitter.instruction("bl __rt_array_push_refcounted");               // append the retained heap payload and return the possibly-grown array pointer
                }
                _ => {
                    emitter.comment("WARNING: unsupported property array push payload");
                    abi::emit_pop_reg(emitter, "x10");
                    return;
                }
            }
            abi::emit_pop_reg(emitter, "x10");
            abi::emit_store_to_address(emitter, "x0", "x10", target.offset);
        }
        crate::codegen::platform::Arch::X86_64 => {
            abi::emit_pop_reg(emitter, "r11");
            match &val_ty {
                crate::types::PhpType::Int | crate::types::PhpType::Bool => {
                    emitter.instruction("mov rsi, rax");                                // move the appended scalar payload into the SysV runtime helper value register
                    emitter.instruction("mov rdi, r11");                                // move the current array pointer into the SysV runtime helper receiver register
                    abi::emit_call_label(emitter, "__rt_array_push_int");
                }
                crate::types::PhpType::Float => {
                    emitter.instruction("movq rsi, xmm0");                              // move the appended float payload bits into the SysV runtime helper value register
                    emitter.instruction("mov rdi, r11");                                // move the current array pointer into the SysV runtime helper receiver register
                    abi::emit_call_label(emitter, "__rt_array_push_int");
                }
                crate::types::PhpType::Str => {
                    emitter.instruction("mov rsi, rax");                                // move the appended string pointer into the SysV runtime helper payload register
                    emitter.instruction("mov rdi, r11");                                // move the current array pointer into the SysV runtime helper receiver register
                    abi::emit_call_label(emitter, "__rt_array_push_str");
                }
                crate::types::PhpType::Callable => {
                    emitter.instruction("mov rsi, rax");                                // move the callable pointer bits into the SysV runtime helper value register
                    emitter.instruction("mov rdi, r11");                                // move the current array pointer into the SysV runtime helper receiver register
                    abi::emit_call_label(emitter, "__rt_array_push_int");
                }
                crate::types::PhpType::Mixed
                | crate::types::PhpType::Array(_)
                | crate::types::PhpType::AssocArray { .. }
                | crate::types::PhpType::Object(_) => {
                    emitter.instruction("mov rsi, rax");                                // move the retained heap payload pointer into the SysV runtime helper child register
                    emitter.instruction("mov rdi, r11");                                // move the current array pointer into the SysV runtime helper receiver register
                    abi::emit_call_label(emitter, "__rt_array_push_refcounted");
                }
                _ => {
                    emitter.comment("WARNING: unsupported property array push payload");
                    abi::emit_pop_reg(emitter, "r10");
                    return;
                }
            }
            abi::emit_pop_reg(emitter, "r10");
            abi::emit_store_to_address(emitter, "rax", "r10", target.offset);
        }
    }
}
