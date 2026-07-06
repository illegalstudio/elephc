//! Purpose:
//! Materializes PHP callable shapes passed to `new Fiber(...)` as runtime callable descriptors.
//! Keeps Fiber constructor lowering focused on object allocation while callable selection stays here.
//!
//! Called from:
//! - `crate::codegen_support::expr::objects::allocation`
//!
//! Key details:
//! - The Fiber object stores one callable descriptor pointer and a generated wrapper pointer.
//! - Raw string callbacks use runtime descriptor-name dispatch; receiver-bound shapes are
//!   converted to first-class-callable descriptors so receiver environments live in captures.

use crate::codegen_support::callable_descriptor::{
    self, CallableDescriptorInvocation, CallableDescriptorShape,
};
use crate::codegen_support::callable_dispatch::{RuntimeCallableCase, RuntimeCallableSelector};
use crate::codegen_support::context::{Context, DeferredClosure, HeapOwnership};
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::{abi, callable_dispatch};
use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{CallableTarget, Expr, ExprKind, StaticReceiver, Stmt, StmtKind};
use crate::span::Span;
use crate::types::{callable_wrapper_sig, FunctionSig, PhpType};

const FIBER_RECEIVER_CAPTURE_PARAM: &str = "__elephc_fiber_callable_receiver";

/// Emits a Fiber callback descriptor and returns the wrapper label that can invoke it.
///
/// Existing descriptor-valued expressions delegate to the ordinary Fiber wrapper planner.
/// Raw string callbacks and callable arrays are materialized into descriptor pointers first,
/// then use the generic descriptor-invoker Fiber wrapper.
pub(super) fn emit_fiber_callable_descriptor(
    callable_expr: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<String> {
    if emit_callable_array_descriptor(callable_expr, emitter, ctx, data)
        || emit_invokable_object_descriptor(callable_expr, emitter, ctx, data)
        || emit_string_callable_descriptor(callable_expr, emitter, ctx, data)
    {
        return Some(super::fiber_wrapper::prepare_descriptor_invoker_wrapper(ctx));
    }

    crate::codegen_support::expr::emit_expr(callable_expr, emitter, ctx, data);
    super::fiber_wrapper::prepare_fiber_wrapper(callable_expr, ctx)
}

/// Emits a first-class-callable descriptor for a callable-array Fiber callback.
fn emit_callable_array_descriptor(
    callable_expr: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> bool {
    if let Some(target) = callable_array_literal_target(callable_expr, ctx) {
        return emit_callable_array_target_descriptor(
            target,
            callable_expr,
            emitter,
            ctx,
            data,
        );
    }

    if let ExprKind::Variable(var_name) = &callable_expr.kind {
        if let Some(target) = ctx.callable_array_targets.get(var_name).cloned() {
            return match target {
                CallableTarget::StaticMethod { .. } => {
                    emit_synthetic_first_class_callable(target, callable_expr, emitter, ctx, data)
                }
                CallableTarget::Method { object, method } => {
                    emit_stored_instance_callable_array_descriptor(
                        var_name,
                        &object,
                        &method,
                        emitter,
                        ctx,
                        data,
                    )
                }
                CallableTarget::Function(_) => false,
            };
        }
    }

    emit_runtime_callable_array_descriptor(callable_expr, emitter, ctx, data)
}

/// Emits a first-class-callable descriptor for an object with public `__invoke()`.
fn emit_invokable_object_descriptor(
    callable_expr: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> bool {
    let callable_ty = crate::codegen_support::functions::infer_contextual_type(callable_expr, ctx);
    let Some(class_name) = crate::codegen_support::functions::singular_object_class(&callable_ty) else {
        return false;
    };
    if !ctx
        .classes
        .get(class_name)
        .is_some_and(|class_info| class_info.methods.contains_key("__invoke"))
    {
        return false;
    }

    emit_instance_receiver_expr_descriptor(
        callable_expr,
        "__invoke",
        callable_expr.span,
        emitter,
        ctx,
        data,
    )
}

/// Emits runtime string-name descriptor selection for a Fiber callback.
fn emit_string_callable_descriptor(
    callable_expr: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> bool {
    let callable_ty = crate::codegen_support::functions::infer_contextual_type(callable_expr, ctx);
    if !matches!(callable_ty.codegen_repr(), PhpType::Str) {
        return false;
    }

    crate::codegen_support::expr::emit_expr(callable_expr, emitter, ctx, data);
    emit_select_loaded_string_descriptor(emitter, ctx, data);
    true
}

/// Emits a synthetic first-class callable expression and leaves its descriptor in the result register.
fn emit_synthetic_first_class_callable(
    target: CallableTarget,
    source_expr: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> bool {
    let fcc_expr = Expr::new(ExprKind::FirstClassCallable(target), source_expr.span);
    crate::codegen_support::expr::emit_expr(&fcc_expr, emitter, ctx, data);
    true
}

/// Emits a callable-array target as a descriptor suitable for Fiber storage.
fn emit_callable_array_target_descriptor(
    target: CallableTarget,
    source_expr: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> bool {
    match target {
        CallableTarget::StaticMethod { .. } => {
            emit_synthetic_first_class_callable(target, source_expr, emitter, ctx, data)
        }
        CallableTarget::Method { object, method } => emit_instance_receiver_expr_descriptor(
            &object,
            &method,
            source_expr.span,
            emitter,
            ctx,
            data,
        ),
        CallableTarget::Function(_) => false,
    }
}

/// Emits a descriptor whose receiver capture comes from slot zero of a stored callable array.
#[allow(clippy::too_many_arguments)]
fn emit_stored_instance_callable_array_descriptor(
    var_name: &str,
    object: &Expr,
    method: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> bool {
    let receiver_ty = crate::codegen_support::functions::infer_contextual_type(object, ctx);
    let Some(class_name) =
        crate::codegen_support::functions::singular_object_class(&receiver_ty).map(str::to_string)
    else {
        return false;
    };
    let Some((resolved_method, sig)) = callable_array_method_wrapper_sig(ctx, &class_name, method)
    else {
        return false;
    };

    let capture_ty = PhpType::Object(class_name.clone());
    let descriptor_label = receiver_bound_instance_method_descriptor(
        class_name,
        resolved_method,
        &sig,
        data,
        ctx,
    );

    emit_runtime_descriptor_with_callable_array_receiver(
        var_name,
        object.span,
        &descriptor_label,
        &capture_ty,
        emitter,
        ctx,
        data,
    );
    true
}

/// Emits a receiver-bound instance method descriptor by evaluating the receiver expression once.
fn emit_instance_receiver_expr_descriptor(
    receiver: &Expr,
    method: &str,
    span: Span,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> bool {
    let receiver_ty = crate::codegen_support::functions::infer_contextual_type(receiver, ctx);
    let Some(class_name) =
        crate::codegen_support::functions::singular_object_class(&receiver_ty).map(str::to_string)
    else {
        return false;
    };
    let Some((resolved_method, sig)) = callable_array_method_wrapper_sig(ctx, &class_name, method)
    else {
        return false;
    };

    let capture_ty = PhpType::Object(class_name.clone());
    let descriptor_label = receiver_bound_instance_method_descriptor(
        class_name,
        resolved_method,
        &sig,
        data,
        ctx,
    );
    emit_runtime_descriptor_with_receiver_expr(
        receiver,
        span,
        &descriptor_label,
        &capture_ty,
        emitter,
        ctx,
        data,
    );
    true
}

/// Resolves the visible wrapper signature for an instance-method callable array.
fn callable_array_method_wrapper_sig(
    ctx: &Context,
    class_name: &str,
    method: &str,
) -> Option<(String, FunctionSig)> {
    let class_info = ctx.classes.get(class_name)?;
    let method_key = php_symbol_key(method);
    let (resolved_method, method_sig) = class_info
        .methods
        .iter()
        .find(|(candidate, _)| php_symbol_key(candidate) == method_key)?;
    Some((resolved_method.clone(), callable_wrapper_sig(method_sig)))
}

/// Registers a receiver-bound wrapper and returns its static descriptor label.
fn receiver_bound_instance_method_descriptor(
    class_name: String,
    resolved_method: String,
    sig: &FunctionSig,
    data: &mut DataSection,
    ctx: &mut Context,
) -> String {
    let hidden_name = unique_hidden_param(FIBER_RECEIVER_CAPTURE_PARAM, sig);
    let capture_ty = PhpType::Object(class_name.clone());
    let captures = vec![(hidden_name.clone(), capture_ty.clone(), false)];
    let hidden_params = vec![(hidden_name.clone(), capture_ty, false)];
    let wrapper_label = ctx.next_label("fiber_callable_array_method");
    let param_names: Vec<String> = sig.params.iter().map(|(name, _)| name.clone()).collect();
    ctx.deferred_closures.push(DeferredClosure {
        label: wrapper_label.clone(),
        params: param_names,
        body: callable_array_method_wrapper_body(&hidden_name, &resolved_method, sig),
        sig: sig.clone(),
        captures: captures.clone(),
        hidden_params: hidden_params.clone(),
        current_class: Some(class_name.clone()),
        needed: true,
    });

    let invoker_label =
        callable_dispatch::ensure_runtime_descriptor_invoker(ctx, &hidden_params, sig);
    callable_descriptor::static_descriptor_with_optional_invoker_meta(
        data,
        &wrapper_label,
        None,
        callable_descriptor::CALLABLE_DESC_KIND_FIRST_CLASS,
        Some(sig),
        &captures,
        &hidden_params,
        CallableDescriptorInvocation::method(
            CallableDescriptorShape::InstanceMethod,
            Some(class_name),
            resolved_method,
        ),
        invoker_label.as_deref(),
    )
}

/// Builds the synthetic wrapper body for a Fiber callable-array instance method.
fn callable_array_method_wrapper_body(
    receiver_param: &str,
    method: &str,
    sig: &FunctionSig,
) -> Vec<Stmt> {
    let last_param_idx = sig.params.len().saturating_sub(1);
    let args: Vec<Expr> = sig
        .params
        .iter()
        .enumerate()
        .map(|(idx, (name, _))| {
            let var_expr = Expr::new(ExprKind::Variable(name.clone()), Span::dummy());
            if sig.variadic.is_some() && idx == last_param_idx {
                Expr::new(ExprKind::Spread(Box::new(var_expr)), Span::dummy())
            } else {
                var_expr
            }
        })
        .collect();
    let call_expr = Expr::new(
        ExprKind::MethodCall {
            object: Box::new(Expr::new(
                ExprKind::Variable(receiver_param.to_string()),
                Span::dummy(),
            )),
            method: method.to_string(),
            args,
        },
        Span::dummy(),
    );

    if sig.return_type == PhpType::Void {
        vec![
            Stmt::new(StmtKind::ExprStmt(call_expr), Span::dummy()),
            Stmt::new(StmtKind::Return(None), Span::dummy()),
        ]
    } else {
        vec![Stmt::new(StmtKind::Return(Some(call_expr)), Span::dummy())]
    }
}

/// Builds a runtime descriptor and stores the current callable-array receiver as capture slot zero.
#[allow(clippy::too_many_arguments)]
fn emit_runtime_descriptor_with_callable_array_receiver(
    var_name: &str,
    span: Span,
    descriptor_label: &str,
    receiver_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let descriptor_reg = abi::nested_call_reg(emitter);
    let total_bytes = callable_descriptor::CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET + 16;

    emitter.comment("fiber callable-array descriptor capture");
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), total_bytes as i64);
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    emitter.instruction(&format!("mov {}, {}", descriptor_reg, abi::int_result_reg(emitter))); // keep the Fiber descriptor pointer while copying its static header
    callable_descriptor::emit_copy_static_descriptor_to_runtime(
        emitter,
        descriptor_reg,
        descriptor_label,
    );
    abi::emit_push_reg(emitter, descriptor_reg);                                // preserve the runtime descriptor while the receiver slot is loaded

    let receiver = callable_array_slot_expr(var_name, 0, span);
    let emitted_ty = crate::codegen_support::expr::emit_expr(&receiver, emitter, ctx, data);
    if matches!(emitted_ty.codegen_repr(), PhpType::Mixed) {
        crate::codegen_support::expr::objects::emit_unbox_mixed_object_or_fatal(
            b"Fatal error: Fiber callable array receiver is not an object\n",
            emitter,
            ctx,
            data,
        );
    }
    if receiver_ty.is_refcounted()
        && crate::codegen_support::expr::expr_result_heap_ownership(&receiver) != HeapOwnership::Owned
    {
        abi::emit_incref_if_refcounted(emitter, receiver_ty);
    }
    abi::emit_pop_reg(emitter, descriptor_reg);                                 // restore the runtime descriptor after receiver capture loading
    callable_descriptor::emit_store_current_result_to_runtime_capture(
        emitter,
        descriptor_reg,
        0,
        receiver_ty,
    );
    if descriptor_reg != abi::int_result_reg(emitter) {
        emitter.instruction(&format!("mov {}, {}", abi::int_result_reg(emitter), descriptor_reg)); // return the receiver-bound Fiber callable descriptor
    }
}

/// Evaluates a receiver expression and stores it as runtime descriptor capture slot zero.
#[allow(clippy::too_many_arguments)]
fn emit_runtime_descriptor_with_receiver_expr(
    receiver: &Expr,
    span: Span,
    descriptor_label: &str,
    receiver_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let emitted_ty = crate::codegen_support::expr::emit_expr(receiver, emitter, ctx, data);
    if matches!(emitted_ty.codegen_repr(), PhpType::Mixed) {
        crate::codegen_support::expr::objects::emit_unbox_mixed_object_or_fatal(
            b"Fatal error: Fiber callable receiver is not an object\n",
            emitter,
            ctx,
            data,
        );
    }
    if receiver_ty.is_refcounted()
        && crate::codegen_support::expr::expr_result_heap_ownership(receiver) != HeapOwnership::Owned
    {
        abi::emit_incref_if_refcounted(emitter, receiver_ty);
    }
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the evaluated Fiber callable receiver while allocating its descriptor
    emit_runtime_descriptor_with_saved_receiver(span, descriptor_label, receiver_ty, emitter, data);
}

/// Stores the receiver currently saved on the temporary stack in descriptor capture slot zero.
fn emit_runtime_descriptor_with_saved_receiver(
    _span: Span,
    descriptor_label: &str,
    receiver_ty: &PhpType,
    emitter: &mut Emitter,
    _data: &mut DataSection,
) {
    let descriptor_reg = abi::nested_call_reg(emitter);
    let total_bytes = callable_descriptor::CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET + 16;

    emitter.comment("fiber receiver-bound descriptor capture");
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), total_bytes as i64);
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    emitter.instruction(&format!("mov {}, {}", descriptor_reg, abi::int_result_reg(emitter))); // keep the Fiber descriptor pointer while copying its static header
    callable_descriptor::emit_copy_static_descriptor_to_runtime(
        emitter,
        descriptor_reg,
        descriptor_label,
    );
    abi::emit_push_reg(emitter, descriptor_reg);                                // preserve the runtime descriptor while the receiver capture is restored
    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 16);
    callable_descriptor::emit_store_current_result_to_runtime_capture(
        emitter,
        descriptor_reg,
        0,
        receiver_ty,
    );
    abi::emit_pop_reg(emitter, descriptor_reg);                                 // restore the runtime descriptor after receiver capture storage
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the saved receiver after it has been copied into the descriptor
    if descriptor_reg != abi::int_result_reg(emitter) {
        emitter.instruction(&format!("mov {}, {}", abi::int_result_reg(emitter), descriptor_reg)); // return the receiver-bound Fiber callable descriptor
    }
}

/// Emits descriptor selection for runtime callable-array Fiber callbacks.
fn emit_runtime_callable_array_descriptor(
    callable_expr: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> bool {
    if crate::codegen_support::builtins::arrays::runtime_callable_array_callback::emit_without_saved_array(
        callable_expr,
        emitter,
        ctx,
        data,
        |case, receiver_ty, emitter, ctx, data| {
            emit_selected_runtime_callable_array_descriptor(
                case,
                receiver_ty,
                callable_expr.span,
                emitter,
                ctx,
                data,
            );
        },
    ) {
        return true;
    }

    crate::codegen_support::builtins::arrays::runtime_callable_array_callback::emit_literal_without_saved_array(
        callable_expr,
        emitter,
        ctx,
        data,
        |case, receiver_ty, emitter, ctx, data| {
            emit_selected_runtime_callable_array_descriptor(
                case,
                receiver_ty,
                callable_expr.span,
                emitter,
                ctx,
                data,
            );
        },
    )
}

/// Materializes the descriptor selected from a runtime callable array.
fn emit_selected_runtime_callable_array_descriptor(
    case: &RuntimeCallableCase,
    receiver_ty: Option<&PhpType>,
    span: Span,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let Some(receiver_ty) = receiver_ty else {
        abi::emit_symbol_address(emitter, abi::int_result_reg(emitter), &case.descriptor_label);
        return;
    };

    let Some((class_name, method_name)) = runtime_case_method_target(case) else {
        emit_fiber_callable_no_match_abort(emitter, data);
        return;
    };
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                  // recover the selected runtime callable-array receiver
    if receiver_ty.is_refcounted() {
        abi::emit_incref_if_refcounted(emitter, receiver_ty);
    }
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the retained receiver while building its descriptor
    let Some((resolved_method, sig)) = callable_array_method_wrapper_sig(ctx, &class_name, &method_name)
    else {
        emit_fiber_callable_no_match_abort(emitter, data);
        return;
    };
    let descriptor_label = receiver_bound_instance_method_descriptor(
        class_name,
        resolved_method,
        &sig,
        data,
        ctx,
    );
    emit_runtime_descriptor_with_saved_receiver(span, &descriptor_label, receiver_ty, emitter, data);
}

/// Extracts the class and method names from a runtime callable case.
fn runtime_case_method_target(case: &RuntimeCallableCase) -> Option<(String, String)> {
    let php_name = case.php_name.as_ref()?;
    let (class_name, method_name) = php_name.split_once("::")?;
    Some((class_name.to_string(), method_name.to_string()))
}

/// Builds `$callback[$index]` for reading a stored callable-array slot.
fn callable_array_slot_expr(var_name: &str, index: i64, span: Span) -> Expr {
    Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(Expr::new(ExprKind::Variable(var_name.to_string()), span)),
            index: Box::new(Expr::new(ExprKind::IntLiteral(index), span)),
        },
        span,
    )
}

/// Returns a hidden receiver parameter name that cannot collide with visible callback params.
fn unique_hidden_param(base: &str, sig: &FunctionSig) -> String {
    if !sig.params.iter().any(|(name, _)| name == base) {
        return base.to_string();
    }
    let mut idx = 0usize;
    loop {
        let candidate = format!("{}_{}", base, idx);
        if !sig.params.iter().any(|(name, _)| name == &candidate) {
            return candidate;
        }
        idx += 1;
    }
}

/// Selects a callable descriptor for the currently loaded string callback name.
fn emit_select_loaded_string_descriptor(
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.comment("fiber callable string descriptor selection");
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);                         // preserve the Fiber string callback name during descriptor selection

    let cases = callable_dispatch::runtime_callable_cases(ctx, data, &[], None);
    let call_reg = abi::nested_call_reg(emitter);
    let done_label = ctx.next_label("fiber_string_callable_done");
    for case in cases.iter().filter(|case| case.has_invoker) {
        emit_string_case_selection(case, call_reg, &done_label, emitter, ctx, data);
    }
    emit_fiber_callable_no_match_abort(emitter, data);
    emitter.label(&done_label);
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the saved Fiber string callback name after descriptor selection
}

/// Emits one runtime string descriptor case for `new Fiber($callback)`.
fn emit_string_case_selection(
    case: &RuntimeCallableCase,
    call_reg: &str,
    done_label: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let next_case = ctx.next_label("fiber_string_callable_next");
    let selector = RuntimeCallableSelector::StringNameStack {
        ptr_offset: 0,
        len_offset: 8,
        call_reg,
    };
    callable_dispatch::emit_branch_if_callable_case_mismatch(
        &selector, case, &next_case, emitter, ctx, data,
    );
    let result_reg = abi::int_result_reg(emitter);
    if call_reg != result_reg {
        emitter.instruction(&format!("mov {}, {}", result_reg, call_reg));      // return the selected Fiber callable descriptor
    }
    abi::emit_jump(emitter, done_label);
    emitter.label(&next_case);
}

/// Returns a callable target for a two-slot literal callable array supported by Fiber.
fn callable_array_literal_target(expr: &Expr, ctx: &Context) -> Option<CallableTarget> {
    let (receiver, method) = callable_array_parts(expr)?;
    if let Some(receiver) = static_callable_receiver(receiver, ctx) {
        return Some(CallableTarget::StaticMethod {
            receiver,
            method: method.to_string(),
        });
    }
    Some(CallableTarget::Method {
        object: Box::new(receiver.clone()),
        method: method.to_string(),
    })
}

/// Returns receiver and method from `[receiver, "method"]`.
fn callable_array_parts(expr: &Expr) -> Option<(&Expr, &str)> {
    let ExprKind::ArrayLiteral(elems) = &expr.kind else {
        return None;
    };
    if elems.len() != 2 {
        return None;
    }
    let ExprKind::StringLiteral(method) = &elems[1].kind else {
        return None;
    };
    Some((&elems[0], method.as_str()))
}

/// Resolves a literal callable-array receiver to a static class target.
fn static_callable_receiver(receiver: &Expr, ctx: &Context) -> Option<StaticReceiver> {
    let class_name = match &receiver.kind {
        ExprKind::StringLiteral(class_name) => resolve_class_name(ctx, class_name)?.to_string(),
        ExprKind::ClassConstant { receiver } => resolve_static_receiver_class(receiver, ctx)?,
        _ => return None,
    };
    Some(StaticReceiver::Named(Name::from(class_name)))
}

/// Resolves a scoped receiver to a concrete class name.
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

/// Resolves a class name case-insensitively against known codegen classes.
fn resolve_class_name<'a>(ctx: &'a Context, class_name: &str) -> Option<&'a str> {
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    ctx.classes
        .keys()
        .find(|existing| php_symbol_key(existing) == class_key)
        .map(String::as_str)
}

/// Emits a fatal diagnostic when a runtime Fiber callable name has no descriptor case.
fn emit_fiber_callable_no_match_abort(emitter: &mut Emitter, data: &mut DataSection) {
    let (message_label, message_len) = data.add_string(
        b"Fatal error: Fiber callback string did not resolve to an invokable target\n",
    );
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // write the Fiber callable diagnostic to stderr
            abi::emit_symbol_address(emitter, "x1", &message_label);
            emitter.instruction(&format!("mov x2, #{}", message_len));          // pass the Fiber callable diagnostic byte length to write()
            emitter.syscall(4);
            abi::emit_exit(emitter, 1);
        }
        Arch::X86_64 => {
            emitter.instruction("mov edi, 2");                                  // write the Fiber callable diagnostic to stderr
            abi::emit_symbol_address(emitter, "rsi", &message_label);
            emitter.instruction(&format!("mov edx, {}", message_len));          // pass the Fiber callable diagnostic byte length to write()
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // emit the Fiber callable diagnostic
            abi::emit_exit(emitter, 1);
        }
    }
}
