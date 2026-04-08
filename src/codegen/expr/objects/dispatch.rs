use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::functions;
use crate::names::{enum_case_symbol, method_symbol, static_method_symbol};
use crate::parser::ast::{Expr, StaticReceiver, Visibility};
use crate::types::{EnumCaseValue, FunctionSig, PhpType};

use super::super::{
    emit_expr, restore_concat_offset_after_nested_call, save_concat_offset_before_nested_call,
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
    let prepared = super::super::calls::args::prepare_call_args(
        sig,
        args,
        super::super::calls::args::regular_param_count(sig, args.len()),
    );
    let mut arg_types = super::super::calls::args::emit_pushed_non_variadic_args(
        &prepared.all_args,
        sig,
        "method ref arg",
        true,
        emitter,
        ctx,
        data,
    );

    if prepared.spread_into_named {
        if let Some(spread_expr) = prepared.spread_arg.as_ref() {
            super::super::calls::args::emit_spread_into_named_params(
                spread_expr,
                sig,
                prepared.spread_at_index,
                prepared.regular_param_count,
                "method params",
                emitter,
                ctx,
                data,
                &mut arg_types,
            );
        }
    }

    if prepared.is_variadic {
        if let Some(spread_expr) = prepared.spread_arg.as_ref() {
            let ty = super::super::calls::args::emit_spread_variadic_array_arg(
                spread_expr,
                "spread array as variadic method param",
                emitter,
                ctx,
                data,
            );
            arg_types.push(ty);
        } else if prepared.variadic_args.is_empty() {
            arg_types.push(super::super::calls::args::emit_empty_variadic_array_arg(
                "empty variadic method array",
                emitter,
            ));
        } else {
            arg_types.push(super::super::calls::args::emit_variadic_array_arg_from_exprs(
                &prepared.variadic_args,
                "build variadic method array",
                true,
                true,
                emitter,
                ctx,
                data,
            ));
        }
    }
    arg_types
}

/// Compute outgoing call-argument placement for the given arg types, starting
/// integer register numbering at `first_int_reg`.
fn compute_register_assignments(
    emitter: &Emitter,
    arg_types: &[PhpType],
    first_int_reg: usize,
) -> Vec<crate::codegen::abi::OutgoingArgAssignment> {
    crate::codegen::abi::build_outgoing_arg_assignments_for_target(
        emitter.target,
        arg_types,
        first_int_reg,
    )
}

/// Materialize outgoing call arguments into their assigned registers/stack area.
fn pop_args_to_registers(
    emitter: &mut Emitter,
    assignments: &[crate::codegen::abi::OutgoingArgAssignment],
) -> usize {
    crate::codegen::abi::materialize_outgoing_args(emitter, assignments)
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
        emitter.adrp("x11", "_class_vtable_ptrs");               // load vtable pointer table page
        emitter.add_lo12("x11", "x11", "_class_vtable_ptrs");        // add vtable pointer table offset
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
    let assignments = compute_register_assignments(emitter, arg_types, 1);
    emitter.instruction("ldr x0, [sp], #16");                                   // pop $this into x0
    let overflow_bytes = pop_args_to_registers(emitter, &assignments);
    let ret_ty = emit_dispatch_instance_method(class_name, method, emitter, ctx);
    if overflow_bytes > 0 {
        emitter.instruction(&format!("add sp, sp, #{}", overflow_bytes));       // drop spilled stack arguments after the method call returns
    }
    ret_ty
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
    if ctx.enums.contains_key(&class_name) {
        return emit_enum_static_method_call(&class_name, method, args, emitter, ctx, data);
    }
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
    let assignments = compute_register_assignments(emitter, &arg_types, first_int_reg);

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
    let overflow_bytes = pop_args_to_registers(emitter, &assignments);

    save_concat_offset_before_nested_call(emitter);
    if dynamic_static_dispatch {
        let slot = static_slot.expect("codegen bug: dynamic static dispatch without slot");
        emitter.instruction("mov x10, x0");                                     // preserve forwarded called-class id for static-vtable lookup
        emitter.adrp("x11", "_class_static_vtable_ptrs");        // load static-vtable pointer table page
        emitter.add_lo12("x11", "x11", "_class_static_vtable_ptrs"); // add static-vtable pointer table offset
        emitter.instruction("ldr x11, [x11, x10, lsl #3]");                     // load class-specific static-vtable pointer
        emitter.instruction(&format!("ldr x11, [x11, #{}]", slot * 8));         // load static method entry from static-vtable slot
        emitter.instruction("blr x11");                                         // call late-bound static method implementation
    } else if let Some(label) = direct_static_private_label {
        emitter.instruction(&format!("bl {}", label));                          // call direct private static helper
    } else {
        emitter.instruction(&format!("bl {}", label));                          // call resolved static or parent/self target
    }
    restore_concat_offset_after_nested_call(emitter, &ret_ty);
    if overflow_bytes > 0 {
        emitter.instruction(&format!("add sp, sp, #{}", overflow_bytes));       // drop spilled stack arguments after the static call returns
    }

    ret_ty
}

fn emit_enum_static_method_call(
    enum_name: &str,
    method: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment(&format!("{}::{}()", enum_name, method));
    let Some(enum_info) = ctx.enums.get(enum_name).cloned() else {
        emitter.comment(&format!("WARNING: undefined enum {}", enum_name));
        return PhpType::Int;
    };

    match method {
        "cases" => emit_enum_cases(enum_name, &enum_info, emitter, ctx),
        "from" => emit_enum_from_like(enum_name, &enum_info, args, emitter, ctx, data, false),
        "tryFrom" => emit_enum_from_like(enum_name, &enum_info, args, emitter, ctx, data, true),
        _ => {
            emitter.comment(&format!("WARNING: undefined enum method {}::{}", enum_name, method));
            PhpType::Int
        }
    }
}

fn emit_enum_cases(
    enum_name: &str,
    enum_info: &crate::types::EnumInfo,
    emitter: &mut Emitter,
    _ctx: &mut Context,
) -> PhpType {
    let capacity = if enum_info.cases.is_empty() { 4 } else { enum_info.cases.len() };
    emitter.instruction(&format!("mov x0, #{}", capacity));                     // capacity = exact enum case count (or a small empty-array default)
    emitter.instruction("mov x1, #8");                                          // enum case arrays store one pointer per element
    emitter.instruction("bl __rt_array_new");                                   // allocate the enum cases array
    emitter.instruction("str x0, [sp, #-16]!");                                 // save the array pointer while filling elements

    for (i, case) in enum_info.cases.iter().enumerate() {
        let case_label = enum_case_symbol(enum_name, &case.name);
        emitter.adrp("x9", &format!("{}", case_label));          // load page of the enum singleton slot
        emitter.add_lo12("x9", "x9", &format!("{}", case_label));    // resolve the enum singleton slot address
        emitter.instruction("ldr x0, [x9]");                                    // load the enum singleton pointer from its slot
        crate::codegen::abi::emit_incref_if_refcounted(emitter, &PhpType::Object(enum_name.to_string())); // array storage becomes a new owner of the singleton reference
        emitter.instruction("ldr x9, [sp]");                                    // peek the enum cases array pointer from the stack
        if i == 0 {
            super::super::arrays::emit_array_value_type_stamp(
                emitter,
                "x9",
                &PhpType::Object(enum_name.to_string()),
            );
        }
        emitter.instruction(&format!("str x0, [x9, #{}]", 24 + i * 8));         // store the enum singleton pointer in the array payload
        emitter.instruction(&format!("mov x10, #{}", i + 1));                   // updated array length after appending this enum case
        emitter.instruction("str x10, [x9]");                                   // persist the new enum cases array length
    }

    emitter.instruction("ldr x0, [sp], #16");                                   // pop the enum cases array pointer into x0
    PhpType::Array(Box::new(PhpType::Object(enum_name.to_string())))
}

fn emit_enum_from_like(
    enum_name: &str,
    enum_info: &crate::types::EnumInfo,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    is_try: bool,
) -> PhpType {
    let Some(backing_ty) = enum_info.backing_type.as_ref() else {
        emitter.comment(&format!("WARNING: enum {} has no backing type", enum_name));
        return PhpType::Int;
    };
    let Some(arg) = args.first() else {
        emitter.comment(&format!("WARNING: missing enum backing argument for {}::{}", enum_name, if is_try { "tryFrom" } else { "from" }));
        return PhpType::Int;
    };

    let input_ty = emit_expr(arg, emitter, ctx, data);
    let success_label = ctx.next_label("enum_from_success");
    let done_label = ctx.next_label("enum_from_done");
    let string_cleanup_label = if matches!(backing_ty, PhpType::Str) {
        Some(ctx.next_label("enum_from_cleanup_input"))
    } else {
        None
    };

    match backing_ty {
        PhpType::Int => {
            let _ = input_ty;
            for case in &enum_info.cases {
                let Some(EnumCaseValue::Int(value)) = case.value.as_ref() else {
                    continue;
                };
                let next_label = ctx.next_label("enum_from_next");
                load_immediate(emitter, "x10", *value);                         // materialize the current enum backing integer for comparison
                emitter.instruction("cmp x0, x10");                             // compare the input integer with the current enum backing value
                emitter.instruction(&format!("b.ne {}", next_label));           // continue scanning when the current enum backing value does not match
                let case_label = enum_case_symbol(enum_name, &case.name);
                emitter.adrp("x9", &format!("{}", case_label));  // load page of the matching enum singleton slot
                emitter.add_lo12("x9", "x9", &format!("{}", case_label)); //resolve the matching enum singleton slot address
                emitter.instruction("ldr x0, [x9]");                            // load the matching enum singleton pointer
                emitter.instruction(&format!("b {}", success_label));           // return the matching enum singleton immediately
                emitter.label(&next_label);
            }
        }
        PhpType::Str => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve the input string payload across candidate comparisons
            for case in &enum_info.cases {
                let Some(EnumCaseValue::Str(value)) = case.value.as_ref() else {
                    continue;
                };
                let match_label = ctx.next_label("enum_from_case");
                let next_label = ctx.next_label("enum_from_next");
                let (label, len) = data.add_string(value.as_bytes());
                emitter.instruction("ldp x1, x2, [sp]");                        // reload the input string pointer and length for this candidate
                emitter.adrp("x3", &format!("{}", label));       // load page of the candidate enum backing string
                emitter.add_lo12("x3", "x3", &format!("{}", label)); // resolve the candidate enum backing string address
                emitter.instruction(&format!("mov x4, #{}", len));              // materialize the candidate enum backing string length
                emitter.instruction("bl __rt_str_eq");                          // compare the input string against the candidate backing string
                emitter.instruction(&format!("cbnz x0, {}", match_label));      // branch when the current enum backing string matches
                emitter.instruction(&format!("b {}", next_label));              // continue scanning when the current enum backing string does not match
                emitter.label(&match_label);
                let case_label = enum_case_symbol(enum_name, &case.name);
                emitter.adrp("x9", &format!("{}", case_label));  // load page of the matching enum singleton slot
                emitter.add_lo12("x9", "x9", &format!("{}", case_label)); //resolve the matching enum singleton slot address
                emitter.instruction("ldr x0, [x9]");                            // load the matching enum singleton pointer
                if let Some(cleanup_label) = &string_cleanup_label {
                    emitter.instruction(&format!("b {}", cleanup_label));       // drop the preserved input string before returning the match
                }
                emitter.label(&next_label);
            }
            emitter.instruction("add sp, sp, #16");                             // drop the preserved input string payload after the scan
        }
        _ => {
            emitter.comment("WARNING: unsupported enum backing type in codegen");
            return PhpType::Int;
        }
    }

    if let Some(cleanup_label) = &string_cleanup_label {
        emitter.label(cleanup_label);
        emitter.instruction("add sp, sp, #16");                                 // drop the preserved input string payload before returning the matching singleton
        emitter.instruction(&format!("b {}", success_label));                   // continue through the shared success path with a clean stack
    }

    if is_try {
        emit_null_into_x0(emitter);
        crate::codegen::emit_box_current_value_as_mixed(emitter, &PhpType::Void);
        emitter.instruction(&format!("b {}", done_label));                      // return boxed null when tryFrom() does not match any case
    } else {
        emitter.instruction("bl __rt_enum_from_fail");                          // abort when from() does not match any case
    }

    emitter.label(&success_label);
    if is_try {
        crate::codegen::emit_box_current_value_as_mixed(
            emitter,
            &PhpType::Object(enum_name.to_string()),
        );
    }
    emitter.label(&done_label);
    if is_try {
        PhpType::Union(vec![PhpType::Object(enum_name.to_string()), PhpType::Void])
    } else {
        PhpType::Object(enum_name.to_string())
    }
}

fn emit_null_into_x0(emitter: &mut Emitter) {
    emitter.instruction("movz x0, #0xFFFE");                                    // load lowest 16 bits of the null sentinel
    emitter.instruction("movk x0, #0xFFFF, lsl #16");                           // insert bits 16-31 of the null sentinel
    emitter.instruction("movk x0, #0xFFFF, lsl #32");                           // insert bits 32-47 of the null sentinel
    emitter.instruction("movk x0, #0x7FFF, lsl #48");                           // insert bits 48-63, completing the null sentinel
}

fn load_immediate(emitter: &mut Emitter, reg: &str, value: i64) {
    if (0..=65535).contains(&value) || (-65536..0).contains(&value) {
        emitter.instruction(&format!("mov {}, #{}", reg, value));               // load a small signed immediate directly into the target register
        return;
    }

    let uval = value as u64;
    emitter.instruction(&format!("movz {}, #0x{:x}", reg, uval & 0xFFFF));      // seed the low 16 bits of the wider immediate value
    if (uval >> 16) & 0xFFFF != 0 {
        emitter.instruction(&format!("movk {}, #0x{:x}, lsl #16", reg, (uval >> 16) & 0xFFFF)); //patch bits 16-31 of the wider immediate value
    }
    if (uval >> 32) & 0xFFFF != 0 {
        emitter.instruction(&format!("movk {}, #0x{:x}, lsl #32", reg, (uval >> 32) & 0xFFFF)); //patch bits 32-47 of the wider immediate value
    }
    if (uval >> 48) & 0xFFFF != 0 {
        emitter.instruction(&format!("movk {}, #0x{:x}, lsl #48", reg, (uval >> 48) & 0xFFFF)); //patch bits 48-63 of the wider immediate value
    }
}
