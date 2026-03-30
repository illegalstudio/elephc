use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use super::{emit_expr, restore_concat_offset_after_nested_call, retain_borrowed_heap_arg};
use super::{save_concat_offset_before_nested_call, Expr, PhpType, Visibility};

pub(super) fn emit_new_object(
    class_name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let class_info = match ctx.classes.get(class_name).cloned() {
        Some(c) => c,
        None => {
            emitter.comment(&format!("WARNING: undefined class {}", class_name));
            return PhpType::Int;
        }
    };
    let num_props = class_info.properties.len();
    let obj_size = 8 + num_props * 16; // 8 for class_id + 16 per property

    emitter.comment(&format!("new {}()", class_name));

    // -- allocate object on heap --
    emitter.instruction(&format!("mov x0, #{}", obj_size));                     // object size in bytes
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate object → x0 = pointer
    emitter.instruction("mov x9, #4");                                          // heap kind 4 = object instance
    emitter.instruction("str x9, [x0, #-8]");                                   // store object kind in the uniform heap header
    emitter.instruction(&format!("mov x10, #{}", class_info.class_id));         // load compile-time class id
    emitter.instruction("str x10, [x0]");                                       // store class id at object header
    emitter.instruction("str x0, [sp, #-16]!");                                 // save object pointer on stack

    // -- zero-initialize all property slots --
    for i in 0..num_props {
        let offset = 8 + i * 16;
        emitter.instruction("ldr x9, [sp]");                                    // peek object pointer
        emitter.instruction(&format!("str xzr, [x9, #{}]", offset));            // zero-init property lo
        emitter.instruction(&format!("str xzr, [x9, #{}]", offset + 8));        // zero-init property hi
    }

    // -- set default property values --
    for i in 0..num_props {
        if let Some(default_expr) = &class_info.defaults[i] {
            let default_expr = default_expr.clone();
            let offset = 8 + i * 16;
            let prop_ty = emit_expr(&default_expr, emitter, ctx, data);
            emitter.instruction("ldr x9, [sp]");                                // peek object pointer
            match &prop_ty {
                PhpType::Int
                | PhpType::Bool
                | PhpType::Callable
                | PhpType::Pointer(_) => {
                    emitter.instruction(&format!("str x0, [x9, #{}]", offset)); // store default value
                    emitter.instruction(&format!("str xzr, [x9, #{}]", offset + 8)); // clear runtime property metadata slot
                }
                PhpType::Array(_) => {
                    emitter.instruction(&format!("str x0, [x9, #{}]", offset)); // store default value
                    emitter.instruction("mov x10, #4");                         // runtime property tag 4 = indexed array
                    emitter.instruction(&format!("str x10, [x9, #{}]", offset + 8)); // store runtime property metadata tag
                }
                PhpType::AssocArray { .. } => {
                    emitter.instruction(&format!("str x0, [x9, #{}]", offset)); // store default value
                    emitter.instruction("mov x10, #5");                         // runtime property tag 5 = associative array
                    emitter.instruction(&format!("str x10, [x9, #{}]", offset + 8)); // store runtime property metadata tag
                }
                PhpType::Object(_) => {
                    emitter.instruction(&format!("str x0, [x9, #{}]", offset)); // store default value
                    emitter.instruction("mov x10, #6");                         // runtime property tag 6 = object
                    emitter.instruction(&format!("str x10, [x9, #{}]", offset + 8)); // store runtime property metadata tag
                }
                PhpType::Float => {
                    emitter.instruction(&format!("str d0, [x9, #{}]", offset)); // store float default
                    emitter.instruction(&format!("str xzr, [x9, #{}]", offset + 8)); // clear runtime property metadata slot
                }
                PhpType::Str => {
                    emitter.instruction(&format!("str x1, [x9, #{}]", offset)); // store string pointer
                    emitter.instruction(&format!("str x2, [x9, #{}]", offset + 8)); //store string length
                }
                PhpType::Void => {}
            }
        }
    }

    // -- call __construct if it exists --
    if class_info.methods.contains_key("__construct") {
        let mut arg_types = Vec::new();
        for arg in args {
            let ty = emit_expr(arg, emitter, ctx, data);
            retain_borrowed_heap_arg(emitter, arg, &ty);
            match &ty {
                PhpType::Bool
                | PhpType::Int
                | PhpType::Array(_)
                | PhpType::AssocArray { .. }
                | PhpType::Callable
                | PhpType::Object(_)
                | PhpType::Pointer(_) => {
                    emitter.instruction("str x0, [sp, #-16]!");                 // push int/object arg onto stack
                }
                PhpType::Float => {
                    emitter.instruction("str d0, [sp, #-16]!");                 // push float arg onto stack
                }
                PhpType::Str => {
                    emitter.instruction("stp x1, x2, [sp, #-16]!");             // push string ptr+len onto stack
                }
                PhpType::Void => {}
            }
            arg_types.push(ty);
        }

        let total_args = arg_types.len();
        let mut int_reg_idx = 1usize;
        let mut float_reg_idx = 0usize;
        let mut assignments: Vec<(PhpType, usize, bool)> = Vec::new();
        for ty in &arg_types {
            if ty.is_float_reg() {
                assignments.push((ty.clone(), float_reg_idx, true));
                float_reg_idx += 1;
            } else {
                assignments.push((ty.clone(), int_reg_idx, false));
                int_reg_idx += ty.register_count();
            }
        }

        for i in (0..total_args).rev() {
            let (ty, start_reg, _is_float) = &assignments[i];
            match ty {
                PhpType::Bool
                | PhpType::Int
                | PhpType::Array(_)
                | PhpType::AssocArray { .. }
                | PhpType::Callable
                | PhpType::Object(_)
                | PhpType::Pointer(_) => {
                    emitter.instruction(&format!("ldr x{}, [sp], #16", start_reg)); //pop arg into register
                }
                PhpType::Float => {
                    emitter.instruction(&format!("ldr d{}, [sp], #16", start_reg)); //pop float arg
                }
                PhpType::Str => {
                    emitter.instruction(&format!(
                        "ldp x{}, x{}, [sp], #16",
                        start_reg,
                        start_reg + 1
                    ));
                }
                PhpType::Void => {}
            }
        }

        emitter.instruction("ldr x0, [sp]");                                    // load $this pointer for constructor
        save_concat_offset_before_nested_call(emitter);
        let constructor_impl = class_info
            .method_impl_classes
            .get("__construct")
            .map(String::as_str)
            .unwrap_or(class_name);
        emitter.instruction(&format!("bl _method_{}___construct", constructor_impl)); // call constructor
        restore_concat_offset_after_nested_call(emitter, &PhpType::Void);
    }

    emitter.instruction("ldr x0, [sp], #16");                                   // pop object pointer into x0
    PhpType::Object(class_name.to_string())
}

pub(super) fn emit_property_access(
    object: &Expr,
    property: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let obj_ty = emit_expr(object, emitter, ctx, data);
    let (class_name, prop_ty, offset, needs_deref) = match &obj_ty {
        PhpType::Object(class_name) => {
            let class_info = match ctx.classes.get(class_name).cloned() {
                Some(c) => c,
                None => {
                    emitter.comment(&format!("WARNING: undefined class {}", class_name));
                    return PhpType::Int;
                }
            };

            let prop_ty = match class_info
                .properties
                .iter()
                .find(|(n, _)| n == property)
                .map(|(_, t)| t.clone())
            {
                Some(v) => v,
                None => {
                    emitter.comment(&format!("WARNING: undefined property {}", property));
                    return PhpType::Int;
                }
            };
            let offset = match class_info.property_offsets.get(property) {
                Some(offset) => *offset,
                None => {
                    emitter.comment(&format!("WARNING: missing property offset {}", property));
                    return PhpType::Int;
                }
            };

            (class_name.clone(), prop_ty, offset, false)
        }
        PhpType::Pointer(Some(class_name)) if ctx.extern_classes.contains_key(class_name) => {
            let class_info = match ctx.extern_classes.get(class_name).cloned() {
                Some(c) => c,
                None => {
                    emitter.comment(&format!("WARNING: undefined extern class {}", class_name));
                    return PhpType::Int;
                }
            };

            let field = match class_info
                .fields
                .iter()
                .find(|field| field.name == property)
            {
                Some(field) => field.clone(),
                None => {
                    emitter.comment(&format!("WARNING: undefined extern field {}", property));
                    return PhpType::Int;
                }
            };

            (class_name.clone(), field.php_type, field.offset, true)
        }
        _ => {
            emitter.comment("WARNING: property access on non-object");
            return PhpType::Int;
        }
    };

    if needs_deref {
        emitter.instruction("bl __rt_ptr_check_nonnull");                       // abort with fatal error on null pointer dereference
        emitter.comment(&format!(
            "->{} via ptr<{}> (offset {})",
            property, class_name, offset
        ));
    } else {
        emitter.comment(&format!("->{}  (offset {})", property, offset));
    }

    match &prop_ty {
        PhpType::Str => {
            emitter.instruction(&format!("ldr x1, [x0, #{}]", offset));         // load string pointer from property
            emitter.instruction(&format!("ldr x2, [x0, #{}]", offset + 8));     // load string length from property
        }
        PhpType::Float => {
            emitter.instruction(&format!("ldr d0, [x0, #{}]", offset));         // load float from property
        }
        PhpType::Bool | PhpType::Int | PhpType::Void => {
            emitter.instruction(&format!("ldr x0, [x0, #{}]", offset));         // load int/bool from property
        }
        PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Callable
        | PhpType::Object(_)
        | PhpType::Pointer(_) => {
            emitter.instruction(&format!("ldr x0, [x0, #{}]", offset));         // load heap pointer from property
        }
    }

    prop_ty
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

    let mut arg_types = Vec::new();
    for arg in args {
        let ty = emit_expr(arg, emitter, ctx, data);
        retain_borrowed_heap_arg(emitter, arg, &ty);
        match &ty {
            PhpType::Bool
            | PhpType::Int
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Callable
            | PhpType::Object(_)
            | PhpType::Pointer(_) => {
                emitter.instruction("str x0, [sp, #-16]!");                     // push int/object arg
            }
            PhpType::Float => {
                emitter.instruction("str d0, [sp, #-16]!");                     // push float arg
            }
            PhpType::Str => {
                emitter.instruction("stp x1, x2, [sp, #-16]!");                 // push string ptr+len
            }
            PhpType::Void => {}
        }
        arg_types.push(ty);
    }

    let obj_ty = emit_expr(object, emitter, ctx, data);
    let class_name = match &obj_ty {
        PhpType::Object(cn) => cn.clone(),
        _ => {
            emitter.comment("WARNING: method call on non-object");
            return PhpType::Int;
        }
    };
    emitter.instruction("str x0, [sp, #-16]!");                                 // push $this pointer

    let total_args = arg_types.len();
    let mut int_reg_idx = 1usize;
    let mut float_reg_idx = 0usize;
    let mut assignments: Vec<(PhpType, usize, bool)> = Vec::new();
    for ty in &arg_types {
        if ty.is_float_reg() {
            assignments.push((ty.clone(), float_reg_idx, true));
            float_reg_idx += 1;
        } else {
            assignments.push((ty.clone(), int_reg_idx, false));
            int_reg_idx += ty.register_count();
        }
    }

    emitter.instruction("ldr x0, [sp], #16");                                   // pop $this into x0

    for i in (0..total_args).rev() {
        let (ty, start_reg, _) = &assignments[i];
        match ty {
            PhpType::Bool
            | PhpType::Int
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Callable
            | PhpType::Object(_)
            | PhpType::Pointer(_) => {
                emitter.instruction(&format!("ldr x{}, [sp], #16", start_reg)); // pop arg into register
            }
            PhpType::Float => {
                emitter.instruction(&format!("ldr d{}, [sp], #16", start_reg)); // pop float arg
            }
            PhpType::Str => {
                emitter.instruction(&format!(
                    "ldp x{}, x{}, [sp], #16",
                    start_reg,
                    start_reg + 1
                ));
            }
            PhpType::Void => {}
        }
    }

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
        super::super::abi::load_at_offset(emitter, "x0", var.stack_offset);     // forward hidden called-class id from current static method
        true
    } else if let Some(var) = ctx.variables.get("this") {
        super::super::abi::load_at_offset(emitter, "x0", var.stack_offset);     // load implicit $this pointer
        emitter.instruction("ldr x0, [x0]");                                    // read dynamic class id from object header
        true
    } else {
        false
    }
}

pub(super) fn emit_static_method_call(
    receiver: &crate::parser::ast::StaticReceiver,
    method: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let parent_call = matches!(receiver, crate::parser::ast::StaticReceiver::Parent);
    let self_call = matches!(receiver, crate::parser::ast::StaticReceiver::Self_);
    let static_call = matches!(receiver, crate::parser::ast::StaticReceiver::Static);
    let forwarded_call = matches!(
        receiver,
        crate::parser::ast::StaticReceiver::Parent
            | crate::parser::ast::StaticReceiver::Self_
            | crate::parser::ast::StaticReceiver::Static
    );
    let class_name = match receiver {
        crate::parser::ast::StaticReceiver::Named(class_name) => class_name.clone(),
        crate::parser::ast::StaticReceiver::Self_
        | crate::parser::ast::StaticReceiver::Static => match &ctx.current_class {
            Some(class_name) => class_name.clone(),
            None => {
                emitter.comment("WARNING: self::/static:: used outside class scope");
                return PhpType::Int;
            }
        },
        crate::parser::ast::StaticReceiver::Parent => {
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

    let mut arg_types = Vec::new();
    for arg in args {
        let ty = emit_expr(arg, emitter, ctx, data);
        retain_borrowed_heap_arg(emitter, arg, &ty);
        match &ty {
            PhpType::Bool
            | PhpType::Int
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Callable
            | PhpType::Object(_)
            | PhpType::Pointer(_) => {
                emitter.instruction("str x0, [sp, #-16]!");                     // push arg onto stack
            }
            PhpType::Float => {
                emitter.instruction("str d0, [sp, #-16]!");                     // push float arg
            }
            PhpType::Str => {
                emitter.instruction("stp x1, x2, [sp, #-16]!");                 // push string ptr+len
            }
            PhpType::Void => {}
        }
        arg_types.push(ty);
    }

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

    let total_args = arg_types.len();
    let mut int_reg_idx = 0usize;
    if needs_called_class_id {
        int_reg_idx += 1;
    }
    if needs_this {
        int_reg_idx += 1;
    }
    let mut float_reg_idx = 0usize;
    let mut assignments: Vec<(PhpType, usize, bool)> = Vec::new();
    for ty in &arg_types {
        if ty.is_float_reg() {
            assignments.push((ty.clone(), float_reg_idx, true));
            float_reg_idx += 1;
        } else {
            assignments.push((ty.clone(), int_reg_idx, false));
            int_reg_idx += ty.register_count();
        }
    }

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
        super::super::abi::load_at_offset(emitter, "x0", this_var.stack_offset); // load implicit $this for scoped instance call
        emitter.instruction("str x0, [sp, #-16]!");                             // push implicit receiver
    }

    if needs_called_class_id {
        emitter.instruction("ldr x0, [sp], #16");                               // pop hidden called-class id into x0
    }
    if needs_this {
        let this_reg = if needs_called_class_id { 1 } else { 0 };
        emitter.instruction(&format!("ldr x{}, [sp], #16", this_reg));          // pop implicit $this into its assigned integer register
    }
    for i in (0..total_args).rev() {
        let (ty, start_reg, _) = &assignments[i];
        match ty {
            PhpType::Bool
            | PhpType::Int
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Callable
            | PhpType::Object(_)
            | PhpType::Pointer(_) => {
                emitter.instruction(&format!("ldr x{}, [sp], #16", start_reg)); // pop arg into assigned register
            }
            PhpType::Float => {
                emitter.instruction(&format!("ldr d{}, [sp], #16", start_reg)); // pop float arg
            }
            PhpType::Str => {
                emitter.instruction(&format!(
                    "ldp x{}, x{}, [sp], #16",
                    start_reg,
                    start_reg + 1
                ));
            }
            PhpType::Void => {}
        }
    }

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
