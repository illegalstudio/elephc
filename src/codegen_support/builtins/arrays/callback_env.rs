//! Purpose:
//! Builds callback capture environments used by array and dynamic-call builtins.
//! Owns hidden capture materialization and deferred wrapper metadata for emitted callbacks.
//!
//! Called from:
//! - Array callback builtins such as `array_map()`, `array_filter()`, `array_reduce()`, and sort/walk helpers.
//! - Dynamic-call builtins such as `call_user_func()` and `call_user_func_array()`.
//!
//! Key details:
//! - Capture slots must preserve source-call evaluation order and ABI argument layout for wrapper calls.
//! - Descriptor-valued callbacks keep receiver and capture environments in descriptor storage.

use crate::codegen_support::abi;
use crate::codegen_support::context::{Context, DeferredCallbackWrapper, HeapOwnership};
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::{emit_expr, expr_result_heap_ownership};
use crate::codegen_support::platform::Arch;
use crate::names::{function_symbol, php_symbol_key};
use crate::parser::ast::{CallableTarget, Expr, ExprKind, StaticReceiver};
use crate::span::Span;
use crate::types::{FunctionSig, PhpType};

use super::super::callable_lookup::{lookup_function, FunctionLookup};

/// Metadata for a deferred callback wrapper emitted after the main function body.
/// Holds the environment layout so the wrapper can reload captures and forward the call.
pub(crate) struct CallbackEnv {
    pub(crate) wrapper_label: String,
    pub(crate) env_bytes: usize,
    pub(crate) array_slot_offset: usize,
}

/// Metadata for a descriptor-backed callback wrapper environment.
pub(crate) struct DescriptorCallbackEnv {
    pub(crate) wrapper_label: String,
    pub(crate) env_bytes: usize,
    pub(crate) array_slot_offset: usize,
}

/// Metadata for a callable-array target that can be invoked through a descriptor callback wrapper.
pub(crate) struct CallableArrayDescriptorCallback {
    pub(crate) descriptor_label: String,
    pub(crate) sig: FunctionSig,
    pub(crate) receiver_prefix: Option<(Expr, PhpType)>,
}

/// Resolves a callback expression and emits code to load its address into `call_reg`.
///
/// Handles string literals, callable variables, and evaluated callback expressions.
/// Returns the list of captured variables with their types and by-ref flags.
pub(crate) fn materialize_callback_address(
    callback: &Expr,
    call_reg: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Vec<(String, PhpType, bool)> {
    match &callback.kind {
        ExprKind::StringLiteral(name) => {
            let resolved_name = match lookup_function(ctx, name) {
                Some(FunctionLookup::UserFunction(name))
                | Some(FunctionLookup::IncludeVariant(name)) => name,
                _ => name.clone(),
            };
            let label = function_symbol(&resolved_name);
            abi::emit_symbol_address(emitter, call_reg, &label);
            Vec::new()
        }
        ExprKind::Variable(name) => {
            let var = ctx.variables.get(name).expect("undefined callback variable");
            abi::load_at_offset(emitter, call_reg, var.stack_offset);           // load the callback descriptor from the callable variable slot
            if ctx.ref_params.contains(name) {
                abi::emit_load_from_address(emitter, call_reg, call_reg, 0);
            }
            crate::codegen_support::callable_descriptor::emit_load_entry_from_descriptor(
                emitter,
                call_reg,
                call_reg,
            );
            crate::codegen_support::callables::callable_captures(callback, ctx)
        }
        _ => {
            emit_expr(callback, emitter, ctx, data);
            let result_reg = abi::int_result_reg(emitter);
            emitter.instruction(&format!("mov {}, {}", call_reg, result_reg));  // keep the evaluated callback descriptor in the nested-call scratch register
            crate::codegen_support::callable_descriptor::emit_load_entry_from_descriptor(
                emitter,
                call_reg,
                call_reg,
            );
            crate::codegen_support::callables::callable_captures(callback, ctx)
        }
    }
}

/// Resolves a local callable-array callback to a descriptor plus optional receiver prefix.
pub(crate) fn resolve_callable_array_descriptor_callback(
    callback: &Expr,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<CallableArrayDescriptorCallback> {
    let ExprKind::Variable(var_name) = &callback.kind else {
        return None;
    };
    let target = ctx.callable_array_targets.get(var_name).cloned()?;
    match target {
        CallableTarget::StaticMethod { receiver, method } => {
            let class_name = resolve_static_receiver_class(&receiver, ctx)?;
            let case =
                crate::codegen_support::callable_dispatch::runtime_static_method_case(ctx, data, &class_name, &method)?;
            Some(CallableArrayDescriptorCallback {
                descriptor_label: case.descriptor_label,
                sig: case.sig,
                receiver_prefix: None,
            })
        }
        CallableTarget::Method { object, method } => {
            let receiver = callable_array_slot_expr(var_name, 0);
            let receiver_ty =
                crate::codegen_support::functions::infer_contextual_type(&receiver, ctx).codegen_repr();
            let object_ty = crate::codegen_support::functions::infer_contextual_type(&object, ctx);
            let class_name =
                crate::codegen_support::functions::singular_object_class(&object_ty)?.to_string();
            let case = crate::codegen_support::callable_dispatch::runtime_instance_method_case(
                ctx,
                data,
                &class_name,
                &method,
                crate::codegen_support::callable_dispatch::RuntimeInstanceCallableShape::InstanceMethod,
            )?;
            Some(CallableArrayDescriptorCallback {
                descriptor_label: case.descriptor_label,
                sig: case.sig,
                receiver_prefix: Some((receiver, receiver_ty)),
            })
        }
        CallableTarget::Function(_) => None,
    }
}

/// Emits code to push each captured variable as a hidden argument before a deferred wrapper call.
///
/// For by-ref captures, emits the variable's address; for value captures, loads the value from
/// the stack slot and pushes it. Appends corresponding types to `arg_types`.
pub(crate) fn push_captures_as_hidden_args(
    captures: &[(String, PhpType, bool)],
    emitter: &mut Emitter,
    ctx: &Context,
    arg_types: &mut Vec<PhpType>,
) {
    if let Some(descriptor_offset) = ctx.runtime_capture_descriptor_offset {
        push_descriptor_captures_as_hidden_args(captures, descriptor_offset, emitter, arg_types);
        return;
    }

    for (capture_name, capture_ty, by_ref) in captures {
        emitter.comment(&format!("push callback capture ${}", capture_name));
        if *by_ref {
            if !crate::codegen_support::expr::calls::args::emit_ref_arg_variable_address(
                capture_name,
                "callback capture ref",
                emitter,
                ctx,
            ) {
                emitter.comment(&format!(
                    "WARNING: captured callback variable ${} not found",
                    capture_name
                ));
                continue;
            }
            crate::codegen_support::expr::calls::args::push_arg_value(emitter, &PhpType::Int);
            arg_types.push(PhpType::Int);
        } else {
            let Some(capture_info) = ctx.variables.get(capture_name) else {
                emitter.comment(&format!(
                    "WARNING: captured callback variable ${} not found",
                    capture_name
                ));
                continue;
            };
            abi::emit_load(emitter, capture_ty, capture_info.stack_offset);
            crate::codegen_support::expr::calls::args::push_arg_value(emitter, capture_ty);
            arg_types.push(capture_ty.clone());
        }
    }
}

/// Pushes hidden captures loaded from the runtime descriptor stored in a frame slot.
fn push_descriptor_captures_as_hidden_args(
    captures: &[(String, PhpType, bool)],
    descriptor_offset: usize,
    emitter: &mut Emitter,
    arg_types: &mut Vec<PhpType>,
) {
    let descriptor_reg = abi::symbol_scratch_reg(emitter);
    for (idx, (capture_name, capture_ty, by_ref)) in captures.iter().enumerate() {
        emitter.comment(&format!("push descriptor capture ${}", capture_name));
        abi::load_at_offset(emitter, descriptor_reg, descriptor_offset);
        if *by_ref {
            crate::codegen_support::callable_descriptor::emit_load_runtime_capture_to_result(
                emitter,
                descriptor_reg,
                idx,
                &PhpType::Int,
            );
            crate::codegen_support::expr::calls::args::push_arg_value(emitter, &PhpType::Int);
            arg_types.push(PhpType::Int);
        } else {
            crate::codegen_support::callable_descriptor::emit_load_runtime_capture_to_result(
                emitter,
                descriptor_reg,
                idx,
                capture_ty,
            );
            crate::codegen_support::expr::calls::args::push_arg_value(emitter, capture_ty);
            arg_types.push(capture_ty.clone());
        }
    }
}

/// Allocates a temporary stack frame for the callback environment and stores the callback
/// address, array pointer, and all captures into it. Returns the wrapper label and stack layout.
pub(crate) fn emit_captured_callback_env(
    callback_reg: &str,
    array_reg: &str,
    captures: &[(String, PhpType, bool)],
    visible_arg_types: Vec<PhpType>,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> CallbackEnv {
    let wrapper_label = ctx.next_label("callback_wrapper");
    ctx.deferred_callback_wrappers.push(DeferredCallbackWrapper {
        label: wrapper_label.clone(),
        visible_arg_types,
        target_visible_arg_types: None,
        capture_types: captures
            .iter()
            .map(|(_, ty, by_ref)| if *by_ref { PhpType::Int } else { ty.clone() })
            .collect(),
        descriptor_prefix_types: Vec::new(),
        descriptor_return_type: None,
    });

    let env_slots = captures.len() + 2;
    let env_bytes = env_slots * 16;
    let array_slot_offset = (env_slots - 1) * 16;

    emitter.comment("callback capture environment");
    abi::emit_reserve_temporary_stack(emitter, env_bytes);
    store_reg_to_env_slot(emitter, callback_reg, 0);
    store_reg_to_env_slot(emitter, array_reg, array_slot_offset);

    for (idx, (capture_name, capture_ty, by_ref)) in captures.iter().enumerate() {
        emitter.comment(&format!("store callback capture ${}", capture_name));
        if *by_ref {
            if !crate::codegen_support::expr::calls::args::emit_ref_arg_variable_address(
                capture_name,
                "callback capture ref",
                emitter,
                ctx,
            ) {
                emitter.comment(&format!(
                    "WARNING: captured callback variable ${} not found",
                    capture_name
                ));
                continue;
            }
            store_current_result_to_env_slot(emitter, &PhpType::Int, (idx + 1) * 16);
        } else {
            let Some(capture_info) = ctx.variables.get(capture_name) else {
                emitter.comment(&format!(
                    "WARNING: captured callback variable ${} not found",
                    capture_name
                ));
                continue;
            };
            abi::emit_load(emitter, capture_ty, capture_info.stack_offset);
            store_current_result_to_env_slot(emitter, capture_ty, (idx + 1) * 16);
        }
    }

    CallbackEnv {
        wrapper_label,
        env_bytes,
        array_slot_offset,
    }
}

/// Emits assembly for persistent callback env from result.
pub(crate) fn emit_persistent_callback_env_from_result(
    captures: &[(String, PhpType, bool)],
    visible_arg_types: Vec<PhpType>,
    target_visible_arg_types: Vec<PhpType>,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> String {
    let wrapper_label = ctx.next_label("callback_wrapper");
    ctx.deferred_callback_wrappers.push(DeferredCallbackWrapper {
        label: wrapper_label.clone(),
        visible_arg_types,
        target_visible_arg_types: Some(target_visible_arg_types),
        capture_types: captures
            .iter()
            .map(|(_, ty, by_ref)| if *by_ref { PhpType::Int } else { ty.clone() })
            .collect(),
        descriptor_prefix_types: Vec::new(),
        descriptor_return_type: None,
    });

    let env_bytes = (captures.len() + 1) * 16;
    emitter.comment("persistent callback capture environment");
    crate::codegen_support::callable_descriptor::emit_load_entry_from_descriptor(
        emitter,
        abi::int_result_reg(emitter),
        abi::int_result_reg(emitter),
    );
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                 // preserve the original callback entry address while allocating its env
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, #{}", env_bytes));            // request persistent callback environment storage
            emitter.instruction("bl __rt_heap_alloc");                          // allocate the persistent callback environment
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, {}", env_bytes));            // request persistent callback environment storage
            emitter.instruction("call __rt_heap_alloc");                        // allocate the persistent callback environment
        }
    }
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                 // keep the env pointer above the saved callback entry address
    store_saved_callback_to_persistent_env(emitter);

    for (idx, (capture_name, capture_ty, by_ref)) in captures.iter().enumerate() {
        emitter.comment(&format!("store persistent callback capture ${}", capture_name));
        let slot_offset = (idx + 1) * 16;
        if *by_ref {
            if !crate::codegen_support::expr::calls::args::emit_ref_arg_variable_address(
                capture_name,
                "callback capture ref",
                emitter,
                ctx,
            ) {
                emitter.comment(&format!(
                    "WARNING: captured callback variable ${} not found",
                    capture_name
                ));
                continue;
            }
            store_current_result_to_persistent_env_slot(emitter, &PhpType::Int, slot_offset);
        } else {
            let Some(capture_info) = ctx.variables.get(capture_name) else {
                emitter.comment(&format!(
                    "WARNING: captured callback variable ${} not found",
                    capture_name
                ));
                continue;
            };
            abi::emit_load(emitter, capture_ty, capture_info.stack_offset);
            store_current_result_to_persistent_env_slot(emitter, capture_ty, slot_offset);
            retain_persistent_capture_result(emitter, capture_ty);
        }
    }

    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                  // return the persistent env pointer as the current result
    abi::emit_release_temporary_stack(emitter, 16);                            // discard the saved original callback entry address
    wrapper_label
}

/// Emits a heap-backed descriptor callback environment from the current descriptor result.
pub(crate) fn emit_persistent_descriptor_callback_env_from_result(
    callback: &Expr,
    visible_arg_types: Vec<PhpType>,
    descriptor_return_type: PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> Option<String> {
    let ownership = callable_descriptor_result_ownership(callback);
    if !matches!(ownership, HeapOwnership::Owned | HeapOwnership::Borrowed) {
        return None;
    }
    if matches!(ownership, HeapOwnership::Borrowed) {
        crate::codegen_support::callable_descriptor::emit_retain_current_descriptor(emitter);
    }

    let wrapper_label = ctx.next_label("descriptor_callback_wrapper");
    ctx.deferred_callback_wrappers.push(DeferredCallbackWrapper {
        label: wrapper_label.clone(),
        visible_arg_types,
        target_visible_arg_types: None,
        capture_types: Vec::new(),
        descriptor_prefix_types: Vec::new(),
        descriptor_return_type: Some(descriptor_return_type),
    });

    let env_bytes = 16;
    emitter.comment("persistent descriptor callback environment");
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                 // preserve the selected callable descriptor while allocating its env
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, #{}", env_bytes));            // request persistent descriptor callback environment storage
            emitter.instruction("bl __rt_heap_alloc");                          // allocate the persistent descriptor callback environment
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, {}", env_bytes));            // request persistent descriptor callback environment storage
            emitter.instruction("call __rt_heap_alloc");                        // allocate the persistent descriptor callback environment
        }
    }
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                 // keep the env pointer above the saved selected descriptor
    store_saved_callback_to_persistent_env(emitter);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                  // return the persistent descriptor env pointer as the current result
    abi::emit_release_temporary_stack(emitter, 16);                            // discard the saved selected callable descriptor
    Some(wrapper_label)
}

/// Emits a heap-backed descriptor callback environment from a static descriptor label.
///
/// Any descriptor-prefix values must already be pushed on the temporary stack in
/// source order. The helper stores them in persistent environment slots and
/// releases those temporary stack slots before returning the env pointer.
pub(crate) fn emit_persistent_descriptor_callback_env_from_static_descriptor(
    descriptor_label: &str,
    visible_arg_types: Vec<PhpType>,
    descriptor_prefix_types: Vec<PhpType>,
    descriptor_return_type: PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> String {
    let wrapper_label = ctx.next_label("descriptor_callback_wrapper");
    let prefix_count = descriptor_prefix_types.len();
    ctx.deferred_callback_wrappers.push(DeferredCallbackWrapper {
        label: wrapper_label.clone(),
        visible_arg_types,
        target_visible_arg_types: None,
        capture_types: Vec::new(),
        descriptor_prefix_types: descriptor_prefix_types.clone(),
        descriptor_return_type: Some(descriptor_return_type),
    });

    let env_bytes = (prefix_count + 1) * 16;
    let prefix_bytes = prefix_count * 16;
    emitter.comment("persistent static descriptor callback environment");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, #{}", env_bytes));            // request persistent descriptor callback environment storage
            emitter.instruction("bl __rt_heap_alloc");                          // allocate the persistent descriptor callback environment
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, {}", env_bytes));            // request persistent descriptor callback environment storage
            emitter.instruction("call __rt_heap_alloc");                        // allocate the persistent descriptor callback environment
        }
    }
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                 // keep the env pointer above saved descriptor-prefix values
    abi::emit_symbol_address(emitter, abi::int_result_reg(emitter), descriptor_label);
    store_current_result_to_persistent_env_slot(emitter, &PhpType::Callable, 0);
    for (idx, prefix_ty) in descriptor_prefix_types.iter().enumerate() {
        let saved_offset = 16 + (prefix_count - 1 - idx) * 16;
        load_temporary_stack_slot_to_current_result(emitter, prefix_ty, saved_offset);
        store_current_result_to_persistent_env_slot(emitter, prefix_ty, (idx + 1) * 16);
        retain_persistent_capture_result(emitter, prefix_ty);
    }
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                  // return the persistent descriptor env pointer as the current result
    abi::emit_release_temporary_stack(emitter, prefix_bytes);                  // discard saved descriptor-prefix values after env storage
    wrapper_label
}

/// Returns true when a callback expression must preserve the selected runtime descriptor.
pub(crate) fn expr_call_needs_descriptor_callback_env(callback: &Expr, ctx: &Context) -> bool {
    if runtime_callable_expr_result_needs_descriptor_callback_env(callback, ctx) {
        return true;
    }

    match &callback.kind {
        ExprKind::Closure { captures, .. } => !captures.is_empty(),
        ExprKind::FirstClassCallable(target) => first_class_target_needs_runtime_capture(target),
        ExprKind::Variable(name) => callable_variable_needs_descriptor_callback_env(name, ctx),
        ExprKind::Assignment { value, .. } => expr_produces_captured_callable(value, ctx),
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            expr_produces_captured_callable(then_expr, ctx)
                || expr_produces_captured_callable(else_expr, ctx)
        }
        ExprKind::ShortTernary { value, default }
        | ExprKind::NullCoalesce { value, default } => {
            expr_produces_captured_callable(value, ctx)
                || expr_produces_captured_callable(default, ctx)
        }
        _ => false,
    }
}

/// Returns true when a runtime-produced callable result must keep its descriptor environment.
fn runtime_callable_expr_result_needs_descriptor_callback_env(
    callback: &Expr,
    ctx: &Context,
) -> bool {
    if !matches!(
        crate::codegen_support::functions::infer_contextual_type(callback, ctx).codegen_repr(),
        PhpType::Callable
    ) {
        return false;
    }

    match &callback.kind {
        ExprKind::Variable(name) => callable_variable_needs_descriptor_callback_env(name, ctx),
        ExprKind::ArrayAccess { .. }
        | ExprKind::PropertyAccess { .. }
        | ExprKind::DynamicPropertyAccess { .. }
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::Assignment { .. }
        | ExprKind::Ternary { .. }
        | ExprKind::ShortTernary { .. }
        | ExprKind::NullCoalesce { .. }
        | ExprKind::FunctionCall { .. }
        | ExprKind::MethodCall { .. }
        | ExprKind::StaticMethodCall { .. }
        | ExprKind::ExprCall { .. } => true,
        _ => false,
    }
}

/// Returns true when a local callable variable should be carried as a descriptor.
fn callable_variable_needs_descriptor_callback_env(name: &str, ctx: &Context) -> bool {
    if ctx.callable_param_names.contains(name) {
        return true;
    }
    if ctx.runtime_callable_vars.contains(name) {
        return true;
    }
    if ctx
        .closure_captures
        .get(name)
        .is_some_and(|captures| !captures.is_empty())
    {
        return true;
    }
    if ctx
        .first_class_callable_targets
        .get(name)
        .is_some_and(first_class_target_needs_runtime_capture)
    {
        return true;
    }
    false
}

/// Returns true when the selected descriptor can be owned safely by a callback environment.
pub(crate) fn descriptor_callback_env_supported(callback: &Expr) -> bool {
    matches!(
        callable_descriptor_result_ownership(callback),
        HeapOwnership::Owned | HeapOwnership::Borrowed
    )
}

/// Retains a borrowed descriptor result before later source-order argument evaluation.
pub(crate) fn retain_borrowed_descriptor_callback_result(
    callback: &Expr,
    emitter: &mut Emitter,
) -> bool {
    if !matches!(
        callable_descriptor_result_ownership(callback),
        HeapOwnership::Borrowed
    ) {
        return false;
    }
    crate::codegen_support::callable_descriptor::emit_retain_current_descriptor(emitter);
    true
}

/// Emits a descriptor-backed callback environment from the current descriptor result.
pub(crate) fn emit_descriptor_callback_env_from_result(
    callback: &Expr,
    array_reg: &str,
    visible_arg_types: Vec<PhpType>,
    descriptor_return_type: PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> Option<DescriptorCallbackEnv> {
    emit_descriptor_callback_env_from_result_inner(
        callback,
        array_reg,
        visible_arg_types,
        descriptor_return_type,
        true,
        emitter,
        ctx,
    )
}

/// Emits a descriptor-backed callback environment from a descriptor already retained if borrowed.
pub(crate) fn emit_descriptor_callback_env_from_retained_result(
    callback: &Expr,
    array_reg: &str,
    visible_arg_types: Vec<PhpType>,
    descriptor_return_type: PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> Option<DescriptorCallbackEnv> {
    emit_descriptor_callback_env_from_result_inner(
        callback,
        array_reg,
        visible_arg_types,
        descriptor_return_type,
        false,
        emitter,
        ctx,
    )
}

/// Emits descriptor callback environment storage for a statically selected descriptor label.
pub(crate) fn emit_descriptor_callback_env_from_static_descriptor(
    descriptor_label: &str,
    visible_arg_types: Vec<PhpType>,
    descriptor_prefix_types: Vec<PhpType>,
    descriptor_return_type: PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> DescriptorCallbackEnv {
    let wrapper_label = ctx.next_label("descriptor_callback_wrapper");
    let env_slots = descriptor_prefix_types.len() + 2;
    ctx.deferred_callback_wrappers.push(DeferredCallbackWrapper {
        label: wrapper_label.clone(),
        visible_arg_types,
        target_visible_arg_types: None,
        capture_types: Vec::new(),
        descriptor_prefix_types,
        descriptor_return_type: Some(descriptor_return_type),
    });

    let env_bytes = env_slots * 16;
    let array_slot_offset = (env_slots - 1) * 16;
    emitter.comment("static descriptor callback environment");
    abi::emit_reserve_temporary_stack(emitter, env_bytes);
    let descriptor_reg = abi::int_result_reg(emitter);
    abi::emit_symbol_address(emitter, descriptor_reg, descriptor_label);
    store_current_result_to_env_slot(emitter, &PhpType::Callable, 0);

    DescriptorCallbackEnv {
        wrapper_label,
        env_bytes,
        array_slot_offset,
    }
}

/// Emits a descriptor callback environment for a callable-array variable after saving an array.
///
/// The caller must have pushed the runtime array pointer before calling this helper. If the
/// callback is a statically tracked callable-array variable, this evaluates any receiver prefix
/// at the callback-expression point, restores the saved array, and stores both into the new
/// descriptor environment. Returns `None` without touching the stack for unsupported callbacks.
pub(crate) fn emit_callable_array_descriptor_env_after_saved_array(
    callback: &Expr,
    array_reg: &str,
    call_reg: &str,
    visible_arg_types: Vec<PhpType>,
    descriptor_return_type: PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<DescriptorCallbackEnv> {
    let array_callback = resolve_callable_array_descriptor_callback(callback, ctx, data)?;
    if let Some((receiver, _)) = &array_callback.receiver_prefix {
        emit_expr(receiver, emitter, ctx, data);
        emitter.instruction(&format!("mov {}, {}", call_reg, abi::int_result_reg(emitter))); // preserve callable-array receiver while restoring the saved array
    }
    abi::emit_pop_reg(emitter, array_reg);

    let descriptor_prefix_types = array_callback
        .receiver_prefix
        .iter()
        .map(|(_, ty)| ty.clone())
        .collect();
    let wrapper = emit_descriptor_callback_env_from_static_descriptor(
        &array_callback.descriptor_label,
        visible_arg_types,
        descriptor_prefix_types,
        descriptor_return_type,
        emitter,
        ctx,
    );
    if let Some((_, receiver_ty)) = &array_callback.receiver_prefix {
        emitter.instruction(&format!("mov {}, {}", abi::int_result_reg(emitter), call_reg)); // restore callable-array receiver for descriptor prefix storage
        store_descriptor_callback_prefix_result(&wrapper, 0, receiver_ty, emitter);
    }
    store_descriptor_callback_array_reg(&wrapper, array_reg, emitter);
    Some(wrapper)
}

/// Stores the current result in a descriptor callback prefix slot.
pub(crate) fn store_descriptor_callback_prefix_result(
    _env: &DescriptorCallbackEnv,
    idx: usize,
    ty: &PhpType,
    emitter: &mut Emitter,
) {
    store_current_result_to_env_slot(emitter, ty, (idx + 1) * 16);
}

/// Stores a runtime array pointer register into the descriptor callback environment.
pub(crate) fn store_descriptor_callback_array_reg(
    env: &DescriptorCallbackEnv,
    array_reg: &str,
    emitter: &mut Emitter,
) {
    store_reg_to_env_slot(emitter, array_reg, env.array_slot_offset);
}

/// Emits descriptor callback environment storage, optionally retaining borrowed descriptors.
fn emit_descriptor_callback_env_from_result_inner(
    callback: &Expr,
    array_reg: &str,
    visible_arg_types: Vec<PhpType>,
    descriptor_return_type: PhpType,
    retain_borrowed: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> Option<DescriptorCallbackEnv> {
    let ownership = callable_descriptor_result_ownership(callback);
    if !matches!(ownership, HeapOwnership::Owned | HeapOwnership::Borrowed) {
        return None;
    }
    if retain_borrowed && matches!(ownership, HeapOwnership::Borrowed) {
        crate::codegen_support::callable_descriptor::emit_retain_current_descriptor(emitter);
    }

    let wrapper_label = ctx.next_label("descriptor_callback_wrapper");
    ctx.deferred_callback_wrappers.push(DeferredCallbackWrapper {
        label: wrapper_label.clone(),
        visible_arg_types,
        target_visible_arg_types: None,
        capture_types: Vec::new(),
        descriptor_prefix_types: Vec::new(),
        descriptor_return_type: Some(descriptor_return_type),
    });

    let env_bytes = 32;
    let array_slot_offset = 16;
    emitter.comment("descriptor callback environment");
    abi::emit_reserve_temporary_stack(emitter, env_bytes);
    store_current_result_to_env_slot(emitter, &PhpType::Callable, 0);
    store_reg_to_env_slot(emitter, array_reg, array_slot_offset);

    Some(DescriptorCallbackEnv {
        wrapper_label,
        env_bytes,
        array_slot_offset,
    })
}

/// Releases a descriptor-backed callback environment after its runtime helper returns.
pub(crate) fn release_descriptor_callback_env(
    env: &DescriptorCallbackEnv,
    emitter: &mut Emitter,
) {
    let descriptor_reg = abi::int_result_reg(emitter);
    abi::emit_push_reg(emitter, descriptor_reg);                                // preserve the callback runtime result while releasing the selected descriptor
    abi::emit_load_temporary_stack_slot(emitter, descriptor_reg, 16);
    crate::codegen_support::callable_descriptor::emit_release_current_descriptor(emitter);
    abi::emit_pop_reg(emitter, descriptor_reg);                                 // restore the callback runtime result after descriptor release
    abi::emit_release_temporary_stack(emitter, env.env_bytes);
}

/// Builds `$callback[$index]` for reading slots out of a stored callable array.
fn callable_array_slot_expr(var: &str, index: i64) -> Expr {
    Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(Expr::new(ExprKind::Variable(var.to_string()), Span::dummy())),
            index: Box::new(Expr::new(ExprKind::IntLiteral(index), Span::dummy())),
        },
        Span::dummy(),
    )
}

/// Resolves a static callable receiver against the current codegen class context.
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

/// Resolves class names case-insensitively against the codegen class table.
fn resolve_class_name<'a>(ctx: &'a Context, class_name: &str) -> Option<&'a str> {
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    ctx.classes
        .keys()
        .find(|existing| php_symbol_key(existing) == class_key)
        .map(String::as_str)
}

/// Returns the ownership class for a callable descriptor expression result.
fn callable_descriptor_result_ownership(callback: &Expr) -> HeapOwnership {
    match &callback.kind {
        ExprKind::Assignment { .. } => HeapOwnership::Borrowed,
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => callable_descriptor_result_ownership(then_expr)
            .merge(callable_descriptor_result_ownership(else_expr)),
        ExprKind::ShortTernary { value, default }
        | ExprKind::NullCoalesce { value, default } => {
            callable_descriptor_result_ownership(value)
                .merge(callable_descriptor_result_ownership(default))
        }
        _ => expr_result_heap_ownership(callback),
    }
}

/// Returns true if an expression produces a callable with descriptor-owned environment.
fn expr_produces_captured_callable(expr: &Expr, ctx: &Context) -> bool {
    match &expr.kind {
        ExprKind::Closure { captures, .. } => !captures.is_empty(),
        ExprKind::FirstClassCallable(target) => first_class_target_needs_runtime_capture(target),
        ExprKind::Variable(name) => {
            ctx.closure_captures
                .get(name)
                .is_some_and(|captures| !captures.is_empty())
                || ctx
                    .first_class_callable_targets
                    .get(name)
                    .is_some_and(first_class_target_needs_runtime_capture)
        }
        ExprKind::Assignment { value, .. } => expr_produces_captured_callable(value, ctx),
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            expr_produces_captured_callable(then_expr, ctx)
                || expr_produces_captured_callable(else_expr, ctx)
        }
        ExprKind::ShortTernary { value, default }
        | ExprKind::NullCoalesce { value, default } => {
            expr_produces_captured_callable(value, ctx)
                || expr_produces_captured_callable(default, ctx)
        }
        _ => false,
    }
}

/// Returns true when a first-class callable target carries receiver environment.
fn first_class_target_needs_runtime_capture(target: &CallableTarget) -> bool {
    matches!(
        target,
        CallableTarget::Method { .. }
            | CallableTarget::StaticMethod {
                receiver: StaticReceiver::Static,
                ..
            }
    )
}

/// Loads a value from an environment slot into `reg` by computing the slot address on the
/// temporary stack and performing a type-aware load.
pub(crate) fn load_env_slot_to_reg(emitter: &mut Emitter, reg: &str, offset: usize) {
    let scratch = abi::symbol_scratch_reg(emitter);
    abi::emit_temporary_stack_address(emitter, scratch, offset);
    abi::emit_load_from_address(emitter, reg, scratch, 0);
}

/// Emits the address of the base of the temporary callback environment stack frame into `reg`.
/// Used by the deferred wrapper to locate the environment.
pub(crate) fn load_env_pointer_to_reg(emitter: &mut Emitter, reg: &str) {
    abi::emit_temporary_stack_address(emitter, reg, 0);
}

/// Stores the raw value in `reg` directly into the environment slot at `offset` using a
/// temporary stack address scratch register.
fn store_reg_to_env_slot(emitter: &mut Emitter, reg: &str, offset: usize) {
    let scratch = abi::symbol_scratch_reg(emitter);
    abi::emit_temporary_stack_address(emitter, scratch, offset);
    abi::emit_store_to_address(emitter, reg, scratch, 0);
}

/// Stores the current ABI result register(s) into the environment slot at `offset` using a
/// temporary stack address scratch register. Handles float, string (ptr+len), and integer
/// representations per `ty.codegen_repr()`. No-op for `Void`/`Never` types.
fn store_current_result_to_env_slot(emitter: &mut Emitter, ty: &PhpType, offset: usize) {
    let scratch = abi::symbol_scratch_reg(emitter);
    abi::emit_temporary_stack_address(emitter, scratch, offset);
    match ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_store_to_address(emitter, abi::float_result_reg(emitter), scratch, 0);
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_store_to_address(emitter, ptr_reg, scratch, 0);
            abi::emit_store_to_address(emitter, len_reg, scratch, 8);
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), scratch, 0);
        }
    }
}

/// Stores saved callback to persistent env into runtime storage or stack state.
fn store_saved_callback_to_persistent_env(emitter: &mut Emitter) {
    let env_reg = abi::symbol_scratch_reg(emitter);
    let callback_reg = abi::secondary_scratch_reg(emitter);
    abi::emit_load_temporary_stack_slot(emitter, env_reg, 0);
    abi::emit_load_temporary_stack_slot(emitter, callback_reg, 16);
    abi::emit_store_to_address(emitter, callback_reg, env_reg, 0);
}

/// Stores current result to persistent env slot into runtime storage or stack state.
fn store_current_result_to_persistent_env_slot(
    emitter: &mut Emitter,
    ty: &PhpType,
    offset: usize,
) {
    let env_reg = abi::symbol_scratch_reg(emitter);
    abi::emit_load_temporary_stack_slot(emitter, env_reg, 0);
    match ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_store_to_address(emitter, abi::float_result_reg(emitter), env_reg, offset);
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_store_to_address(emitter, ptr_reg, env_reg, offset);
            abi::emit_store_to_address(emitter, len_reg, env_reg, offset + 8);
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), env_reg, offset);
        }
    }
}

/// Loads a temporary-stack value into the standard expression result registers.
fn load_temporary_stack_slot_to_current_result(emitter: &mut Emitter, ty: &PhpType, offset: usize) {
    match ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_load_temporary_stack_slot(emitter, abi::float_result_reg(emitter), offset);
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_load_temporary_stack_slot(emitter, ptr_reg, offset);
            abi::emit_load_temporary_stack_slot(emitter, len_reg, offset + 8);
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), offset);
        }
    }
}

/// Retains persistent capture result so ownership remains valid across runtime calls.
fn retain_persistent_capture_result(emitter: &mut Emitter, ty: &PhpType) {
    match ty.codegen_repr() {
        PhpType::Str => {
            let (ptr_reg, _) = abi::string_result_regs(emitter);
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction(&format!("mov x0, {}", ptr_reg));       // pass the captured string pointer to the retain helper
                    emitter.instruction("bl __rt_incref");                      // retain the captured string for the persistent callback env
                }
                Arch::X86_64 => {
                    if ptr_reg != "rax" {
                        emitter.instruction(&format!("mov rax, {}", ptr_reg));  // pass the captured string pointer to the retain helper
                    }
                    emitter.instruction("call __rt_incref");                    // retain the captured string for the persistent callback env
                }
            }
        }
        other if other.is_refcounted() => {
            abi::emit_incref_if_refcounted(emitter, &other);
        }
        _ => {}
    }
}
