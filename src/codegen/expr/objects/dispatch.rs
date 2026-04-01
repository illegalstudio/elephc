use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::functions;
use crate::names::{method_symbol, static_method_symbol};
use crate::parser::ast::{Expr, ExprKind, StaticReceiver, Visibility};
use crate::types::{FunctionSig, PhpType};

use super::super::{
    emit_expr, restore_concat_offset_after_nested_call, retain_borrowed_heap_arg,
    save_concat_offset_before_nested_call,
};

/// Evaluate arguments, retain borrowed heap values, and push each onto the stack.
/// Returns the list of types for later register assignment.
fn eval_and_push_args(
    args: &[Expr],
    sig: Option<&FunctionSig>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Vec<PhpType> {
    let is_variadic = sig.map(|s| s.variadic.is_some()).unwrap_or(false);
    let regular_param_count = sig
        .map(|s| {
            if s.variadic.is_some() {
                s.params.len().saturating_sub(1)
            } else {
                s.params.len()
            }
        })
        .unwrap_or(args.len());
    let mut regular_args: Vec<&Expr> = Vec::new();
    let mut variadic_args: Vec<&Expr> = Vec::new();
    let mut spread_arg: Option<&Expr> = None;
    let mut spread_at_index: usize = 0;
    for (i, arg) in args.iter().enumerate() {
        if let ExprKind::Spread(inner) = &arg.kind {
            spread_arg = Some(inner.as_ref());
            spread_at_index = regular_args.len();
        } else if is_variadic && i >= regular_param_count {
            variadic_args.push(arg);
        } else {
            regular_args.push(arg);
        }
    }
    let spread_into_named = spread_arg.is_some() && !is_variadic;

    let mut all_args: Vec<&Expr> = regular_args;
    let mut default_exprs: Vec<Expr> = Vec::new();
    if !spread_into_named {
        if let Some(sig) = sig {
            for i in all_args.len()..regular_param_count {
                if let Some(Some(default)) = sig.defaults.get(i) {
                    default_exprs.push(default.clone());
                }
            }
        }
        let default_refs: Vec<&Expr> = default_exprs.iter().collect();
        all_args.extend(default_refs);
    }

    let mut arg_types = Vec::new();
    for (i, arg) in all_args.iter().enumerate() {
        let is_ref = sig
            .and_then(|sig| sig.ref_params.get(i))
            .copied()
            .unwrap_or(false);
        if is_ref {
            if let ExprKind::Variable(var_name) = &arg.kind {
                if ctx.global_vars.contains(var_name) {
                    let label = format!("_gvar_{}", var_name);
                    emitter.comment(&format!("method ref arg: address of global ${}", var_name));
                    emitter.instruction(&format!("adrp x0, {}@PAGE", label));   // load page of global var
                    emitter.instruction(&format!("add x0, x0, {}@PAGEOFF", label)); // resolve global var address
                } else if ctx.ref_params.contains(var_name) {
                    let Some(var) = ctx.variables.get(var_name) else {
                        emitter.comment(&format!("WARNING: undefined ref variable ${}", var_name));
                        continue;
                    };
                    emitter.comment(&format!("method ref arg: forward underlying reference for ${}", var_name));
                    crate::codegen::abi::load_at_offset(emitter, "x0", var.stack_offset); // load existing reference pointer
                } else {
                    let Some(var) = ctx.variables.get(var_name) else {
                        emitter.comment(&format!("WARNING: undefined variable ${}", var_name));
                        continue;
                    };
                    emitter.comment(&format!("method ref arg: address of ${}", var_name));
                    emitter.instruction(&format!("sub x0, x29, #{}", var.stack_offset)); // compute address of local variable
                }
            } else {
                let ty = emit_expr(arg, emitter, ctx, data);
                retain_borrowed_heap_arg(emitter, arg, &ty);
            }
            emitter.instruction("str x0, [sp, #-16]!");                         // push address for by-ref argument
            arg_types.push(PhpType::Int);
        } else {
            let ty = emit_expr(arg, emitter, ctx, data);
            retain_borrowed_heap_arg(emitter, arg, &ty);
            match &ty {
                PhpType::Float => {
                    emitter.instruction("str d0, [sp, #-16]!");                 // push float arg
                }
                PhpType::Str => {
                    emitter.instruction("stp x1, x2, [sp, #-16]!");             // push string ptr+len
                }
                PhpType::Void => {}
                _ => {
                    emitter.instruction("str x0, [sp, #-16]!");                 // push int/object/pointer arg
                }
            }
            arg_types.push(ty);
        }
    }

    if spread_into_named {
        if let Some(spread_expr) = spread_arg {
            let remaining = regular_param_count.saturating_sub(spread_at_index);
            emitter.comment(&format!("unpack spread into {} method params", remaining));
            let _ty = emit_expr(spread_expr, emitter, ctx, data);
            let elem_ty = if let Some(sig) = sig {
                if spread_at_index < sig.params.len() {
                    sig.params[spread_at_index].1.clone()
                } else {
                    PhpType::Int
                }
            } else {
                PhpType::Int
            };
            emitter.instruction("mov x9, x0");                                  // save array pointer in x9
            emitter.instruction("add x9, x9, #24");                             // skip 24-byte array header to reach data
            for idx in 0..remaining {
                match &elem_ty {
                    PhpType::Int | PhpType::Bool => {
                        emitter.instruction(&format!("ldr x0, [x9, #{}]", idx * 8)); // load int element from spread array
                        emitter.instruction("str x0, [sp, #-16]!");             // push unpacked int arg onto stack
                    }
                    PhpType::Float => {
                        emitter.instruction(&format!("ldr d0, [x9, #{}]", idx * 8)); // load float element from spread array
                        emitter.instruction("str d0, [sp, #-16]!");             // push unpacked float arg onto stack
                    }
                    PhpType::Str => {
                        emitter.instruction(&format!("ldr x1, [x9, #{}]", idx * 16)); // load spread string pointer
                        emitter.instruction(&format!("ldr x2, [x9, #{}]", idx * 16 + 8)); // load spread string length
                        emitter.instruction("stp x1, x2, [sp, #-16]!");         // push unpacked string arg onto stack
                    }
                    _ => {
                        emitter.instruction(&format!("ldr x0, [x9, #{}]", idx * 8)); // load spread element from array
                        emitter.instruction("str x0, [sp, #-16]!");             // push unpacked arg onto stack
                    }
                }
                arg_types.push(elem_ty.clone());
            }
        }
    }

    if is_variadic {
        if let Some(spread_expr) = spread_arg {
            emitter.comment("spread array as variadic method param");
            let ty = emit_expr(spread_expr, emitter, ctx, data);
            retain_borrowed_heap_arg(emitter, spread_expr, &ty);
            emitter.instruction("str x0, [sp, #-16]!");                         // push variadic array pointer onto stack
            arg_types.push(ty);
        } else if variadic_args.is_empty() {
            emitter.comment("empty variadic method array");
            emitter.instruction("mov x0, #4");                                  // initial capacity: 4 (grows dynamically)
            emitter.instruction("mov x1, #8");                                  // element size: 8 bytes
            emitter.instruction("bl __rt_array_new");                           // allocate empty array for variadic param
            emitter.instruction("str x0, [sp, #-16]!");                         // push empty variadic array onto stack
            arg_types.push(PhpType::Array(Box::new(PhpType::Int)));
        } else {
            let n = variadic_args.len();
            emitter.comment(&format!("build variadic method array ({} elements)", n));
            let first_elem_ty = functions::infer_contextual_type(variadic_args[0], ctx);
            let es: usize = match &first_elem_ty {
                PhpType::Str => 16,
                _ => 8,
            };
            emitter.instruction(&format!("mov x0, #{}", n));                    // capacity: exact element count
            emitter.instruction(&format!("mov x1, #{}", es));                   // element size in bytes
            emitter.instruction("bl __rt_array_new");                           // allocate array for variadic args
            emitter.instruction("str x0, [sp, #-16]!");                         // save variadic array pointer on stack

            for (i, varg) in variadic_args.iter().enumerate() {
                let ty = emit_expr(varg, emitter, ctx, data);
                retain_borrowed_heap_arg(emitter, varg, &ty);
                emitter.instruction("ldr x9, [sp]");                            // peek variadic array pointer from stack
                if i == 0 {
                    super::super::arrays::emit_array_value_type_stamp(emitter, "x9", &ty);
                }
                match &ty {
                    PhpType::Int | PhpType::Bool | PhpType::Callable => {
                        emitter.instruction(&format!("str x0, [x9, #{}]", 24 + i * 8)); // store int-like variadic element
                    }
                    PhpType::Float => {
                        emitter.instruction(&format!("str d0, [x9, #{}]", 24 + i * 8)); // store float variadic element
                    }
                    PhpType::Str => {
                        emitter.instruction(&format!("str x1, [x9, #{}]", 24 + i * 16)); // store variadic string pointer
                        emitter.instruction(&format!("str x2, [x9, #{}]", 24 + i * 16 + 8)); // store variadic string length
                    }
                    PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
                        emitter.instruction(&format!("str x0, [x9, #{}]", 24 + i * 8)); // store refcounted variadic payload
                    }
                    _ => {}
                }
                emitter.instruction(&format!("mov x10, #{}", i + 1));           // new variadic array length after this element
                emitter.instruction("str x10, [x9]");                           // persist updated variadic array length
            }

            arg_types.push(PhpType::Array(Box::new(first_elem_ty)));
        }
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

fn resolve_instance_method_dispatch(
    ctx: &Context,
    class_name: &str,
    method: &str,
) -> (PhpType, Option<usize>, Option<String>) {
    let class_info = ctx.classes.get(class_name).cloned();
    let ret_ty = class_info
        .as_ref()
        .and_then(|ci| {
            let impl_class = ci
                .method_impl_classes
                .get(method)
                .map(String::as_str)
                .unwrap_or(class_name);
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
                .unwrap_or(class_name);
            Some(method_symbol(impl_class, method))
        } else {
            None
        }
    });
    (ret_ty, slot, direct_private_label)
}

pub(super) fn emit_dispatch_instance_method(
    class_name: &str,
    method: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    let (ret_ty, slot, direct_private_label) =
        resolve_instance_method_dispatch(ctx, class_name, method);

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

pub(super) fn emit_method_call_with_pushed_args(
    class_name: &str,
    method: &str,
    arg_types: &[PhpType],
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    let assignments = compute_register_assignments(arg_types, 1);
    emitter.instruction("ldr x0, [sp], #16");                                   // pop $this into x0
    pop_args_to_registers(emitter, &assignments);
    emit_dispatch_instance_method(class_name, method, emitter, ctx)
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

    let obj_ty = functions::infer_contextual_type(object, ctx);
    let class_name = match &obj_ty {
        PhpType::Object(cn) => cn.clone(),
        _ => {
            emitter.comment("WARNING: method call on non-object");
            return PhpType::Int;
        }
    };
    let sig = ctx
        .classes
        .get(&class_name)
        .and_then(|class_info| class_info.methods.get(method))
        .cloned();
    let arg_types = eval_and_push_args(args, sig.as_ref(), emitter, ctx, data);

    let obj_ty = emit_expr(object, emitter, ctx, data);
    let class_name = match &obj_ty {
        PhpType::Object(cn) => cn.clone(),
        _ => {
            emitter.comment("WARNING: method call on non-object");
            return PhpType::Int;
        }
    };
    emitter.instruction("str x0, [sp, #-16]!");                                 // push $this pointer

    emit_method_call_with_pushed_args(&class_name, method, &arg_types, emitter, ctx)
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
        StaticReceiver::Named(class_name) => class_name.as_str().to_string(),
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

    let class_info = match ctx.classes.get(&class_name).cloned() {
        Some(class_info) => class_info,
        None => {
            emitter.comment(&format!("WARNING: undefined class {}", class_name));
            return PhpType::Int;
        }
    };
    let sig = if class_info.static_methods.contains_key(method) {
        class_info.static_methods.get(method)
    } else if parent_call || self_call {
        class_info.methods.get(method)
    } else {
        None
    }
    .cloned();
    let arg_types = eval_and_push_args(args, sig.as_ref(), emitter, ctx, data);
    let static_slot = class_info.static_vtable_slots.get(method).copied();
    let direct_static_private_label = if static_call {
        None
    } else if class_info.static_methods.contains_key(method) && static_slot.is_none() {
        let impl_class = class_info
            .static_method_impl_classes
            .get(method)
            .map(String::as_str)
            .unwrap_or(class_name.as_str());
        Some(static_method_symbol(impl_class, method))
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
                static_method_symbol(impl_class, method),
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
                method_symbol(impl_class, method),
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
