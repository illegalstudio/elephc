use super::super::abi;
use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use super::super::expr::emit_expr;
use super::PhpType;
use crate::parser::ast::{Expr, ExprKind};

pub(super) fn emit_assign_stmt(
    name: &str,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment(&format!("${} = ...", name));
    let mut ty = emit_expr(value, emitter, ctx, data);
    let dest_needs_mixed_box = ctx
        .variables
        .get(name)
        .is_some_and(|var| matches!(var.ty, PhpType::Mixed) && ty != PhpType::Mixed);
    if dest_needs_mixed_box {
        super::super::emit_box_current_value_as_mixed(emitter, &ty);
        ty = PhpType::Mixed;
    }

    if ctx.extern_globals.contains_key(name) {
        super::emit_extern_global_store(emitter, name, &ty);
    } else if ctx.global_vars.contains(name) {
        super::retain_borrowed_heap_result(emitter, value, &ty);
        super::emit_global_store(emitter, ctx, name, &ty);
    } else if ctx.ref_params.contains(name) {
        let var = match ctx.variables.get(name) {
            Some(v) => v,
            None => {
                emitter.comment(&format!("WARNING: undefined variable ${}", name));
                return;
            }
        };
        let offset = var.stack_offset;
        let old_ty = var.ty.clone();
        super::retain_borrowed_heap_result(emitter, value, &ty);
        emitter.comment(&format!("write through ref ${}", name));
        abi::load_at_offset(emitter, "x9", offset);                             // load pointer to referenced variable
        if old_ty.is_refcounted() {
            let needs_save_x0 = !matches!(&ty, PhpType::Str | PhpType::Float);
            if needs_save_x0 {
                emitter.instruction("mov x8, x0");                              // preserve incoming heap value across decref
            }
            emitter.instruction("ldr x0, [x9]");                                // load previous heap pointer from ref target
            abi::emit_decref_if_refcounted(emitter, &old_ty);
            if needs_save_x0 {
                emitter.instruction("mov x0, x8");                              // restore incoming value after decref
            }
        }
        match &ty {
            PhpType::Bool | PhpType::Int => {
                emitter.instruction("str x0, [x9]");                            // store int/bool through reference pointer
            }
            PhpType::Float => {
                emitter.instruction("str d0, [x9]");                            // store float through reference pointer
            }
            PhpType::Str => {
                emitter.instruction("str x9, [sp, #-16]!");                     // save ref pointer (str_persist clobbers x9)
                emitter.instruction("ldr x0, [x9]");                            // load old string ptr from ref target
                emitter.instruction("bl __rt_heap_free_safe");                  // free old string if on heap
                emitter.instruction("bl __rt_str_persist");                     // persist new string to heap
                emitter.instruction("ldr x9, [sp], #16");                       // restore ref pointer
                emitter.instruction("str x1, [x9]");                            // store heap string pointer through ref
                emitter.instruction("str x2, [x9, #8]");                        // store string length through ref
            }
            _ => {
                emitter.instruction("str x0, [x9]");                            // store value through reference pointer
            }
        }
    } else {
        let var = match ctx.variables.get(name) {
            Some(v) => v,
            None => {
                emitter.comment(&format!("WARNING: undefined variable ${}", name));
                return;
            }
        };
        let offset = var.stack_offset;
        let old_ty = var.ty.clone();

        if ctx.static_vars.contains(name) {
            super::retain_borrowed_heap_result(emitter, value, &ty);
            super::emit_static_store(emitter, ctx, name, &ty);
        } else {
            super::retain_borrowed_heap_result(emitter, value, &ty);
            let needs_save_x0 = !matches!(&ty, PhpType::Str | PhpType::Float);
            super::release_owned_slot(emitter, &old_ty, offset, needs_save_x0);
        }

        abi::emit_store(emitter, &ty, offset);
        ctx.update_var_type_and_ownership(
            name,
            ty.clone(),
            super::local_slot_ownership_after_store(&ty),
        );

        if ctx.in_main && ctx.all_global_var_names.contains(name) {
            if ty.is_refcounted() {
                abi::emit_incref_if_refcounted(emitter, &ty);                   // global storage becomes a second owner alongside the local slot
            }
            super::emit_global_store(emitter, ctx, name, &ty);
        }
    }

    if matches!(&value.kind, ExprKind::Closure { .. }) {
        if let Some(deferred) = ctx.deferred_closures.last() {
            ctx.closure_sigs.insert(name.to_string(), deferred.sig.clone());
            if !deferred.captures.is_empty() {
                ctx.closure_captures
                    .insert(name.to_string(), deferred.captures.clone());
            }
        }
    }

    if let Some(var) = ctx.variables.get(name) {
        if var.ty != ty {
            ctx.update_var_type_and_ownership(
                name,
                ty.clone(),
                super::local_slot_ownership_after_store(&ty),
            );
        }
    }
}

pub(super) fn emit_property_assign_stmt(
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
        super::retain_borrowed_heap_result(emitter, value, &val_ty);
    }

    match &val_ty {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Mixed
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Callable
        | PhpType::Object(_)
        | PhpType::Pointer(_) => {
            emitter.instruction("str x0, [sp, #-16]!");                         // save value on stack
        }
        PhpType::Float => {
            emitter.instruction("str d0, [sp, #-16]!");                         // save float value on stack
        }
        PhpType::Str => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // save string ptr+len on stack
        }
        PhpType::Void => {}
    }

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
        _ => {
            emitter.comment("WARNING: property assign on non-object");
            return;
        }
    };

    if needs_deref {
        emitter.instruction("bl __rt_ptr_check_nonnull");                       // abort with fatal error on null pointer dereference
        emitter.comment(&format!(
            "store extern field {}::{} at offset {}",
            class_name, property, offset
        ));
    }

    emitter.instruction("mov x9, x0");                                          // save object pointer in x9
    if !needs_deref {
        if matches!(prop_ty, PhpType::Str) {
            emitter.instruction("str x9, [sp, #-16]!");                         // preserve object pointer across string release call
            emitter.instruction(&format!("ldr x0, [x9, #{}]", offset));         // load previous string pointer from property slot
            emitter.instruction("bl __rt_heap_free_safe");                      // release previous string storage before overwrite
            emitter.instruction("ldr x9, [sp], #16");                           // restore object pointer after string release
        } else if prop_ty.is_refcounted() {
            emitter.instruction("str x9, [sp, #-16]!");                         // preserve object pointer across decref call
            emitter.instruction(&format!("ldr x0, [x9, #{}]", offset));         // load previous heap pointer from property slot
            abi::emit_decref_if_refcounted(emitter, &prop_ty);
            emitter.instruction("ldr x9, [sp], #16");                           // restore object pointer after decref
        }
    }

    match &val_ty {
        PhpType::Bool | PhpType::Int | PhpType::Callable | PhpType::Pointer(_) => {
            emitter.instruction("ldr x10, [sp], #16");                          // pop saved value
            emitter.instruction(&format!("str x10, [x9, #{}]", offset));        // store value into property
            emitter.instruction(&format!("str xzr, [x9, #{}]", offset + 8));    // clear runtime property metadata slot
        }
        PhpType::Mixed => {
            emitter.instruction("ldr x10, [sp], #16");                          // pop saved boxed mixed value
            emitter.instruction(&format!("str x10, [x9, #{}]", offset));        // store boxed mixed pointer into property
            emitter.instruction("mov x10, #7");                                 // runtime property tag 7 = mixed
            emitter.instruction(&format!("str x10, [x9, #{}]", offset + 8));    // store runtime property metadata tag
        }
        PhpType::Array(_) => {
            emitter.instruction("ldr x10, [sp], #16");                          // pop saved value
            emitter.instruction(&format!("str x10, [x9, #{}]", offset));        // store value into property
            emitter.instruction("mov x10, #4");                                 // runtime property tag 4 = indexed array
            emitter.instruction(&format!("str x10, [x9, #{}]", offset + 8));    // store runtime property metadata tag
        }
        PhpType::AssocArray { .. } => {
            emitter.instruction("ldr x10, [sp], #16");                          // pop saved value
            emitter.instruction(&format!("str x10, [x9, #{}]", offset));        // store value into property
            emitter.instruction("mov x10, #5");                                 // runtime property tag 5 = associative array
            emitter.instruction(&format!("str x10, [x9, #{}]", offset + 8));    // store runtime property metadata tag
        }
        PhpType::Object(_) => {
            emitter.instruction("ldr x10, [sp], #16");                          // pop saved value
            emitter.instruction(&format!("str x10, [x9, #{}]", offset));        // store value into property
            emitter.instruction("mov x10, #6");                                 // runtime property tag 6 = object
            emitter.instruction(&format!("str x10, [x9, #{}]", offset + 8));    // store runtime property metadata tag
        }
        PhpType::Float => {
            emitter.instruction("ldr d0, [sp], #16");                           // pop saved float
            emitter.instruction(&format!("str d0, [x9, #{}]", offset));         // store float into property
            emitter.instruction(&format!("str xzr, [x9, #{}]", offset + 8));    // clear runtime property metadata slot
        }
        PhpType::Str => {
            emitter.instruction("ldp x1, x2, [sp], #16");                       // pop saved string ptr+len
            emitter.instruction(&format!("str x1, [x9, #{}]", offset));         // store string pointer into property
            emitter.instruction(&format!("str x2, [x9, #{}]", offset + 8));     // store string length into property
        }
        PhpType::Void => {
            emitter.instruction(&format!("str xzr, [x9, #{}]", offset));        // clear the property payload slot
            emitter.instruction(&format!("str xzr, [x9, #{}]", offset + 8));    // clear runtime property metadata slot
        }
    }
}

fn resolve_magic_set_target(object: &Expr, property: &str, ctx: &Context) -> Option<String> {
    let obj_ty = super::super::functions::infer_contextual_type(object, ctx);
    let PhpType::Object(class_name) = obj_ty else {
        return None;
    };
    let class_info = ctx.classes.get(&class_name)?;
    if class_info.properties.iter().any(|(name, _)| name == property) {
        return None;
    }
    class_info
        .methods
        .contains_key("__set")
        .then_some(class_name)
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
    emitter.instruction("str x0, [sp, #-16]!");                                 // push $this pointer while boxing the value argument

    if *val_ty == PhpType::Void {
        emitter.instruction("mov x0, #8");                                      // runtime tag 8 = null payload for Mixed boxing
        emitter.instruction("mov x1, xzr");                                     // null mixed payloads have no low word
        emitter.instruction("mov x2, xzr");                                     // null mixed payloads have no high word
        emitter.instruction("bl __rt_mixed_from_value");                        // box null into an owned Mixed cell for __set
        emitter.instruction("ldr x10, [sp]");                                   // reload $this after the boxing helper may clobber caller-saved registers
        emitter.instruction("add sp, sp, #16");                                 // drop the temporary $this stack slot
    } else {
        match val_ty {
            PhpType::Float => {
                emitter.instruction("ldr d0, [sp, #16]");                       // reload the saved float value for Mixed boxing
                super::super::emit_box_current_value_as_mixed(emitter, val_ty);
            }
            PhpType::Str => {
                emitter.instruction("ldp x1, x2, [sp, #16]");                   // reload the saved string payload for Mixed boxing
                super::super::emit_box_current_value_as_mixed(emitter, val_ty);
            }
            _ => {
                emitter.instruction("ldr x0, [sp, #16]");                       // reload the saved scalar/heap value for Mixed boxing
                if *val_ty != PhpType::Mixed {
                    super::super::emit_box_current_value_as_mixed(emitter, val_ty);
                }
            }
        }
        emitter.instruction("ldr x10, [sp]");                                   // reload $this after the boxing helper may clobber caller-saved registers
        emitter.instruction("add sp, sp, #32");                                 // drop the temporary $this slot and saved original value
    }

    emitter.instruction("mov x11, x0");                                         // keep the boxed Mixed value across property-name setup
    super::super::expr::push_magic_property_name_arg(property, emitter, data);
    emitter.instruction("str x11, [sp, #-16]!");                                // push the boxed Mixed $value argument
    emitter.instruction("str x10, [sp, #-16]!");                                // push $this pointer for __set dispatch
    super::super::expr::emit_method_call_with_pushed_args(
        class_name,
        "__set",
        &[PhpType::Str, PhpType::Mixed],
        emitter,
        ctx,
    );
}
