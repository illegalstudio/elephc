use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::{Expr, StaticReceiver, Visibility};
use crate::types::PhpType;

use super::super::{
    emit_expr, restore_concat_offset_after_nested_call, retain_borrowed_heap_arg,
    save_concat_offset_before_nested_call,
};

/// Evaluate arguments, retain borrowed heap values, and push each onto the stack.
/// Returns the list of types for later register assignment.
fn eval_and_push_args(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Vec<PhpType> {
    let mut arg_types = Vec::new();
    for arg in args {
        let ty = emit_expr(arg, emitter, ctx, data);
        retain_borrowed_heap_arg(emitter, arg, &ty);
        match &ty {
            PhpType::Float => {
                emitter.instruction("str d0, [sp, #-16]!");                     // push float arg
            }
            PhpType::Str => {
                emitter.instruction("stp x1, x2, [sp, #-16]!");                 // push string ptr+len
            }
            PhpType::Void => {}
            _ => {
                emitter.instruction("str x0, [sp, #-16]!");                     // push int/object/pointer arg
            }
        }
        arg_types.push(ty);
    }
    arg_types
}

/// Compute register assignments for the given arg types, starting integer
/// register numbering at `first_int_reg`. Returns (assignments, next_int_reg, next_float_reg).
fn compute_register_assignments(
    arg_types: &[PhpType],
    first_int_reg: usize,
) -> Vec<(PhpType, usize, bool)> {
    let mut int_reg_idx = first_int_reg;
    let mut float_reg_idx = 0usize;
    let mut assignments = Vec::new();
    for ty in arg_types {
        if ty.is_float_reg() {
            assignments.push((ty.clone(), float_reg_idx, true));
            float_reg_idx += 1;
        } else {
            assignments.push((ty.clone(), int_reg_idx, false));
            int_reg_idx += ty.register_count();
        }
    }
    assignments
}

/// Pop arguments from the stack into their assigned registers (in reverse order).
fn pop_args_to_registers(emitter: &mut Emitter, assignments: &[(PhpType, usize, bool)]) {
    for i in (0..assignments.len()).rev() {
        let (ty, start_reg, _) = &assignments[i];
        match ty {
            PhpType::Float => {
                emitter.instruction(&format!("ldr d{}, [sp], #16", start_reg)); // pop float arg
            }
            PhpType::Str => {
                emitter.instruction(&format!(                                   // pop string arg pair
                    "ldp x{}, x{}, [sp], #16",
                    start_reg,
                    start_reg + 1
                ));
            }
            PhpType::Void => {}
            _ => {
                emitter.instruction(&format!("ldr x{}, [sp], #16", start_reg)); // pop arg into register
            }
        }
    }
}

pub(super) fn emit_method_call(
    object: &Expr,
    method: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment(&format!("->{}()", method));

    let arg_types = eval_and_push_args(args, emitter, ctx, data);

    let obj_ty = emit_expr(object, emitter, ctx, data);
    let class_name = match &obj_ty {
        PhpType::Object(cn) => cn.clone(),
        _ => {
            emitter.comment("WARNING: method call on non-object");
            return PhpType::Int;
        }
    };
    emitter.instruction("str x0, [sp, #-16]!");                                 // push $this pointer

    let assignments = compute_register_assignments(&arg_types, 1);
    emitter.instruction("ldr x0, [sp], #16");                                   // pop $this into x0
    pop_args_to_registers(emitter, &assignments);

    let class_info = ctx.classes.get(&class_name).cloned();
    let ret_ty = class_info
        .as_ref()
        .and_then(|ci| {
            let impl_class = ci
                .method_impl_classes
                .get(method)
                .map(String::as_str)
                .unwrap_or(class_name.as_str());
            ctx.classes
                .get(impl_class)
                .and_then(|impl_info| impl_info.methods.get(method))
                .cloned()
        })
        .map(|sig| sig.return_type)
        .unwrap_or(PhpType::Int);
    let slot = class_info
        .as_ref()
        .and_then(|ci| ci.vtable_slots.get(method).copied());
    let direct_private_label = class_info.as_ref().and_then(|ci| {
        if ci.method_visibilities.get(method) == Some(&Visibility::Private) {
            let impl_class = ci
                .method_impl_classes
                .get(method)
                .map(String::as_str)
                .unwrap_or(class_name.as_str());
            Some(format!("_method_{}_{}", impl_class, method))
        } else {
            None
        }
    });

    save_concat_offset_before_nested_call(emitter);
    if let Some(slot) = slot {
        emitter.instruction("ldr x10, [x0]");                                   // load dynamic class id from object header
        emitter.instruction("adrp x11, _class_vtable_ptrs@PAGE");               // load vtable pointer table page
        emitter.instruction("add x11, x11, _class_vtable_ptrs@PAGEOFF");        // add vtable pointer table offset
        emitter.instruction("ldr x11, [x11, x10, lsl #3]");                     // load class-specific vtable pointer
        emitter.instruction(&format!("ldr x11, [x11, #{}]", slot * 8));         // load method entry from vtable slot
        emitter.instruction("blr x11");                                         // call virtual method implementation
    } else if let Some(label) = direct_private_label {
        emitter.instruction(&format!("bl {}", label));                          // call lexically-resolved private method directly
    } else {
        emitter.comment(&format!(
            "WARNING: missing vtable slot for {}::{}",
            class_name, method
        ));
    }
    restore_concat_offset_after_nested_call(emitter, &ret_ty);

    ret_ty
}

pub(super) fn emit_immediate_class_id(emitter: &mut Emitter, class_id: u64) {
    emitter.instruction(&format!("mov x0, #{}", class_id));                     // load compile-time class id for static dispatch
}

pub(super) fn emit_forwarded_called_class_id(emitter: &mut Emitter, ctx: &Context) -> bool {
    if let Some(var) = ctx.variables.get("__elephc_called_class_id") {
        crate::codegen::abi::load_at_offset(emitter, "x0", var.stack_offset);       // forward hidden called-class id from current static method
        true
    } else if let Some(var) = ctx.variables.get("this") {
        crate::codegen::abi::load_at_offset(emitter, "x0", var.stack_offset);       // load implicit $this pointer
        emitter.instruction("ldr x0, [x0]");                                    // read dynamic class id from object header
        true
    } else {
        false
    }
}

pub(super) fn emit_static_method_call(
    receiver: &StaticReceiver,
    method: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let parent_call = matches!(receiver, StaticReceiver::Parent);
    let self_call = matches!(receiver, StaticReceiver::Self_);
    let static_call = matches!(receiver, StaticReceiver::Static);
    let forwarded_call = matches!(
        receiver,
        StaticReceiver::Parent | StaticReceiver::Self_ | StaticReceiver::Static
    );
    let class_name = match receiver {
        StaticReceiver::Named(class_name) => class_name.clone(),
        StaticReceiver::Self_ | StaticReceiver::Static => match &ctx.current_class {
            Some(class_name) => class_name.clone(),
            None => {
                emitter.comment("WARNING: self::/static:: used outside class scope");
                return PhpType::Int;
            }
        },
        StaticReceiver::Parent => {
            let current_class = match &ctx.current_class {
                Some(class_name) => class_name.clone(),
                None => {
                    emitter.comment("WARNING: parent:: used outside class scope");
                    return PhpType::Int;
                }
            };
            match ctx.classes.get(&current_class).and_then(|info| info.parent.clone()) {
                Some(parent_name) => parent_name,
                None => {
                    emitter.comment(&format!("WARNING: class {} has no parent", current_class));
                    return PhpType::Int;
                }
            }
        }
    };
    emitter.comment(&format!("{}::{}()", class_name, method));

    let arg_types = eval_and_push_args(args, emitter, ctx, data);

    let class_info = match ctx.classes.get(&class_name).cloned() {
        Some(class_info) => class_info,
        None => {
            emitter.comment(&format!("WARNING: undefined class {}", class_name));
            return PhpType::Int;
        }
    };
    let static_slot = class_info.static_vtable_slots.get(method).copied();
    let direct_static_private_label = if static_call {
        None
    } else if class_info.static_methods.contains_key(method) && static_slot.is_none() {
        let impl_class = class_info
            .static_method_impl_classes
            .get(method)
            .map(String::as_str)
            .unwrap_or(class_name.as_str());
        Some(format!("_static_{}_{}", impl_class, method))
    } else {
        None
    };

    let (ret_ty, label, needs_this, needs_called_class_id, dynamic_static_dispatch) =
        if class_info.static_methods.contains_key(method) {
            let impl_class = class_info
                .static_method_impl_classes
                .get(method)
                .map(String::as_str)
                .unwrap_or(class_name.as_str());
            (
                ctx.classes
                    .get(impl_class)
                    .and_then(|impl_info| impl_info.static_methods.get(method))
                    .map(|sig| sig.return_type.clone())
                    .unwrap_or(PhpType::Int),
                format!("_static_{}_{}", impl_class, method),
                false,
                true,
                static_call && static_slot.is_some(),
            )
        } else if static_call {
            emitter.comment(&format!(
                "WARNING: undefined static method {}::{}",
                class_name, method
            ));
            return PhpType::Int;
        } else if parent_call || self_call {
            let _sig = match class_info.methods.get(method) {
                Some(sig) => sig,
                None => {
                    emitter.comment(&format!(
                        "WARNING: undefined direct instance method {}::{}",
                        class_name, method
                    ));
                    return PhpType::Int;
                }
            };
            let impl_class = class_info
                .method_impl_classes
                .get(method)
                .map(String::as_str)
                .unwrap_or(class_name.as_str());
            (
                ctx.classes
                    .get(impl_class)
                    .and_then(|impl_info| impl_info.methods.get(method))
                    .map(|sig| sig.return_type.clone())
                    .unwrap_or(PhpType::Int),
                format!("_method_{}_{}", impl_class, method),
                true,
                false,
                false,
            )
        } else {
            emitter.comment(&format!(
                "WARNING: cannot call instance method statically {}::{}",
                class_name, method
            ));
            return PhpType::Int;
        };

    let first_int_reg = (if needs_called_class_id { 1 } else { 0 })
        + (if needs_this { 1 } else { 0 });
    let assignments = compute_register_assignments(&arg_types, first_int_reg);

    if needs_called_class_id {
        if forwarded_call {
            if !emit_forwarded_called_class_id(emitter, ctx) {
                emitter.comment("WARNING: missing forwarded called class id");
                return PhpType::Int;
            }
        } else if let Some(target_info) = ctx.classes.get(&class_name) {
            emit_immediate_class_id(emitter, target_info.class_id);
        } else {
            emitter.comment(&format!("WARNING: undefined class {}", class_name));
            return PhpType::Int;
        }
        emitter.instruction("str x0, [sp, #-16]!");                             // push hidden called-class id
    }

    if needs_this {
        let this_var = match ctx.variables.get("this") {
            Some(var) => var,
            None => {
                emitter.comment("WARNING: direct scoped instance call without $this");
                return PhpType::Int;
            }
        };
        crate::codegen::abi::load_at_offset(emitter, "x0", this_var.stack_offset);  // load implicit $this for scoped instance call
        emitter.instruction("str x0, [sp, #-16]!");                             // push implicit receiver
    }

    if needs_called_class_id {
        emitter.instruction("ldr x0, [sp], #16");                               // pop hidden called-class id into x0
    }
    if needs_this {
        let this_reg = if needs_called_class_id { 1 } else { 0 };
        emitter.instruction(&format!("ldr x{}, [sp], #16", this_reg));          // pop implicit $this into its assigned integer register
    }
    pop_args_to_registers(emitter, &assignments);

    save_concat_offset_before_nested_call(emitter);
    if dynamic_static_dispatch {
        let slot = static_slot.expect("codegen bug: dynamic static dispatch without slot");
        emitter.instruction("mov x10, x0");                                     // preserve forwarded called-class id for static-vtable lookup
        emitter.instruction("adrp x11, _class_static_vtable_ptrs@PAGE");        // load static-vtable pointer table page
        emitter.instruction("add x11, x11, _class_static_vtable_ptrs@PAGEOFF"); // add static-vtable pointer table offset
        emitter.instruction("ldr x11, [x11, x10, lsl #3]");                     // load class-specific static-vtable pointer
        emitter.instruction(&format!("ldr x11, [x11, #{}]", slot * 8));         // load static method entry from static-vtable slot
        emitter.instruction("blr x11");                                         // call late-bound static method implementation
    } else if let Some(label) = direct_static_private_label {
        emitter.instruction(&format!("bl {}", label));                          // call direct private static helper
    } else {
        emitter.instruction(&format!("bl {}", label));                          // call resolved static or parent/self target
    }
    restore_concat_offset_after_nested_call(emitter, &ret_ty);

    ret_ty
}
