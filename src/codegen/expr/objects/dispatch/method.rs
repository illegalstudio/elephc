//! Purpose:
//! Lowers instance method target selection and invocation.
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
use crate::codegen::functions;
use crate::names::php_symbol_key;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::fiber::emit_fiber_instance_method_dispatch;
use super::prep::{compute_register_assignments, eval_and_push_args, pop_args_to_registers};
use super::vtable::emit_dispatch_instance_method;
use super::super::super::emit_expr;

pub(in crate::codegen::expr::objects) fn emit_method_call_with_pushed_args(
    class_name: &str,
    method: &str,
    arg_types: &[PhpType],
    source_temp_bytes: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    let assignments = compute_register_assignments(emitter, arg_types, 1);
    abi::emit_pop_reg(emitter, abi::int_arg_reg_name(emitter.target, 0));      // pop $this into the first integer argument register for the target ABI
    let overflow_bytes = pop_args_to_registers(emitter, &assignments);
    let ret_ty = if class_name == "Fiber" {
        emit_fiber_instance_method_dispatch(method, &assignments, overflow_bytes, emitter, ctx)
    } else {
        emit_dispatch_instance_method(class_name, method, emitter, ctx)
    };
    abi::emit_release_temporary_stack(emitter, overflow_bytes);                 // drop spilled stack arguments after the method call returns
    abi::emit_release_temporary_stack(emitter, source_temp_bytes);              // drop source-order named-argument temporaries after dispatch
    ret_ty
}

/// Codegen interception for `Fiber::suspend(...)` and `Fiber::getCurrent()`.

pub(in crate::codegen::expr::objects) fn emit_method_call_with_saved_receiver_below_args(
    class_name: &str,
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
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                 // duplicate the saved receiver above the evaluated arguments for normal method dispatch
    let ret_ty = emit_method_call_with_pushed_args(
        class_name,
        method,
        arg_types,
        source_temp_bytes,
        emitter,
        ctx,
    );
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the original receiver slot saved below the argument temporaries
    ret_ty
}

pub(in crate::codegen::expr::objects) fn emit_pushed_method_args(
    args: &[Expr],
    sig: Option<&crate::types::FunctionSig>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> super::super::super::calls::args::EmittedCallArgs {
    eval_and_push_args(args, sig, emitter, ctx, data)
}

fn pushed_arg_temp_bytes(arg_types: &[PhpType]) -> usize {
    arg_types
        .iter()
        .map(|ty| if matches!(ty, PhpType::Void) { 0 } else { 16 })
        .sum()
}

pub(in crate::codegen::expr::objects) fn emit_method_call(
    object: &Expr,
    method: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment(&format!("->{}()", method));

    // Resolve the receiver's static class. Accepts a direct object type or
    // a nullable object union (`?Foo`, `Foo|null`) — for those, the
    // singular Object member's class is used and the runtime unbox below
    // turns null receivers into a controlled fatal before dispatch.
    let obj_ty = functions::infer_contextual_type(object, ctx);
    let class_name = match functions::singular_object_class(&obj_ty) {
        Some(cn) => cn.to_string(),
        None => {
            emitter.comment("WARNING: method call on non-object");
            return PhpType::Int;
        }
    };
    // Evaluate the receiver before arguments, matching PHP's left-to-right
    // call order. When the receiver's codegen-level type is Mixed (the
    // runtime representation for nullable / union object parameters), the
    // result register holds a pointer to a boxed mixed cell rather than the
    // raw object — unbox it so the downstream method dispatch receives the
    // underlying object pointer.
    let runtime_obj_ty = emit_expr(object, emitter, ctx, data);
    if matches!(runtime_obj_ty, PhpType::Mixed | PhpType::Union(_)) {
        let message = format!(
            "Fatal error: Call to a member function {}() on null\n",
            method
        );
        super::super::emit_unbox_mixed_object_or_fatal(
            message.as_bytes(),
            emitter,
            ctx,
            data,
        );
    }
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                 // save the receiver below later argument temporaries for PHP evaluation order

    let method_key = php_symbol_key(method);
    let mut dispatch_method = method_key.as_str();
    let mut magic_args = None;
    let sig = ctx.classes.get(&class_name).and_then(|class_info| {
        if let Some(sig) = class_info.methods.get(&method_key) {
            return Some(sig.clone());
        }
        if let Some(sig) = class_info.methods.get("__call") {
            dispatch_method = "__call";
            magic_args = Some(super::super::magic_method_args(method, args, object.span));
            return Some(sig.clone());
        }
        None
    });
    let args_to_emit = magic_args.as_deref().unwrap_or(args);
    let emitted_args = eval_and_push_args(args_to_emit, sig.as_ref(), emitter, ctx, data);

    emit_method_call_with_saved_receiver_below_args(
        &class_name,
        dispatch_method,
        &emitted_args.arg_types,
        emitted_args.source_temp_bytes,
        emitter,
        ctx,
    )
}
