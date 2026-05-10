//! Purpose:
//! Lowers static method call target selection and invocation.
//! Shares receiver preparation and ABI call conventions with the object call dispatcher.
//!
//! Called from:
//! - `crate::codegen::expr::objects::dispatch`
//!
//! Key details:
//! - Receiver ownership, late/static binding, and vtable slot layout must match class metadata emission.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::names::{method_symbol, static_method_symbol};
use crate::parser::ast::{Expr, StaticReceiver};
use crate::types::PhpType;

use super::enums::emit_enum_static_method_call;
use super::fiber::emit_fiber_static_method_dispatch;
use super::prep::{compute_register_assignments, eval_and_push_args, pop_args_to_registers};
use super::super::super::{
    restore_concat_offset_after_nested_call, save_concat_offset_before_nested_call,
};

pub(in crate::codegen::expr::objects) fn emit_immediate_class_id(emitter: &mut Emitter, class_id: u64) {
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), class_id as i64);
}

pub(in crate::codegen::expr::objects) fn emit_forwarded_called_class_id(emitter: &mut Emitter, ctx: &Context) -> bool {
    if let Some(var) = ctx.variables.get("__elephc_fcc_called_class_id") {
        abi::load_at_offset(emitter, abi::int_result_reg(emitter), var.stack_offset); // forward the first-class callable's captured called-class id
        return true;
    }
    if let Some(var) = ctx.variables.get("__elephc_called_class_id") {
        abi::load_at_offset(emitter, abi::int_result_reg(emitter), var.stack_offset); // forward the hidden called-class id from the current static method frame
        true
    } else if let Some(var) = ctx.variables.get("__elephc_fcc_this") {
        abi::load_at_offset(emitter, abi::int_result_reg(emitter), var.stack_offset); // load the first-class callable's captured receiver for dynamic static dispatch
        abi::emit_load_from_address(
            emitter,
            abi::int_result_reg(emitter),
            abi::int_result_reg(emitter),
            0,
        );
        true
    } else if let Some(var) = ctx.variables.get("this") {
        abi::load_at_offset(emitter, abi::int_result_reg(emitter), var.stack_offset); // load the implicit $this pointer for dynamic static dispatch
        abi::emit_load_from_address(
            emitter,
            abi::int_result_reg(emitter),
            abi::int_result_reg(emitter),
            0,
        );
        true
    } else {
        false
    }
}

pub(in crate::codegen::expr::objects) fn emit_static_method_call(
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
    if class_name == "Fiber" {
        return emit_fiber_static_method_dispatch(method, args, emitter, ctx, data);
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
    let emitted_args = eval_and_push_args(args, sig.as_ref(), emitter, ctx, data);
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

    let first_int_reg =
        (if needs_called_class_id { 1 } else { 0 }) + (if needs_this { 1 } else { 0 });
    let assignments = compute_register_assignments(emitter, &emitted_args.arg_types, first_int_reg);
    let hidden_called_class_reg = abi::int_arg_reg_name(emitter.target, 0);
    let hidden_this_reg =
        abi::int_arg_reg_name(emitter.target, if needs_called_class_id { 1 } else { 0 });
    let class_id_scratch = abi::temp_int_reg(emitter.target);
    let dispatch_scratch = abi::symbol_scratch_reg(emitter);

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
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // push the hidden called-class id before loading the visible arguments
    }

    if needs_this {
        let this_var = match ctx.variables.get("this") {
            Some(var) => var,
            None => {
                emitter.comment("WARNING: direct scoped instance call without $this");
                return PhpType::Int;
            }
        };
        abi::load_at_offset(emitter, abi::int_result_reg(emitter), this_var.stack_offset); // load the implicit scoped-call receiver into the integer result register
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // push the implicit receiver before visible argument materialization
    }

    if needs_called_class_id {
        abi::emit_pop_reg(emitter, hidden_called_class_reg);                    // pop the hidden called-class id into its outgoing ABI register
    }
    if needs_this {
        abi::emit_pop_reg(emitter, hidden_this_reg);                            // pop the implicit receiver into its outgoing ABI register
    }
    let overflow_bytes = pop_args_to_registers(emitter, &assignments);

    save_concat_offset_before_nested_call(emitter, ctx);
    if dynamic_static_dispatch {
        let slot = static_slot.expect("codegen bug: dynamic static dispatch without slot");
        emitter.instruction(&format!("mov {}, {}", class_id_scratch, hidden_called_class_reg)); // preserve the forwarded called-class id across static-vtable address materialization
        abi::emit_symbol_address(emitter, dispatch_scratch, "_class_static_vtable_ptrs");
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction(&format!("ldr {}, [{}, {}, lsl #3]", dispatch_scratch, dispatch_scratch, class_id_scratch)); // load the class-specific static-vtable pointer from the global table
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction(&format!("mov {}, QWORD PTR [{} + {} * 8]", dispatch_scratch, dispatch_scratch, class_id_scratch)); // load the class-specific static-vtable pointer from the global table
            }
        }
        abi::emit_load_from_address(emitter, dispatch_scratch, dispatch_scratch, slot * 8); // load the selected static method entry from the class-specific vtable
        abi::emit_call_reg(emitter, dispatch_scratch);                          // call the late-bound static method implementation
    } else if let Some(label) = direct_static_private_label {
        abi::emit_call_label(emitter, &label);                                  // call the direct private static helper
    } else {
        abi::emit_call_label(emitter, &label);                                  // call the resolved static or parent/self method target
    }
    restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
    if overflow_bytes > 0 {
        abi::emit_release_temporary_stack(emitter, overflow_bytes);             // drop spilled stack arguments after the static call returns
    }
    abi::emit_release_temporary_stack(emitter, emitted_args.source_temp_bytes);  // drop source-order named-argument temporaries after the static call

    ret_ty
}
