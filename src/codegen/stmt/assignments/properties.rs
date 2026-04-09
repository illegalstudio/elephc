use super::super::super::abi;
use super::super::super::context::Context;
use super::super::super::data_section::DataSection;
use super::super::super::emit::Emitter;
use super::super::super::expr::emit_expr;
use super::super::PhpType;
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

    let magic_set_class = resolve_magic_set_target(object, property, ctx);
    let val_ty = emit_expr(value, emitter, ctx, data);
    if magic_set_class.is_none() {
        super::super::helpers::retain_borrowed_heap_result(emitter, value, &val_ty);
    }
    abi::emit_push_result_value(emitter, &val_ty);

    let obj_ty = emit_expr(object, emitter, ctx, data);
    let (class_name, offset, prop_ty, needs_deref) = match &obj_ty {
        PhpType::Object(class_name) => {
            let class_info = match ctx.classes.get(class_name).cloned() {
                Some(c) => c,
                None => {
                    emitter.comment(&format!("WARNING: undefined class {}", class_name));
                    return;
                }
            };
            let prop_ty = match class_info.properties.iter().find(|(n, _)| n == property) {
                Some((_, ty)) => ty.clone(),
                None => {
                    if let Some(magic_class_name) = magic_set_class.as_deref() {
                        emit_magic_set_call(
                            magic_class_name,
                            property,
                            &val_ty,
                            emitter,
                            ctx,
                            data,
                        );
                        return;
                    }
                    emitter.comment(&format!("WARNING: undefined property {}", property));
                    return;
                }
            };
            let offset = match class_info.property_offsets.get(property) {
                Some(offset) => *offset,
                None => {
                    emitter.comment(&format!("WARNING: missing property offset {}", property));
                    return;
                }
            };
            (class_name.clone(), offset, prop_ty, false)
        }
        PhpType::Pointer(Some(class_name)) if ctx.extern_classes.contains_key(class_name) => {
            let class_info = match ctx.extern_classes.get(class_name).cloned() {
                Some(c) => c,
                None => {
                    emitter.comment(&format!("WARNING: undefined extern class {}", class_name));
                    return;
                }
            };
            let field = match class_info.fields.iter().find(|field| field.name == property) {
                Some(field) => field.clone(),
                None => {
                    emitter.comment(&format!("WARNING: undefined extern field {}", property));
                    return;
                }
            };
            (class_name.clone(), field.offset, field.php_type, true)
        }
        PhpType::Pointer(Some(class_name)) if ctx.packed_classes.contains_key(class_name) => {
            let class_info = match ctx.packed_classes.get(class_name).cloned() {
                Some(c) => c,
                None => {
                    emitter.comment(&format!("WARNING: undefined packed class {}", class_name));
                    return;
                }
            };
            let field = match class_info.fields.iter().find(|field| field.name == property) {
                Some(field) => field.clone(),
                None => {
                    emitter.comment(&format!("WARNING: undefined packed field {}", property));
                    return;
                }
            };
            (class_name.clone(), field.offset, field.php_type, true)
        }
        _ => {
            emitter.comment("WARNING: property assign on non-object");
            return;
        }
    };

    if needs_deref {
        abi::emit_call_label(emitter, "__rt_ptr_check_nonnull");
        emitter.comment(&format!(
            "store extern field {}::{} at offset {}",
            class_name, property, offset
        ));
    }

    let object_reg = abi::symbol_scratch_reg(emitter);
    emitter.instruction(&format!("mov {}, {}", object_reg, abi::int_result_reg(emitter))); // keep the object pointer in a scratch register while property storage is updated
    if !needs_deref {
        release_previous_property_value(emitter, object_reg, &prop_ty, offset);
    }

    store_property_value(emitter, object_reg, &val_ty, offset);
}

fn resolve_magic_set_target(object: &Expr, property: &str, ctx: &Context) -> Option<String> {
    let obj_ty = super::super::super::functions::infer_contextual_type(object, ctx);
    let PhpType::Object(class_name) = obj_ty else {
        return None;
    };
    let class_info = ctx.classes.get(&class_name)?;
    if class_info.properties.iter().any(|(name, _)| name == property) {
        return None;
    }
    class_info.methods.contains_key("__set").then_some(class_name)
}

fn emit_magic_set_call(
    class_name: &str,
    property: &str,
    val_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.comment(&format!("magic __set('{}')", property));
    emitter.instruction("str x0, [sp, #-16]!");                                      // push $this pointer while boxing the value argument

    if *val_ty == PhpType::Void {
        emitter.instruction("mov x0, #8");                                           // runtime tag 8 = null payload for Mixed boxing
        emitter.instruction("mov x1, xzr");                                          // null mixed payloads have no low word
        emitter.instruction("mov x2, xzr");                                          // null mixed payloads have no high word
        emitter.instruction("bl __rt_mixed_from_value");                             // box null into an owned Mixed cell for __set
        emitter.instruction("ldr x10, [sp]");                                        // reload $this after the boxing helper may clobber caller-saved registers
        emitter.instruction("add sp, sp, #16");                                      // drop the temporary $this stack slot
    } else {
        match val_ty {
            PhpType::Float => {
                emitter.instruction("ldr d0, [sp, #16]");                            // reload the saved float value for Mixed boxing
                super::super::super::emit_box_current_value_as_mixed(emitter, val_ty);
            }
            PhpType::Str => {
                emitter.instruction("ldp x1, x2, [sp, #16]");                        // reload the saved string payload for Mixed boxing
                super::super::super::emit_box_current_value_as_mixed(emitter, val_ty);
            }
            _ => {
                emitter.instruction("ldr x0, [sp, #16]");                            // reload the saved scalar/heap value for Mixed boxing
                if !matches!(val_ty, PhpType::Mixed | PhpType::Union(_)) {
                    super::super::super::emit_box_current_value_as_mixed(emitter, val_ty);
                }
            }
        }
        emitter.instruction("ldr x10, [sp]");                                        // reload $this after the boxing helper may clobber caller-saved registers
        emitter.instruction("add sp, sp, #32");                                      // drop the temporary $this slot and saved original value
    }

    emitter.instruction("mov x11, x0");                                              // keep the boxed Mixed value across property-name setup
    super::super::super::expr::push_magic_property_name_arg(property, emitter, data);
    emitter.instruction("str x11, [sp, #-16]!");                                     // push the boxed Mixed $value argument
    emitter.instruction("str x10, [sp, #-16]!");                                     // push $this pointer for __set dispatch
    super::super::super::expr::emit_method_call_with_pushed_args(
        class_name,
        "__set",
        &[PhpType::Str, PhpType::Mixed],
        emitter,
        ctx,
    );
}

fn release_previous_property_value(
    emitter: &mut Emitter,
    object_reg: &str,
    prop_ty: &PhpType,
    offset: usize,
) {
    if matches!(prop_ty, PhpType::Str) {
        abi::emit_push_reg(emitter, object_reg);
        abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), object_reg, offset);
        abi::emit_call_label(emitter, "__rt_heap_free_safe");
        abi::emit_pop_reg(emitter, object_reg);
    } else if prop_ty.is_refcounted() {
        abi::emit_push_reg(emitter, object_reg);
        abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), object_reg, offset);
        abi::emit_decref_if_refcounted(emitter, prop_ty);
        abi::emit_pop_reg(emitter, object_reg);
    }
}

fn store_property_value(emitter: &mut Emitter, object_reg: &str, val_ty: &PhpType, offset: usize) {
    let temp_reg = abi::temp_int_reg(emitter.target);
    match val_ty {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Callable
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Packed(_) => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, object_reg, offset);
            abi::emit_store_zero_to_address(emitter, object_reg, offset + 8);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, object_reg, offset);
            abi::emit_load_int_immediate(emitter, temp_reg, 7);
            abi::emit_store_to_address(emitter, temp_reg, object_reg, offset + 8);
        }
        PhpType::Array(_) => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, object_reg, offset);
            abi::emit_load_int_immediate(emitter, temp_reg, 4);
            abi::emit_store_to_address(emitter, temp_reg, object_reg, offset + 8);
        }
        PhpType::AssocArray { .. } => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, object_reg, offset);
            abi::emit_load_int_immediate(emitter, temp_reg, 5);
            abi::emit_store_to_address(emitter, temp_reg, object_reg, offset + 8);
        }
        PhpType::Object(_) => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, object_reg, offset);
            abi::emit_load_int_immediate(emitter, temp_reg, 6);
            abi::emit_store_to_address(emitter, temp_reg, object_reg, offset + 8);
        }
        PhpType::Float => {
            abi::emit_pop_float_reg(emitter, abi::float_result_reg(emitter));
            abi::emit_store_to_address(emitter, abi::float_result_reg(emitter), object_reg, offset);
            abi::emit_store_zero_to_address(emitter, object_reg, offset + 8);
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_pop_reg_pair(emitter, ptr_reg, len_reg);
            abi::emit_push_reg(emitter, object_reg);
            abi::emit_call_label(emitter, "__rt_str_persist");
            abi::emit_pop_reg(emitter, object_reg);
            abi::emit_store_to_address(emitter, ptr_reg, object_reg, offset);
            abi::emit_store_to_address(emitter, len_reg, object_reg, offset + 8);
        }
        PhpType::Void => {
            abi::emit_store_zero_to_address(emitter, object_reg, offset);
            abi::emit_store_zero_to_address(emitter, object_reg, offset + 8);
        }
    }
}
