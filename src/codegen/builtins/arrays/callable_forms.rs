//! Purpose:
//! Lowers non-scalar PHP callable forms used by dynamic-call builtins.
//! Handles invokable objects and static/literal callable arrays before generic pointer dispatch.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::call_user_func`
//! - `crate::codegen::builtins::arrays::call_user_func_array`
//!
//! Key details:
//! - Descriptor-capable shapes route through the uniform invoker; unsupported
//!   shapes preserve PHP evaluation order by delegating to normal method emitters.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::callable_dispatch::RuntimeInstanceCallableShape;
use crate::codegen::functions;
use crate::codegen::{abi, callable_dispatch};
use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{CallableTarget, Expr, ExprKind, StaticReceiver};
use crate::types::PhpType;

use super::call_user_func_array::{self, LoadedArraySource};
use super::descriptor_arg_builder;
use super::receiver_call_args;

/// Emits assembly for call user func form.
pub(crate) fn emit_call_user_func_form(
    callback: &Expr,
    callback_args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    match resolve_callable_form(callback, ctx) {
        Some(CallableForm::InvokableObject { object, .. }) => {
            if callback_args_have_spread(callback_args) {
                return emit_instance_method_descriptor_spread_form(
                    &object,
                    "__invoke",
                    RuntimeInstanceCallableShape::ObjectInvoke,
                    callback_args,
                    callback.span,
                    emitter,
                    ctx,
                    data,
                )
                .or_else(|| {
                    Some(crate::codegen::expr::objects::emit_method_call(
                        &object,
                        "__invoke",
                        callback_args,
                        emitter,
                        ctx,
                        data,
                    ))
                });
            }
            let descriptor_args =
                receiver_prefixed_indexed_arg_array(&object, callback_args, callback.span);
            emit_instance_method_descriptor_form(
                &object,
                "__invoke",
                RuntimeInstanceCallableShape::ObjectInvoke,
                emitter,
                ctx,
                data,
                &descriptor_args,
            )
            .or_else(|| {
                Some(crate::codegen::expr::objects::emit_method_call(
                    &object,
                    "__invoke",
                    callback_args,
                    emitter,
                    ctx,
                    data,
                ))
            })
        }
        Some(CallableForm::InstanceMethod { object, method, .. }) => {
            if callback_args_have_spread(callback_args) {
                return emit_instance_method_descriptor_spread_form(
                    &object,
                    &method,
                    RuntimeInstanceCallableShape::InstanceMethod,
                    callback_args,
                    callback.span,
                    emitter,
                    ctx,
                    data,
                )
                .or_else(|| {
                    Some(crate::codegen::expr::objects::emit_method_call(
                        &object,
                        &method,
                        callback_args,
                        emitter,
                        ctx,
                        data,
                    ))
                });
            }
            let descriptor_args =
                receiver_prefixed_indexed_arg_array(&object, callback_args, callback.span);
            emit_instance_method_descriptor_form(
                &object,
                &method,
                RuntimeInstanceCallableShape::InstanceMethod,
                emitter,
                ctx,
                data,
                &descriptor_args,
            )
            .or_else(|| {
                Some(crate::codegen::expr::objects::emit_method_call(
                    &object,
                    &method,
                    callback_args,
                    emitter,
                    ctx,
                    data,
                ))
            })
        }
        Some(CallableForm::StaticMethod { receiver, method }) => {
            if callback_args_have_spread(callback_args) {
                return emit_static_method_descriptor_spread_form(
                    &receiver,
                    &method,
                    callback_args,
                    emitter,
                    ctx,
                    data,
                )
                .or_else(|| {
                    Some(crate::codegen::expr::objects::emit_static_method_call(
                        &receiver,
                        &method,
                        callback_args,
                        emitter,
                        ctx,
                        data,
                    ))
                });
            }
            emit_static_method_descriptor_form(
                &receiver,
                &method,
                emitter,
                ctx,
                data,
                &Expr::new(ExprKind::ArrayLiteral(callback_args.to_vec()), callback.span),
            )
            .or_else(|| {
                Some(crate::codegen::expr::objects::emit_static_method_call(
                    &receiver,
                    &method,
                    callback_args,
                    emitter,
                    ctx,
                    data,
                ))
            })
        }
        None => crate::codegen::expr::calls::emit_runtime_callable_array_call(
            callback,
            callback_args,
            emitter,
            ctx,
            data,
        ),
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
    if let Some(form) = resolve_callable_form(callback, ctx) {
        match form {
            CallableForm::StaticMethod { receiver, method } => {
                if let Some(ret_ty) = emit_static_method_descriptor_form(
                    &receiver,
                    &method,
                    emitter,
                    ctx,
                    data,
                    arg_array,
                ) {
                    return Some(ret_ty);
                }
            }
            CallableForm::InvokableObject { object } => {
                if let Some(descriptor_args) =
                    receiver_prefixed_call_user_func_array_args(&object, arg_array)
                {
                    if let Some(ret_ty) = emit_instance_method_descriptor_form(
                        &object,
                        "__invoke",
                        RuntimeInstanceCallableShape::ObjectInvoke,
                        emitter,
                        ctx,
                        data,
                        &descriptor_args,
                    ) {
                        return Some(ret_ty);
                    }
                }
                if let Some(ret_ty) = emit_instance_method_descriptor_dynamic_arg_form(
                    &object,
                    "__invoke",
                    RuntimeInstanceCallableShape::ObjectInvoke,
                    arg_array,
                    emitter,
                    ctx,
                    data,
                ) {
                    return Some(ret_ty);
                }
            }
            CallableForm::InstanceMethod { object, method } => {
                if let Some(descriptor_args) =
                    receiver_prefixed_call_user_func_array_args(&object, arg_array)
                {
                    if let Some(ret_ty) = emit_instance_method_descriptor_form(
                        &object,
                        &method,
                        RuntimeInstanceCallableShape::InstanceMethod,
                        emitter,
                        ctx,
                        data,
                        &descriptor_args,
                    ) {
                        return Some(ret_ty);
                    }
                }
                if let Some(ret_ty) = emit_instance_method_descriptor_dynamic_arg_form(
                    &object,
                    &method,
                    RuntimeInstanceCallableShape::InstanceMethod,
                    arg_array,
                    emitter,
                    ctx,
                    data,
                ) {
                    return Some(ret_ty);
                }
            }
        }
    }

    let spread_args = vec![Expr::new(
        ExprKind::Spread(Box::new(arg_array.clone())),
        arg_array.span,
    )];
    emit_call_user_func_form(callback, &spread_args, emitter, ctx, data)
}

/// Invokes receiver-bound `call_user_func()` spread args through the descriptor invoker.
#[allow(clippy::too_many_arguments)]
fn emit_instance_method_descriptor_spread_form(
    object: &Expr,
    method: &str,
    shape: RuntimeInstanceCallableShape,
    callback_args: &[Expr],
    _span: crate::span::Span,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    if let Some(arg_array) = single_spread_inner(callback_args) {
        return emit_instance_method_descriptor_dynamic_arg_form(
            object, method, shape, arg_array, emitter, ctx, data,
        );
    }

    emit_instance_method_descriptor_positional_spread_form(
        object,
        method,
        shape,
        callback_args,
        emitter,
        ctx,
        data,
    )
}

/// Returns the spread source when `call_user_func()` forwards one spread argument segment.
fn single_spread_inner(args: &[Expr]) -> Option<&Expr> {
    if let [arg] = args {
        if let ExprKind::Spread(inner) = &arg.kind {
            return Some(inner);
        }
    }
    None
}

/// Invokes receiver-bound positional+spread `call_user_func()` args through descriptors.
#[allow(clippy::too_many_arguments)]
fn emit_instance_method_descriptor_positional_spread_form(
    object: &Expr,
    method: &str,
    shape: RuntimeInstanceCallableShape,
    callback_args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let receiver_ty = functions::infer_contextual_type(object, ctx);
    let class_name = functions::singular_object_class(&receiver_ty)?;
    let case =
        callable_dispatch::runtime_instance_method_case(ctx, data, class_name, method, shape)?;
    if !case.has_invoker {
        return None;
    }
    let save_concat_before_args =
        emitter.target.arch == crate::codegen::platform::Arch::X86_64;
    if save_concat_before_args {
        crate::codegen::expr::save_concat_offset_before_nested_call(emitter, ctx);
    }

    let leading_args = vec![object.clone()];
    let arg_array_ty = descriptor_arg_builder::emit_positional_spread_invoker_arg_array(
        &leading_args,
        callback_args,
        Some(&case.sig),
        true,
        emitter,
        ctx,
        data,
    )?;
    let call_reg = abi::nested_call_reg(emitter);
    abi::emit_symbol_address(emitter, call_reg, &case.descriptor_label);
    call_user_func_array::emit_call_descriptor_array_invoker(
        LoadedArraySource::Result,
        &arg_array_ty,
        call_reg,
        save_concat_before_args,
        emitter,
        ctx,
        data,
    );
    Some(PhpType::Mixed)
}

/// Invokes a public instance-method or `__invoke` callable through its descriptor invoker.
fn emit_instance_method_descriptor_form(
    object: &Expr,
    method: &str,
    shape: RuntimeInstanceCallableShape,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    arg_array: &Expr,
) -> Option<PhpType> {
    let receiver_ty = functions::infer_contextual_type(object, ctx);
    let class_name = functions::singular_object_class(&receiver_ty)?;
    let case =
        callable_dispatch::runtime_instance_method_case(ctx, data, class_name, method, shape)?;
    if !case.has_invoker {
        return None;
    }
    let save_concat_before_args =
        emitter.target.arch == crate::codegen::platform::Arch::X86_64;
    if save_concat_before_args {
        crate::codegen::expr::save_concat_offset_before_nested_call(emitter, ctx);
    }

    let arr_ty = emit_expr(arg_array, emitter, ctx, data);
    let call_reg = abi::nested_call_reg(emitter);
    abi::emit_symbol_address(emitter, call_reg, &case.descriptor_label);
    call_user_func_array::emit_call_descriptor_array_invoker(
        LoadedArraySource::Result,
        &arr_ty,
        call_reg,
        save_concat_before_args,
        emitter,
        ctx,
        data,
    );
    Some(PhpType::Mixed)
}

/// Invokes a receiver-bound descriptor with a dynamic `call_user_func_array()` container.
fn emit_instance_method_descriptor_dynamic_arg_form(
    object: &Expr,
    method: &str,
    shape: RuntimeInstanceCallableShape,
    arg_array: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let inferred_arg_ty = functions::infer_contextual_type(arg_array, ctx);
    if !matches!(
        inferred_arg_ty.codegen_repr(),
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed
    ) {
        return None;
    }
    let receiver_ty = functions::infer_contextual_type(object, ctx);
    let class_name = functions::singular_object_class(&receiver_ty)?;
    let case =
        callable_dispatch::runtime_instance_method_case(ctx, data, class_name, method, shape)?;
    if !case.has_invoker {
        return None;
    }
    let save_concat_before_args =
        emitter.target.arch == crate::codegen::platform::Arch::X86_64;
    if save_concat_before_args {
        crate::codegen::expr::save_concat_offset_before_nested_call(emitter, ctx);
    }

    if !receiver_call_args::emit_receiver_prefixed_dynamic_arg_mixed(
        object,
        arg_array,
        &inferred_arg_ty,
        emitter,
        ctx,
        data,
    ) {
        return None;
    }
    let call_reg = abi::nested_call_reg(emitter);
    abi::emit_symbol_address(emitter, call_reg, &case.descriptor_label);
    call_user_func_array::emit_call_descriptor_array_invoker(
        LoadedArraySource::Result,
        &PhpType::Mixed,
        call_reg,
        save_concat_before_args,
        emitter,
        ctx,
        data,
    );
    Some(PhpType::Mixed)
}

/// Invokes static-method positional+spread `call_user_func()` args through descriptors.
fn emit_static_method_descriptor_spread_form(
    receiver: &StaticReceiver,
    method: &str,
    callback_args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let StaticReceiver::Named(class_name) = receiver else {
        return None;
    };
    let case = callable_dispatch::runtime_static_method_case(
        ctx,
        data,
        class_name.as_str(),
        method,
    )?;
    if !case.has_invoker {
        return None;
    }
    let save_concat_before_args =
        emitter.target.arch == crate::codegen::platform::Arch::X86_64;
    if save_concat_before_args {
        crate::codegen::expr::save_concat_offset_before_nested_call(emitter, ctx);
    }

    let arg_array_ty = descriptor_arg_builder::emit_positional_spread_invoker_arg_array(
        &[],
        callback_args,
        Some(&case.sig),
        true,
        emitter,
        ctx,
        data,
    )?;
    let call_reg = abi::nested_call_reg(emitter);
    abi::emit_symbol_address(emitter, call_reg, &case.descriptor_label);
    call_user_func_array::emit_call_descriptor_array_invoker(
        LoadedArraySource::Result,
        &arg_array_ty,
        call_reg,
        save_concat_before_args,
        emitter,
        ctx,
        data,
    );
    Some(PhpType::Mixed)
}

/// Invokes a public static-method callable array through its descriptor invoker.
fn emit_static_method_descriptor_form(
    receiver: &StaticReceiver,
    method: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    arg_array: &Expr,
) -> Option<PhpType> {
    let StaticReceiver::Named(class_name) = receiver else {
        return None;
    };
    let case = callable_dispatch::runtime_static_method_case(
        ctx,
        data,
        class_name.as_str(),
        method,
    )?;
    if !case.has_invoker {
        return None;
    }
    let save_concat_before_args =
        emitter.target.arch == crate::codegen::platform::Arch::X86_64;
    if save_concat_before_args {
        crate::codegen::expr::save_concat_offset_before_nested_call(emitter, ctx);
    }

    let arr_ty = emit_expr(arg_array, emitter, ctx, data);
    let call_reg = abi::nested_call_reg(emitter);
    abi::emit_symbol_address(emitter, call_reg, &case.descriptor_label);
    call_user_func_array::emit_call_descriptor_array_invoker(
        LoadedArraySource::Result,
        &arr_ty,
        call_reg,
        save_concat_before_args,
        emitter,
        ctx,
        data,
    );
    Some(PhpType::Mixed)
}

/// Builds an indexed descriptor argument literal with receiver prepended.
fn receiver_prefixed_indexed_arg_array(
    receiver: &Expr,
    args: &[Expr],
    span: crate::span::Span,
) -> Expr {
    let mut elems = Vec::with_capacity(args.len() + 1);
    elems.push(receiver.clone());
    elems.extend(args.iter().cloned());
    Expr::new(ExprKind::ArrayLiteral(elems), span)
}

/// Builds descriptor invoker args for `call_user_func_array()` when safe to rewrite.
fn receiver_prefixed_call_user_func_array_args(
    receiver: &Expr,
    arg_array: &Expr,
) -> Option<Expr> {
    match &arg_array.kind {
        ExprKind::ArrayLiteral(elems) => {
            let mut prefixed = Vec::with_capacity(elems.len() + 1);
            prefixed.push(receiver.clone());
            prefixed.extend(elems.iter().cloned());
            Some(Expr::new(ExprKind::ArrayLiteral(prefixed), arg_array.span))
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            let mut prefixed = Vec::with_capacity(pairs.len() + 1);
            prefixed.push((
                Expr::new(ExprKind::IntLiteral(0), arg_array.span),
                receiver.clone(),
            ));
            prefixed.extend(pairs.iter().cloned());
            Some(Expr::new(ExprKind::ArrayLiteralAssoc(prefixed), arg_array.span))
        }
        _ => None,
    }
}

/// Returns true when descriptor argument literal rewriting would need spread support.
fn callback_args_have_spread(args: &[Expr]) -> bool {
    args.iter().any(|arg| matches!(arg.kind, ExprKind::Spread(_)))
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
