use super::super::super::abi;
use super::super::super::context::Context;
use super::super::super::data_section::DataSection;
use super::super::super::emit::Emitter;
use super::super::super::expr::emit_expr;
use super::super::PhpType;
use crate::parser::ast::{Expr, ExprKind};

pub(crate) fn emit_assign_stmt(
    name: &str,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment(&format!("${} = ...", name));
    let mut ty = emit_expr(value, emitter, ctx, data);
    let dest_needs_mixed_box = ctx.variables.get(name).is_some_and(|var| {
        !ctx.ref_params.contains(name)
            && matches!(var.ty, PhpType::Mixed)
            && !matches!(ty, PhpType::Mixed | PhpType::Union(_))
    });
    if dest_needs_mixed_box {
        super::super::super::emit_box_current_value_as_mixed(emitter, &ty);
        ty = PhpType::Mixed;
    }

    if ctx.extern_globals.contains_key(name) {
        super::super::emit_extern_global_store(emitter, name, &ty);
    } else if ctx.global_vars.contains(name) {
        if !dest_needs_mixed_box {
            super::super::helpers::retain_borrowed_heap_result(emitter, value, &ty);
        }
        super::super::emit_global_store(emitter, ctx, name, &ty);
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
        let ref_needs_mixed_box =
            matches!(old_ty, PhpType::Mixed) && !matches!(ty, PhpType::Mixed | PhpType::Union(_));
        if ref_needs_mixed_box {
            super::super::super::emit_box_current_value_as_mixed(emitter, &ty);
            ty = PhpType::Mixed;
        } else {
            super::super::helpers::retain_borrowed_heap_result(emitter, value, &ty);
        }
        emitter.comment(&format!("write through ref ${}", name));
        abi::load_at_offset(emitter, "x9", offset);                                  // load pointer to referenced variable
        if old_ty.is_refcounted() {
            emitter.instruction("str x9, [sp, #-16]!");                              // preserve the referenced-slot address across decref helper calls
            let needs_save_x0 = !matches!(&ty, PhpType::Str | PhpType::Float);
            if needs_save_x0 {
                emitter.instruction("mov x8, x0");                                   // preserve incoming heap value across decref
            }
            emitter.instruction("ldr x0, [x9]");                                     // load previous heap pointer from ref target
            abi::emit_decref_if_refcounted(emitter, &old_ty);
            emitter.instruction("ldr x9, [sp], #16");                                // restore the referenced-slot address after decref helper calls
            if needs_save_x0 {
                emitter.instruction("mov x0, x8");                                   // restore incoming value after decref
            }
        }
        match &ty {
            PhpType::Bool | PhpType::Int => {
                emitter.instruction("str x0, [x9]");                                 // store int/bool through reference pointer
            }
            PhpType::Float => {
                emitter.instruction("str d0, [x9]");                                 // store float through reference pointer
            }
            PhpType::Str => {
                emitter.instruction("str x9, [sp, #-16]!");                          // save ref pointer (str_persist clobbers x9)
                emitter.instruction("ldr x0, [x9]");                                 // load old string ptr from ref target
                emitter.instruction("bl __rt_heap_free_safe");                       // free old string if on heap
                emitter.instruction("bl __rt_str_persist");                          // persist new string to heap
                emitter.instruction("ldr x9, [sp], #16");                            // restore ref pointer
                emitter.instruction("str x1, [x9]");                                 // store heap string pointer through ref
                emitter.instruction("str x2, [x9, #8]");                             // store string length through ref
            }
            _ => {
                emitter.instruction("str x0, [x9]");                                 // store value through reference pointer
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
            if !dest_needs_mixed_box {
                super::super::helpers::retain_borrowed_heap_result(emitter, value, &ty);
            }
            super::super::emit_static_store(emitter, ctx, name, &ty);
        } else {
            if !dest_needs_mixed_box {
                super::super::helpers::retain_borrowed_heap_result(emitter, value, &ty);
            }
            let needs_save_x0 = !matches!(&ty, PhpType::Str | PhpType::Float);
            super::super::helpers::release_owned_slot(emitter, &old_ty, offset, needs_save_x0);
        }

        abi::emit_store(emitter, &ty, offset);
        ctx.update_var_type_and_ownership(
            name,
            ty.clone(),
            super::super::helpers::local_slot_ownership_after_store(&ty),
        );

        if ctx.in_main && ctx.all_global_var_names.contains(name) {
            if ty.is_refcounted() {
                abi::emit_incref_if_refcounted(emitter, &ty);                        // global storage becomes a second owner alongside the local slot
            }
            super::super::emit_global_store(emitter, ctx, name, &ty);
        }
    }

    match &value.kind {
        ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_) => {
            if let Some(deferred) = ctx.deferred_closures.last() {
                ctx.closure_sigs.insert(name.to_string(), deferred.sig.clone());
                if !deferred.captures.is_empty() {
                    ctx.closure_captures
                        .insert(name.to_string(), deferred.captures.clone());
                } else {
                    ctx.closure_captures.remove(name);
                }
            }
        }
        ExprKind::Variable(src_name) if ty == PhpType::Callable => {
            if let Some(sig) = ctx.closure_sigs.get(src_name).cloned() {
                ctx.closure_sigs.insert(name.to_string(), sig);
            } else {
                ctx.closure_sigs.remove(name);
            }
            if let Some(captures) = ctx.closure_captures.get(src_name).cloned() {
                ctx.closure_captures.insert(name.to_string(), captures);
            } else {
                ctx.closure_captures.remove(name);
            }
        }
        _ => {
            ctx.closure_sigs.remove(name);
            ctx.closure_captures.remove(name);
        }
    }

    if let Some(var) = ctx.variables.get(name) {
        if var.ty != ty {
            ctx.update_var_type_and_ownership(
                name,
                ty.clone(),
                super::super::helpers::local_slot_ownership_after_store(&ty),
            );
        }
    }
}
