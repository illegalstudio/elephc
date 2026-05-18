//! Purpose:
//! Lowers `$obj[$key]` syntax for objects that implement PHP's `ArrayAccess`.
//! Routes read, write, isset, and unset forms through the corresponding `offset*` methods.
//!
//! Called from:
//! - `crate::codegen::expr::arrays::access`
//! - `crate::codegen::stmt::arrays`
//! - `crate::codegen::builtins`
//!
//! Key details:
//! - Receiver evaluation and argument evaluation reuse normal method-call lowering so Mixed boxing
//!   follows the declared `ArrayAccess` method signatures.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::functions;
use crate::parser::ast::Expr;
use crate::types::{FunctionSig, PhpType};

enum ArrayAccessDispatchTarget {
    Class(String),
    Interface(String),
}

pub(crate) fn expr_is_array_access_object(expr: &Expr, ctx: &Context) -> bool {
    let ty = functions::infer_contextual_type(expr, ctx);
    type_is_array_access_object(&ty, ctx)
}

pub(crate) fn type_is_array_access_object(ty: &PhpType, ctx: &Context) -> bool {
    match ty {
        PhpType::Object(name) => ctx.object_type_implements_interface(name, "ArrayAccess"),
        PhpType::Union(members) => {
            let mut saw_object = false;
            for member in members {
                match member {
                    PhpType::Void => {}
                    PhpType::Object(name) => {
                        if !ctx.object_type_implements_interface(name, "ArrayAccess") {
                            return false;
                        }
                        saw_object = true;
                    }
                    _ => return false,
                }
            }
            saw_object
        }
        _ => false,
    }
}

pub(crate) fn emit_offset_get(
    object: &Expr,
    index: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emit_array_access_method(object, "offsetget", &[index.clone()], emitter, ctx, data)
}

pub(crate) fn emit_offset_set(
    object: &Expr,
    index: &Expr,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emit_array_access_method(
        object,
        "offsetset",
        &[index.clone(), value.clone()],
        emitter,
        ctx,
        data,
    )
}

pub(crate) fn emit_offset_exists(
    object: &Expr,
    index: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emit_array_access_method(object, "offsetexists", &[index.clone()], emitter, ctx, data)
}

pub(crate) fn emit_offset_unset(
    object: &Expr,
    index: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emit_array_access_method(object, "offsetunset", &[index.clone()], emitter, ctx, data)
}

fn emit_array_access_method(
    object: &Expr,
    method: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment(&format!("ArrayAccess::{}()", method));
    let static_ty = functions::infer_contextual_type(object, ctx);
    let Some(target) = array_access_dispatch_target(&static_ty, ctx) else {
        emitter.comment("WARNING: ArrayAccess subscript on non-ArrayAccess receiver");
        return PhpType::Int;
    };

    let runtime_ty = crate::codegen::expr::emit_expr(object, emitter, ctx, data);
    if matches!(runtime_ty, PhpType::Mixed | PhpType::Union(_)) {
        let message = format!(
            "Fatal error: Call to a member function {}() on null\n",
            method
        );
        crate::codegen::expr::objects::emit_unbox_mixed_object_or_fatal(
            message.as_bytes(),
            emitter,
            ctx,
            data,
        );
    }
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                 // save the ArrayAccess receiver below evaluated offset/value arguments

    let sig = array_access_method_sig(&target, method, ctx);
    let emitted_args = crate::codegen::expr::objects::emit_pushed_method_args(
        args,
        sig.as_ref(),
        emitter,
        ctx,
        data,
    );

    match target {
        ArrayAccessDispatchTarget::Class(class_name) => {
            crate::codegen::expr::objects::emit_method_call_with_saved_receiver_below_args(
                &class_name,
                method,
                &emitted_args.arg_types,
                emitted_args.source_temp_bytes,
                emitter,
                ctx,
            )
        }
        ArrayAccessDispatchTarget::Interface(interface_name) => {
            emit_interface_method_call_with_saved_receiver_below_args(
                &interface_name,
                method,
                &emitted_args.arg_types,
                emitted_args.source_temp_bytes,
                emitter,
                ctx,
            )
        }
    }
}

fn array_access_dispatch_target(
    ty: &PhpType,
    ctx: &Context,
) -> Option<ArrayAccessDispatchTarget> {
    match ty {
        PhpType::Object(name) if ctx.classes.contains_key(name) => ctx
            .object_type_implements_interface(name, "ArrayAccess")
            .then(|| ArrayAccessDispatchTarget::Class(name.clone())),
        PhpType::Object(name) if ctx.interfaces.contains_key(name) => ctx
            .object_type_implements_interface(name, "ArrayAccess")
            .then(|| ArrayAccessDispatchTarget::Interface("ArrayAccess".to_string())),
        PhpType::Union(members) => {
            let mut class_target: Option<String> = None;
            let mut needs_interface_dispatch = false;
            let mut saw_array_access_object = false;
            for member in members {
                match member {
                    PhpType::Void => {}
                    PhpType::Object(name) if ctx.classes.contains_key(name) => {
                        if !ctx.object_type_implements_interface(name, "ArrayAccess") {
                            return None;
                        }
                        saw_array_access_object = true;
                        match &class_target {
                            Some(existing) if existing != name => {
                                needs_interface_dispatch = true;
                            }
                            None => {
                                class_target = Some(name.clone());
                            }
                            _ => {}
                        }
                    }
                    PhpType::Object(name) if ctx.interfaces.contains_key(name) => {
                        if !ctx.object_type_implements_interface(name, "ArrayAccess") {
                            return None;
                        }
                        saw_array_access_object = true;
                        needs_interface_dispatch = true;
                    }
                    _ => return None,
                }
            }
            if !saw_array_access_object {
                None
            } else if needs_interface_dispatch {
                Some(ArrayAccessDispatchTarget::Interface(
                    "ArrayAccess".to_string(),
                ))
            } else {
                class_target.map(ArrayAccessDispatchTarget::Class)
            }
        }
        _ => None,
    }
}

fn array_access_method_sig(
    target: &ArrayAccessDispatchTarget,
    method: &str,
    ctx: &Context,
) -> Option<FunctionSig> {
    match target {
        ArrayAccessDispatchTarget::Class(class_name) => ctx
            .classes
            .get(class_name)
            .and_then(|class_info| class_info.methods.get(method))
            .cloned(),
        ArrayAccessDispatchTarget::Interface(interface_name) => ctx
            .interfaces
            .get(interface_name)
            .and_then(|interface_info| interface_info.methods.get(method))
            .cloned(),
    }
}

fn emit_interface_method_call_with_saved_receiver_below_args(
    interface_name: &str,
    method: &str,
    arg_types: &[PhpType],
    source_temp_bytes: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    let arg_temp_bytes = pushed_arg_temp_bytes(arg_types) + source_temp_bytes;
    abi::emit_load_temporary_stack_slot(
        emitter,
        abi::int_result_reg(emitter),
        arg_temp_bytes,
    );
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                 // duplicate the saved ArrayAccess receiver above the evaluated arguments
    let ret_ty = emit_interface_method_call_with_pushed_args(
        interface_name,
        method,
        arg_types,
        source_temp_bytes,
        emitter,
        ctx,
    );
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the original receiver slot saved below the argument temporaries
    ret_ty
}

fn emit_interface_method_call_with_pushed_args(
    interface_name: &str,
    method: &str,
    arg_types: &[PhpType],
    source_temp_bytes: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    let assignments = abi::build_outgoing_arg_assignments_for_target(emitter.target, arg_types, 1);
    abi::emit_pop_reg(emitter, abi::int_arg_reg_name(emitter.target, 0));      // pop the ArrayAccess receiver into the first integer argument register
    let overflow_bytes = abi::materialize_outgoing_args(emitter, &assignments);
    let ret_ty = crate::codegen::expr::objects::dispatch::emit_dispatch_interface_method(
        interface_name,
        method,
        emitter,
        ctx,
    );
    abi::emit_release_temporary_stack(emitter, overflow_bytes);                 // drop spilled stack arguments after the interface method call returns
    abi::emit_release_temporary_stack(emitter, source_temp_bytes);              // drop source-order named-argument temporaries after dispatch
    ret_ty
}

fn pushed_arg_temp_bytes(arg_types: &[PhpType]) -> usize {
    arg_types
        .iter()
        .map(|ty| if matches!(ty, PhpType::Void) { 0 } else { 16 })
        .sum()
}
