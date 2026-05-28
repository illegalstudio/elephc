//! Purpose:
//! Dispatches function-like expression calls including direct, indirect, closure, method-adjacent, and first-class forms.
//! Coordinates call signatures, argument lowering, and result typing for expression consumers.
//!
//! Called from:
//! - `crate::codegen::expr::emit_expr()`
//!
//! Key details:
//! - Argument evaluation must preserve PHP source order before ABI materialization happens in call-argument helpers.

pub(crate) mod args;
mod callable_array_runtime;
mod closure;
mod descriptor_invoker_args;
mod descriptor_value;
mod first_class;
mod function;
mod indirect;
mod pipe;

use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use super::Expr;
use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{CallableTarget, ExprKind, StaticReceiver, TypeExpr};
use crate::span::Span;
use crate::types::PhpType;

/// Emits a direct or namespaced function call by name.
pub(super) fn emit_function_call(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    function::emit_function_call(name, args, emitter, ctx, data)
}

/// Emits a closure (anonymous function) definition with captures.
pub(super) fn emit_closure(
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    variadic: &Option<String>,
    return_type: &Option<TypeExpr>,
    body: &[crate::parser::ast::Stmt],
    captures: &[String],
    capture_refs: &[String],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    closure::emit_closure(
        params,
        variadic,
        return_type,
        body,
        captures,
        capture_refs,
        emitter,
        ctx,
        data,
    )
}

/// Emits a closure call expression (e.g., `$closure(...)`).
pub(super) fn emit_closure_call(
    var: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    closure::emit_closure_call(var, args, emitter, ctx, data)
}

/// Emits an indirect call where the callee is a runtime-loaded expression.
pub(super) fn emit_loaded_expr_call(
    callee: &Expr,
    args: &[Expr],
    loaded_callee_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    indirect::emit_loaded_expr_call(callee, args, loaded_callee_ty, emitter, ctx, data)
}

/// Emits a call where the already-loaded callee result is a runtime string callback name.
pub(super) fn emit_loaded_runtime_string_call(
    args: &[Expr],
    span: Span,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("call runtime string callable");
    let save_concat_before_args =
        emitter.target.arch == crate::codegen::platform::Arch::X86_64;
    if save_concat_before_args {
        super::save_concat_offset_before_nested_call(emitter, ctx);
    }

    let (ptr_reg, len_reg) = crate::codegen::abi::string_result_regs(emitter);
    crate::codegen::abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);         // preserve the runtime string callback name while building descriptor arguments
    let arr_ty = descriptor_invoker_args::emit_descriptor_invoker_arg_array(
        args,
        None,
        span,
        emitter,
        ctx,
        data,
    );
    let call_reg = crate::codegen::abi::nested_call_reg(emitter);
    let ret_ty =
        crate::codegen::builtins::arrays::call_user_func_array::emit_loaded_array_string_callback_call(
            crate::codegen::builtins::arrays::call_user_func_array::LoadedArraySource::Result,
            &arr_ty,
            0,
            8,
            call_reg,
            save_concat_before_args,
            emitter,
            ctx,
            data,
        );
    crate::codegen::abi::emit_release_temporary_stack(emitter, 16);             // discard the preserved runtime string callback name
    ret_ty
}

/// Emits `([$object, "method"])(...)` or `([ClassName::class, "method"])(...)`.
pub(super) fn emit_callable_array_literal_call(
    callee: &Expr,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    if let Some((receiver, method)) = callable_array_parts(callee) {
        if let Some(receiver) = static_callable_receiver(receiver, ctx) {
            return emit_static_callable_array_descriptor_call(
                &receiver,
                method,
                args,
                emitter,
                ctx,
                data,
            );
        }
        if let Some(ret_ty) =
            emit_instance_callable_array_descriptor_call(receiver, method, args, emitter, ctx, data)
        {
            return Some(ret_ty);
        }
    }
    callable_array_runtime::emit_literal_call(callee, args, emitter, ctx, data)
}

/// Emits a runtime-selected callable-array invocation for builtin callback paths.
pub(crate) fn emit_runtime_callable_array_call(
    callee: &Expr,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    if let ExprKind::Variable(var) = &callee.kind {
        if let Some(ret_ty) =
            callable_array_runtime::emit_variable_call(var, args, emitter, ctx, data)
        {
            return Some(ret_ty);
        }
    }
    callable_array_runtime::emit_literal_call(callee, args, emitter, ctx, data)
}

/// Emits a direct `$callback(...)` call when `$callback` stores a PHP callable array.
pub(super) fn emit_callable_array_variable_call(
    var: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let Some(target) = ctx.callable_array_targets.get(var).cloned() else {
        return callable_array_runtime::emit_variable_call(var, args, emitter, ctx, data);
    };
    match target {
        CallableTarget::Method { object, method } => emit_instance_callable_array_variable_call(
            var, &object, &method, args, emitter, ctx, data,
        ),
        CallableTarget::StaticMethod { receiver, method } => {
            emit_static_callable_array_variable_call(&receiver, &method, args, emitter, ctx, data)
        }
        CallableTarget::Function(_) => None,
    }
}

/// Emits a descriptor invocation for a local object variable with public `__invoke`.
pub(super) fn emit_invokable_object_variable_call(
    var: &str,
    class_name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let case = crate::codegen::callable_dispatch::runtime_instance_method_case(
        ctx,
        data,
        class_name,
        "__invoke",
        crate::codegen::callable_dispatch::RuntimeInstanceCallableShape::ObjectInvoke,
    )?;
    if !case.has_invoker {
        return None;
    }
    let mut descriptor_args = Vec::with_capacity(args.len() + 1);
    descriptor_args.push(Expr::new(
        ExprKind::Variable(var.to_string()),
        Span::dummy(),
    ));
    descriptor_args.extend(args.iter().cloned());
    emit_callable_array_descriptor_case_call(
        &case.descriptor_label,
        &case.sig,
        &descriptor_args,
        emitter,
        ctx,
        data,
    )
}

/// Emits a descriptor invocation for a stored instance-method callable array.
fn emit_instance_callable_array_variable_call(
    var: &str,
    object: &Expr,
    method: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let receiver_ty = crate::codegen::functions::infer_contextual_type(object, ctx);
    let class_name = crate::codegen::functions::singular_object_class(&receiver_ty)?;
    let case = crate::codegen::callable_dispatch::runtime_instance_method_case(
        ctx,
        data,
        class_name,
        method,
        crate::codegen::callable_dispatch::RuntimeInstanceCallableShape::InstanceMethod,
    )?;
    if !case.has_invoker {
        return None;
    }
    let receiver = callable_array_receiver_slot_expr(var);
    let mut descriptor_args = Vec::with_capacity(args.len() + 1);
    descriptor_args.push(receiver);
    descriptor_args.extend(args.iter().cloned());
    emit_callable_array_descriptor_case_call(
        &case.descriptor_label,
        &case.sig,
        &descriptor_args,
        emitter,
        ctx,
        data,
    )
}

/// Emits a descriptor invocation for a literal instance-method callable array.
fn emit_instance_callable_array_descriptor_call(
    receiver: &Expr,
    method: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let receiver_ty = crate::codegen::functions::infer_contextual_type(receiver, ctx);
    let class_name = crate::codegen::functions::singular_object_class(&receiver_ty)?;
    let case = crate::codegen::callable_dispatch::runtime_instance_method_case(
        ctx,
        data,
        class_name,
        method,
        crate::codegen::callable_dispatch::RuntimeInstanceCallableShape::InstanceMethod,
    )?;
    if !case.has_invoker {
        return None;
    }
    let mut descriptor_args = Vec::with_capacity(args.len() + 1);
    descriptor_args.push(receiver.clone());
    descriptor_args.extend(args.iter().cloned());
    emit_callable_array_descriptor_case_call(
        &case.descriptor_label,
        &case.sig,
        &descriptor_args,
        emitter,
        ctx,
        data,
    )
}

/// Emits a descriptor invocation for a stored static-method callable array.
fn emit_static_callable_array_variable_call(
    receiver: &StaticReceiver,
    method: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emit_static_callable_array_descriptor_call(receiver, method, args, emitter, ctx, data)
}

/// Emits a descriptor invocation for a static-method callable array.
fn emit_static_callable_array_descriptor_call(
    receiver: &StaticReceiver,
    method: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let StaticReceiver::Named(class_name) = receiver else {
        return None;
    };
    let case = crate::codegen::callable_dispatch::runtime_static_method_case(
        ctx,
        data,
        class_name.as_str(),
        method,
    )?;
    if !case.has_invoker {
        return None;
    }
    emit_callable_array_descriptor_case_call(
        &case.descriptor_label,
        &case.sig,
        args,
        emitter,
        ctx,
        data,
    )
}

/// Calls a callable-array descriptor case with direct callable-array arguments.
fn emit_callable_array_descriptor_case_call(
    descriptor_label: &str,
    sig: &crate::types::FunctionSig,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("call callable-array descriptor");
    let save_concat_before_args =
        emitter.target.arch == crate::codegen::platform::Arch::X86_64;
    if save_concat_before_args {
        super::save_concat_offset_before_nested_call(emitter, ctx);
    }
    let arr_ty = descriptor_invoker_args::emit_descriptor_invoker_arg_array(
        args,
        Some(sig),
        Span::dummy(),
        emitter,
        ctx,
        data,
    );
    let call_reg = crate::codegen::abi::nested_call_reg(emitter);
    crate::codegen::abi::emit_symbol_address(emitter, call_reg, descriptor_label);
    crate::codegen::builtins::arrays::call_user_func_array::emit_call_descriptor_array_invoker(
        crate::codegen::builtins::arrays::call_user_func_array::LoadedArraySource::Result,
        &arr_ty,
        call_reg,
        save_concat_before_args,
        emitter,
        ctx,
        data,
    );
    Some(PhpType::Mixed)
}

/// Builds `$callback[0]`, the receiver slot stored inside a callable-array value.
fn callable_array_receiver_slot_expr(var: &str) -> Expr {
    callable_array_slot_expr(var, 0)
}

/// Builds `$callback[$index]`, a positional slot stored inside a callable-array value.
fn callable_array_slot_expr(var: &str, index: i64) -> Expr {
    Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(Expr::new(ExprKind::Variable(var.to_string()), Span::dummy())),
            index: Box::new(Expr::new(ExprKind::IntLiteral(index), Span::dummy())),
        },
        Span::dummy(),
    )
}

/// Returns receiver and method from a two-element PHP callable array literal.
fn callable_array_parts(callee: &Expr) -> Option<(&Expr, &str)> {
    let elems = match &callee.kind {
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

/// Resolves a callable-array receiver expression to a static class receiver.
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

/// Resolves `self`, `parent`, `static`, and named static receivers to concrete class names.
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

/// Resolves a class name case-insensitively against the known codegen class table.
fn resolve_class_name<'a>(ctx: &'a Context, class_name: &str) -> Option<&'a str> {
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    ctx.classes
        .keys()
        .find(|existing| php_symbol_key(existing) == class_key)
        .map(String::as_str)
}

/// Emits a first-class callable expression (e.g., `$fn(...)()`).
pub(super) fn emit_first_class_callable(
    target: &crate::parser::ast::CallableTarget,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    first_class::emit_first_class_callable(target, emitter, ctx, data)
}

/// Returns the function signature for a first-class callable target.
pub(crate) fn first_class_callable_sig(
    target: &crate::parser::ast::CallableTarget,
    ctx: &Context,
) -> Option<crate::types::FunctionSig> {
    first_class::first_class_callable_sig(target, ctx)
}

/// Generates a unique temp name for the receiver of an inline first-class callable.
pub(crate) fn first_class_method_receiver_temp_name(span: Span) -> String {
    first_class::method_receiver_temp_name(span)
}

/// Generates a unique temp name for the pipe value in an arrow-function pipeline.
pub(crate) fn pipe_value_temp_name(span: Span) -> String {
    format!("__elephc_pipe_value_{}_{}", span.line, span.col)
}

/// Emits a pipe expression (first-class callable pipeline).
pub(super) fn emit_pipe(
    value: &Expr,
    callable: &Expr,
    span: Span,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    pipe::emit_pipe(value, callable, span, emitter, ctx, data)
}
