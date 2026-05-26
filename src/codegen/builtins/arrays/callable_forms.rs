//! Purpose:
//! Lowers non-scalar PHP callable forms used by dynamic-call builtins.
//! Handles invokable objects and static/literal callable arrays before generic pointer dispatch.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::call_user_func`
//! - `crate::codegen::builtins::arrays::call_user_func_array`
//!
//! Key details:
//! - These paths preserve PHP evaluation order by delegating to normal method/static-call emitters.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::functions;
use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{CallableTarget, Expr, ExprKind, StaticReceiver};
use crate::types::PhpType;

/// Emits assembly for call user func form.
pub(crate) fn emit_call_user_func_form(
    callback: &Expr,
    callback_args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    match resolve_callable_form(callback, ctx) {
        Some(CallableForm::InvokableObject { object, .. }) => Some(
            crate::codegen::expr::objects::emit_method_call(
                &object,
                "__invoke",
                callback_args,
                emitter,
                ctx,
                data,
            ),
        ),
        Some(CallableForm::InstanceMethod { object, method, .. }) => Some(
            crate::codegen::expr::objects::emit_method_call(
                &object,
                &method,
                callback_args,
                emitter,
                ctx,
                data,
            ),
        ),
        Some(CallableForm::StaticMethod { receiver, method }) => Some(
            crate::codegen::expr::objects::emit_static_method_call(
                &receiver,
                &method,
                callback_args,
                emitter,
                ctx,
                data,
            ),
        ),
        None => None,
    }
}

/// Emits assembly for call user func array form.
pub(crate) fn emit_call_user_func_array_form(
    callback: &Expr,
    arg_array: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let spread_args = vec![Expr::new(
        ExprKind::Spread(Box::new(arg_array.clone())),
        arg_array.span,
    )];
    emit_call_user_func_form(callback, &spread_args, emitter, ctx, data)
}

enum CallableForm {
    InvokableObject {
        object: Expr,
    },
    InstanceMethod {
        object: Expr,
        method: String,
    },
    StaticMethod {
        receiver: StaticReceiver,
        method: String,
    },
}

/// Resolves callable form using the available compile-time metadata.
fn resolve_callable_form(callback: &Expr, ctx: &Context) -> Option<CallableForm> {
    if let ExprKind::Variable(var_name) = &callback.kind {
        if let Some(target) = ctx.callable_array_targets.get(var_name) {
            return callable_target_form(target);
        }
    }

    if let Some((receiver, method)) = callable_array_parts(callback) {
        if let Some(receiver) = static_callable_receiver(receiver, ctx) {
            return Some(CallableForm::StaticMethod {
                receiver,
                method: method.to_string(),
            });
        }
        let receiver_ty = functions::infer_contextual_type(receiver, ctx);
        let class_name = functions::singular_object_class(&receiver_ty)?;
        if ctx
            .classes
            .get(class_name)
            .is_some_and(|class_info| class_info.methods.contains_key(&php_symbol_key(method)))
        {
            return Some(CallableForm::InstanceMethod {
                object: receiver.clone(),
                method: method.to_string(),
            });
        }
        return None;
    }

    let callback_ty = functions::infer_contextual_type(callback, ctx);
    let class_name = functions::singular_object_class(&callback_ty)?;
    if ctx
        .classes
        .get(class_name)
        .is_some_and(|class_info| class_info.methods.contains_key("__invoke"))
    {
        Some(CallableForm::InvokableObject {
            object: callback.clone(),
        })
    } else {
        None
    }
}

/// Provides the Callable target form helper used by the callable forms module.
fn callable_target_form(target: &CallableTarget) -> Option<CallableForm> {
    match target {
        CallableTarget::Method { object, method } => Some(CallableForm::InstanceMethod {
            object: *object.clone(),
            method: method.clone(),
        }),
        CallableTarget::StaticMethod { receiver, method } => Some(CallableForm::StaticMethod {
            receiver: receiver.clone(),
            method: method.clone(),
        }),
        CallableTarget::Function(_) => None,
    }
}

/// Provides the Callable array parts helper used by the callable forms module.
fn callable_array_parts(callback: &Expr) -> Option<(&Expr, &str)> {
    let elems = match &callback.kind {
        ExprKind::ArrayLiteral(elems) => elems,
        _ => return None,
    };
    if elems.len() != 2 {
        return None;
    }
    let ExprKind::StringLiteral(method) = &elems[1].kind else {
        return None;
    };
    Some((&elems[0], method.as_str()))
}

/// Provides the Static callable receiver helper used by the callable forms module.
fn static_callable_receiver(receiver: &Expr, ctx: &Context) -> Option<StaticReceiver> {
    let class_name = match &receiver.kind {
        ExprKind::StringLiteral(class_name) => {
            resolve_class_name(ctx, class_name).map(str::to_string)
        }
        ExprKind::ClassConstant { receiver } => resolve_static_receiver_class(receiver, ctx),
        _ => None,
    }?;
    Some(StaticReceiver::Named(Name::from(class_name)))
}

/// Resolves static receiver class using the available compile-time metadata.
fn resolve_static_receiver_class(receiver: &StaticReceiver, ctx: &Context) -> Option<String> {
    match receiver {
        StaticReceiver::Named(name) => resolve_class_name(ctx, name.as_str()).map(str::to_string),
        StaticReceiver::Self_ | StaticReceiver::Static => ctx.current_class.clone(),
        StaticReceiver::Parent => ctx
            .current_class
            .as_ref()
            .and_then(|class_name| ctx.classes.get(class_name))
            .and_then(|class_info| class_info.parent.clone()),
    }
}

/// Resolves class name using the available compile-time metadata.
fn resolve_class_name<'a>(ctx: &'a Context, class_name: &str) -> Option<&'a str> {
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    ctx.classes
        .keys()
        .find(|existing| php_symbol_key(existing) == class_key)
        .map(String::as_str)
}
