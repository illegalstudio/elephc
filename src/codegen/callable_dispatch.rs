//! Purpose:
//! Defines runtime callable dispatch metadata shared by indirect callback emitters.
//! Bridges AOT function signatures with runtime-selected callable values or names.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::call_user_func_array`
//!
//! Key details:
//! - Cases carry the ABI entry label, optional PHP-visible name, signature metadata, and hidden captures.
//! - String-name dispatch compares against userland callable names before loading the matched descriptor.

use crate::codegen::abi;
use crate::codegen::callable_descriptor::{
    self, CallableDescriptorInvocation, CallableDescriptorShape,
};
use crate::codegen::context::{Context, DeferredClosure, DeferredRuntimeCallableInvoker};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::names::{function_symbol, php_symbol_key, Name};
use crate::parser::ast::{Expr, ExprKind, StaticReceiver, Stmt, StmtKind, Visibility};
use crate::span::Span;
use crate::types::{
    callable_wrapper_sig, first_class_callable_builtin_sig, ExternFunctionSig, FunctionSig, PhpType,
};
use crate::types::checker::builtins::supported_builtin_function_names;

const RUNTIME_RECEIVER_PARAM: &str = "__elephc_callable_receiver";

#[derive(Clone)]
pub(crate) struct RuntimeCallableCase {
    pub(crate) label: String,
    pub(crate) descriptor_label: String,
    pub(crate) php_name: Option<String>,
    pub(crate) sig: FunctionSig,
    pub(crate) captures: Vec<(String, PhpType, bool)>,
    pub(crate) has_invoker: bool,
    pub(crate) invoker_label: Option<String>,
}

pub(crate) enum RuntimeCallableSelector<'a> {
    Address(&'a str),
    StringNameStack {
        ptr_offset: usize,
        len_offset: usize,
        call_reg: &'a str,
    },
}

#[derive(Clone, Copy)]
pub(crate) enum RuntimeInstanceCallableShape {
    ObjectInvoke,
    InstanceMethod,
}

#[derive(Clone)]
pub(crate) struct RuntimeInstanceMethodCallableCase {
    pub(crate) class_id: u64,
    pub(crate) method_name: String,
    pub(crate) case: RuntimeCallableCase,
}

#[derive(Clone)]
pub(crate) struct RuntimeStaticMethodCallableCase {
    pub(crate) class_name: String,
    pub(crate) method_name: String,
    pub(crate) case: RuntimeCallableCase,
}

/// Provides the Runtime callable cases helper used by the callable dispatch module.
pub(crate) fn runtime_callable_cases(
    ctx: &mut Context,
    data: &mut DataSection,
    captures: &[(String, PhpType, bool)],
    source_arg_ty: Option<&PhpType>,
) -> Vec<RuntimeCallableCase> {
    let mut cases = Vec::new();
    let source_elem_ty = source_arg_ty.map(runtime_case_source_elem_ty);
    if captures.is_empty() {
        for (name, sig) in runtime_extern_wrappers(ctx) {
            let case_sig = callable_wrapper_sig(&sig);
            let label = ensure_runtime_extern_wrapper(ctx, &name, &case_sig);
            let invoker_label = ensure_runtime_descriptor_invoker(ctx, captures, &case_sig);
            let descriptor_label = runtime_case_descriptor(
                data,
                &label,
                Some(&name),
                callable_descriptor::CALLABLE_DESC_KIND_EXTERN,
                &case_sig,
                &[],
                &[],
                CallableDescriptorInvocation::named(CallableDescriptorShape::Extern, &name),
                invoker_label.as_deref(),
            );
            cases.push(RuntimeCallableCase {
                label,
                descriptor_label,
                php_name: Some(name),
                sig: case_sig,
                captures: Vec::new(),
                has_invoker: invoker_label.is_some(),
                invoker_label,
            });
        }
        for name in supported_builtin_function_names() {
            if runtime_builtin_wrapper_excluded(name) || runtime_extern_named(ctx, name) {
                continue;
            }
            let Some(sig) = first_class_callable_builtin_sig(name) else {
                continue;
            };
            let case_sig = callable_wrapper_sig(&sig);
            let label = ensure_runtime_builtin_wrapper(ctx, name, &case_sig);
            let invoker_label = ensure_runtime_descriptor_invoker(ctx, captures, &case_sig);
            let descriptor_label = runtime_case_descriptor(
                data,
                &label,
                Some(name),
                callable_descriptor::CALLABLE_DESC_KIND_BUILTIN,
                &case_sig,
                &[],
                &[],
                CallableDescriptorInvocation::named(CallableDescriptorShape::Builtin, *name),
                invoker_label.as_deref(),
            );
            cases.push(RuntimeCallableCase {
                label,
                descriptor_label,
                php_name: Some((*name).to_string()),
                sig: case_sig,
                captures: Vec::new(),
                has_invoker: invoker_label.is_some(),
                invoker_label,
            });
        }
        for (class_name, method_name, sig) in runtime_static_method_wrappers(ctx) {
            let case_sig = static_method_runtime_wrapper_sig(&sig);
            let label =
                ensure_runtime_static_method_wrapper(ctx, &class_name, &method_name, &case_sig);
            let php_name = format!("{}::{}", class_name, method_name);
            let invoker_label = ensure_runtime_descriptor_invoker(ctx, captures, &case_sig);
            let descriptor_label = runtime_case_descriptor(
                data,
                &label,
                Some(&php_name),
                callable_descriptor::CALLABLE_DESC_KIND_STATIC_METHOD,
                &case_sig,
                &[],
                &[],
                CallableDescriptorInvocation::method(
                    CallableDescriptorShape::StaticMethod,
                    Some(class_name.clone()),
                    method_name.as_str(),
                ),
                invoker_label.as_deref(),
            );
            cases.push(RuntimeCallableCase {
                label,
                descriptor_label,
                php_name: Some(php_name),
                sig: case_sig,
                captures: Vec::new(),
                has_invoker: invoker_label.is_some(),
                invoker_label,
            });
        }
    }
    let user_functions: Vec<(String, FunctionSig)> = ctx
        .functions
        .iter()
        .filter(|(name, _)| !ctx.extern_functions.contains_key(*name))
        .map(|(name, sig)| (name.clone(), sig.clone()))
        .collect();
    for (name, sig) in user_functions {
        let case_sig = callable_wrapper_sig(&sig);
        let invoker_label = ensure_runtime_descriptor_invoker(ctx, captures, &case_sig);
        let descriptor_label = runtime_case_descriptor(
            data,
            &function_symbol(&name),
            Some(&name),
            callable_descriptor::CALLABLE_DESC_KIND_FUNCTION,
            &case_sig,
            &[],
            &[],
            CallableDescriptorInvocation::named(CallableDescriptorShape::Function, &name),
            invoker_label.as_deref(),
        );
        cases.push(RuntimeCallableCase {
            label: function_symbol(&name),
            descriptor_label,
            php_name: Some(name),
            sig: case_sig,
            captures: Vec::new(),
            has_invoker: invoker_label.is_some(),
            invoker_label,
        });
    }
    let mut deferred_closure_cases = Vec::new();
    for deferred in &mut ctx.deferred_closures {
        if !captures.is_empty() && deferred.hidden_params.as_slice() != captures {
            continue;
        }
        let sig = specialized_runtime_case_sig(&deferred.sig, source_elem_ty.as_ref());
        deferred.sig = sig.clone();
        deferred_closure_cases.push((
            deferred.label.clone(),
            sig,
            deferred.captures.clone(),
            deferred.hidden_params.clone(),
        ));
    }
    for (label, sig, closure_captures, hidden_params) in deferred_closure_cases {
        let invoker_label = ensure_runtime_descriptor_invoker(ctx, &hidden_params, &sig);
        let descriptor_label = runtime_case_descriptor(
            data,
            &label,
            None,
            callable_descriptor::CALLABLE_DESC_KIND_CLOSURE,
            &sig,
            &closure_captures,
            &hidden_params,
            CallableDescriptorInvocation::new(CallableDescriptorShape::Closure),
            invoker_label.as_deref(),
        );
        cases.push(RuntimeCallableCase {
            label,
            descriptor_label,
            php_name: None,
            sig,
            captures: hidden_params,
            has_invoker: invoker_label.is_some(),
            invoker_label,
        });
    }
    cases.sort_by(|left, right| left.label.cmp(&right.label));
    cases.dedup_by(|left, right| left.label == right.label);
    cases
}

/// Emits a runtime-callable case descriptor and returns its data label.
fn runtime_case_descriptor(
    data: &mut DataSection,
    label: &str,
    php_name: Option<&str>,
    kind: u64,
    sig: &FunctionSig,
    captures: &[(String, PhpType, bool)],
    hidden_params: &[(String, PhpType, bool)],
    invocation: CallableDescriptorInvocation,
    invoker_label: Option<&str>,
) -> String {
    callable_descriptor::static_descriptor_with_optional_invoker_meta(
        data,
        label,
        php_name,
        kind,
        Some(sig),
        captures,
        hidden_params,
        invocation,
        invoker_label,
    )
}

/// Returns the element/value type visible to dynamic argument specialization.
fn runtime_case_source_elem_ty(source_arg_ty: &PhpType) -> PhpType {
    match source_arg_ty {
        PhpType::Array(elem_ty) => *elem_ty.clone(),
        PhpType::AssocArray { value, .. } => *value.clone(),
        other => other.clone(),
    }
}

/// Ensures a descriptor-compatible runtime invoker exists for the callable signature.
pub(crate) fn ensure_runtime_descriptor_invoker(
    ctx: &mut Context,
    captures: &[(String, PhpType, bool)],
    sig: &FunctionSig,
) -> Option<String> {
    if let Some(existing) = ctx
        .deferred_runtime_callable_invokers
        .iter()
        .find(|invoker| invoker.sig == *sig && invoker.captures == captures)
    {
        return Some(existing.label.clone());
    }
    let label = ctx.next_label("callable_invoker");
    ctx.deferred_runtime_callable_invokers
        .push(DeferredRuntimeCallableInvoker {
            label: label.clone(),
            sig: sig.clone(),
            captures: captures.to_vec(),
        });
    Some(label)
}

/// Provides runtime extern wrapper metadata in deterministic declaration-name order.
fn runtime_extern_wrappers(ctx: &Context) -> Vec<(String, FunctionSig)> {
    let mut wrappers: Vec<(String, FunctionSig)> = ctx
        .extern_functions
        .iter()
        .map(|(name, extern_sig)| {
            let sig = ctx
                .functions
                .get(name)
                .cloned()
                .unwrap_or_else(|| function_sig_from_extern(extern_sig));
            (name.clone(), sig)
        })
        .collect();
    wrappers.sort_by(|left, right| left.0.cmp(&right.0));
    wrappers
}

/// Converts extern metadata to the PHP-facing wrapper signature used by descriptor dispatch.
fn function_sig_from_extern(sig: &ExternFunctionSig) -> FunctionSig {
    FunctionSig {
        params: sig.params.clone(),
        defaults: vec![None; sig.params.len()],
        return_type: sig.return_type.clone(),
        declared_return: true,
        by_ref_return: false,
        ref_params: vec![false; sig.params.len()],
        declared_params: vec![true; sig.params.len()],
        variadic: None,
        deprecation: None,
    }
}

/// Returns whether an extern declaration shadows a builtin callback name.
fn runtime_extern_named(ctx: &Context, name: &str) -> bool {
    let name_key = php_symbol_key(name);
    ctx.extern_functions
        .keys()
        .any(|extern_name| php_symbol_key(extern_name) == name_key)
}

/// Provides the Runtime static method wrappers helper used by the callable dispatch module.
fn runtime_static_method_wrappers(ctx: &Context) -> Vec<(String, String, FunctionSig)> {
    let mut wrappers = Vec::new();
    for (class_name, class_info) in &ctx.classes {
        // Synthetic builtin classes (e.g. DateTime::createFromFormat) are emitted on demand, so
        // their static-method symbols may not exist in a program that never uses the class. Keep
        // them out of the dynamic-callable descriptor to avoid referencing an unemitted symbol,
        // mirroring how they are excluded from dynamic `new $x()`.
        if crate::codegen::expr::objects::known_dynamic_new_builtin_class_names()
            .contains(&class_name.as_str())
        {
            continue;
        }
        for (method_name, sig) in &class_info.static_methods {
            if !class_info
                .static_method_visibilities
                .get(method_name)
                .is_some_and(|visibility| matches!(visibility, Visibility::Public))
            {
                continue;
            }
            wrappers.push((class_name.clone(), method_name.clone(), sig.clone()));
        }
    }
    wrappers.sort_by(|left, right| (&left.0, &left.1).cmp(&(&right.0, &right.1)));
    wrappers
}

/// Builds descriptor cases for every public instance method visible to runtime callable arrays.
pub(crate) fn runtime_public_instance_method_cases(
    ctx: &mut Context,
    data: &mut DataSection,
) -> Vec<RuntimeInstanceMethodCallableCase> {
    let mut methods = Vec::new();
    for (class_name, class_info) in &ctx.classes {
        for method_name in class_info.methods.keys() {
            if !class_info
                .method_visibilities
                .get(method_name)
                .is_some_and(|visibility| matches!(visibility, Visibility::Public))
            {
                continue;
            }
            methods.push((class_name.clone(), class_info.class_id, method_name.clone()));
        }
    }
    methods.sort_by(|left, right| (&left.0, &left.2).cmp(&(&right.0, &right.2)));

    let mut cases = Vec::new();
    for (class_name, class_id, method_name) in methods {
        if let Some(case) = runtime_instance_method_case(
            ctx,
            data,
            &class_name,
            &method_name,
            RuntimeInstanceCallableShape::InstanceMethod,
        ) {
            cases.push(RuntimeInstanceMethodCallableCase {
                class_id,
                method_name,
                case,
            });
        }
    }
    cases
}

/// Builds descriptor cases for every public static method visible to runtime callable arrays.
pub(crate) fn runtime_public_static_method_cases(
    ctx: &mut Context,
    data: &mut DataSection,
) -> Vec<RuntimeStaticMethodCallableCase> {
    let wrappers = runtime_static_method_wrappers(ctx);
    let mut cases = Vec::new();
    for (class_name, method_name, _) in wrappers {
        if let Some(case) = runtime_static_method_case(ctx, data, &class_name, &method_name) {
            cases.push(RuntimeStaticMethodCallableCase {
                class_name,
                method_name,
                case,
            });
        }
    }
    cases
}

/// Builds the runtime descriptor case for one public static method callable.
pub(crate) fn runtime_static_method_case(
    ctx: &mut Context,
    data: &mut DataSection,
    class_name: &str,
    method_name: &str,
) -> Option<RuntimeCallableCase> {
    let (resolved_method_name, sig) = {
        let class_info = ctx.classes.get(class_name)?;
        let method_key = php_symbol_key(method_name);
        let (resolved_method_name, sig) = class_info
            .static_methods
            .iter()
            .find(|(candidate, _)| php_symbol_key(candidate) == method_key)?;
        if !class_info
            .static_method_visibilities
            .get(resolved_method_name)
            .is_some_and(|visibility| matches!(visibility, Visibility::Public))
        {
            return None;
        }
        (resolved_method_name.clone(), sig.clone())
    };

    let case_sig = static_method_runtime_wrapper_sig(&sig);
    let label = ensure_runtime_static_method_wrapper(
        ctx,
        class_name,
        &resolved_method_name,
        &case_sig,
    );
    let php_name = format!("{}::{}", class_name, resolved_method_name);
    let invoker_label = ensure_runtime_descriptor_invoker(ctx, &[], &case_sig);
    let descriptor_label = runtime_case_descriptor(
        data,
        &label,
        Some(&php_name),
        callable_descriptor::CALLABLE_DESC_KIND_STATIC_METHOD,
        &case_sig,
        &[],
        &[],
        CallableDescriptorInvocation::method(
            CallableDescriptorShape::StaticMethod,
            Some(class_name.to_string()),
            resolved_method_name.as_str(),
        ),
        invoker_label.as_deref(),
    );

    Some(RuntimeCallableCase {
        label,
        descriptor_label,
        php_name: Some(php_name),
        sig: case_sig,
        captures: Vec::new(),
        has_invoker: invoker_label.is_some(),
        invoker_label,
    })
}

/// Builds the runtime descriptor case for one public instance-method or `__invoke` callable.
pub(crate) fn runtime_instance_method_case(
    ctx: &mut Context,
    data: &mut DataSection,
    class_name: &str,
    method_name: &str,
    shape: RuntimeInstanceCallableShape,
) -> Option<RuntimeCallableCase> {
    let (resolved_method_name, sig) = {
        let class_info = ctx.classes.get(class_name)?;
        let method_key = php_symbol_key(method_name);
        let (resolved_method_name, sig) = class_info
            .methods
            .iter()
            .find(|(candidate, _)| php_symbol_key(candidate) == method_key)?;
        if !class_info
            .method_visibilities
            .get(resolved_method_name)
            .is_some_and(|visibility| matches!(visibility, Visibility::Public))
        {
            return None;
        }
        (resolved_method_name.clone(), sig.clone())
    };

    let case_sig = instance_method_runtime_wrapper_sig(class_name, &sig);
    let label =
        ensure_runtime_instance_method_wrapper(ctx, class_name, &resolved_method_name, &case_sig);
    let php_name = format!("{}::{}", class_name, resolved_method_name);
    let invoker_label = ensure_runtime_descriptor_invoker(ctx, &[], &case_sig);
    let (kind, invocation_shape) = match shape {
        RuntimeInstanceCallableShape::ObjectInvoke => (
            callable_descriptor::CALLABLE_DESC_KIND_OBJECT_INVOKE,
            CallableDescriptorShape::ObjectInvoke,
        ),
        RuntimeInstanceCallableShape::InstanceMethod => (
            callable_descriptor::CALLABLE_DESC_KIND_INSTANCE_METHOD,
            CallableDescriptorShape::InstanceMethod,
        ),
    };
    let descriptor_label = runtime_case_descriptor(
        data,
        &label,
        Some(&php_name),
        kind,
        &case_sig,
        &[],
        &[],
        CallableDescriptorInvocation::method(
            invocation_shape,
            Some(class_name.to_string()),
            resolved_method_name.as_str(),
        ),
        invoker_label.as_deref(),
    );

    Some(RuntimeCallableCase {
        label,
        descriptor_label,
        php_name: Some(php_name),
        sig: case_sig,
        captures: Vec::new(),
        has_invoker: invoker_label.is_some(),
        invoker_label,
    })
}

/// Provides the Runtime builtin wrapper excluded helper used by the callable dispatch module.
///
/// `__elephc_mktime_raw` / `__elephc_gmmktime_raw` are internal escape hatches that the
/// `mktime`/`gmmktime` procedural-alias rewriter and synthetic DateTime bodies call directly.
/// They are lowered inline by the active EIR backend (`__rt_mktime` / `__rt_gmmktime`) and have no
/// standalone `fn_` symbol, but the deferred-closure wrapper body emitted here is lowered by the
/// frozen legacy direct backend, which does not know these names and would emit an unresolved
/// `bl _fn_<name>` reference. They are never invoked dynamically, so excluding them from the
/// dynamic-call descriptor table is both safe and semantically correct.
fn runtime_builtin_wrapper_excluded(name: &str) -> bool {
    matches!(
        name,
        "iterator_apply" | "preg_replace_callback"
            | "__elephc_mktime_raw" | "__elephc_gmmktime_raw"
    )
}

/// Ensures runtime builtin wrapper is available before the caller continues.
pub(crate) fn ensure_runtime_builtin_wrapper(
    ctx: &mut Context,
    name: &str,
    sig: &FunctionSig,
) -> String {
    if let Some(label) = ctx.runtime_callable_builtin_wrappers.get(name) {
        return label.clone();
    }

    let label = ctx.next_label("callable_builtin");
    let params: Vec<String> = sig.params.iter().map(|(name, _)| name.clone()).collect();
    ctx.deferred_closures.push(DeferredClosure {
        label: label.clone(),
        params,
        body: builtin_wrapper_body(name, sig),
        sig: sig.clone(),
        captures: Vec::new(),
        hidden_params: Vec::new(),
        current_class: None,
        needed: true,
    });
    ctx.runtime_callable_builtin_wrappers
        .insert(name.to_string(), label.clone());
    label
}

/// Ensures a PHP-ABI extern wrapper is available before runtime descriptor dispatch uses it.
fn ensure_runtime_extern_wrapper(
    ctx: &mut Context,
    name: &str,
    sig: &FunctionSig,
) -> String {
    if let Some(label) = ctx.runtime_callable_extern_wrappers.get(name) {
        return label.clone();
    }

    let label = ctx.next_label("callable_extern");
    let params: Vec<String> = sig.params.iter().map(|(name, _)| name.clone()).collect();
    ctx.deferred_closures.push(DeferredClosure {
        label: label.clone(),
        params,
        body: extern_wrapper_body(name, sig),
        sig: sig.clone(),
        captures: Vec::new(),
        hidden_params: Vec::new(),
        current_class: None,
        needed: true,
    });
    ctx.runtime_callable_extern_wrappers
        .insert(name.to_string(), label.clone());
    label
}

/// Ensures runtime static method wrapper is available before the caller continues.
pub(crate) fn ensure_runtime_static_method_wrapper(
    ctx: &mut Context,
    class_name: &str,
    method_name: &str,
    sig: &FunctionSig,
) -> String {
    let key = format!("{}::{}", class_name, method_name);
    if let Some(label) = ctx.runtime_callable_static_method_wrappers.get(&key) {
        return label.clone();
    }

    let label = ctx.next_label("callable_static_method");
    let params: Vec<String> = sig.params.iter().map(|(name, _)| name.clone()).collect();
    ctx.deferred_closures.push(DeferredClosure {
        label: label.clone(),
        params,
        body: static_method_wrapper_body(class_name, method_name, sig),
        sig: sig.clone(),
        captures: Vec::new(),
        hidden_params: Vec::new(),
        current_class: None,
        needed: true,
    });
    ctx.runtime_callable_static_method_wrappers
        .insert(key, label.clone());
    label
}

/// Ensures runtime instance-method wrapper is available before descriptor dispatch uses it.
fn ensure_runtime_instance_method_wrapper(
    ctx: &mut Context,
    class_name: &str,
    method_name: &str,
    sig: &FunctionSig,
) -> String {
    let key = format!("{}::{}", class_name, method_name);
    if let Some(label) = ctx.runtime_callable_instance_method_wrappers.get(&key) {
        return label.clone();
    }

    let label = ctx.next_label("callable_instance_method");
    let params: Vec<String> = sig.params.iter().map(|(name, _)| name.clone()).collect();
    ctx.deferred_closures.push(DeferredClosure {
        label: label.clone(),
        params,
        body: instance_method_wrapper_body(method_name, sig),
        sig: sig.clone(),
        captures: Vec::new(),
        hidden_params: Vec::new(),
        current_class: Some(class_name.to_string()),
        needed: true,
    });
    ctx.runtime_callable_instance_method_wrappers
        .insert(key, label.clone());
    label
}

/// Builds a static-method runtime wrapper signature that can receive keyed variadic tails.
pub(crate) fn static_method_runtime_wrapper_sig(sig: &FunctionSig) -> FunctionSig {
    let mut wrapper_sig = callable_wrapper_sig(sig);
    if wrapper_sig.variadic.is_some() {
        if let Some((_, ty)) = wrapper_sig.params.last_mut() {
            *ty = PhpType::Iterable;
        }
    }
    wrapper_sig
}

/// Builds an instance-method runtime wrapper signature with receiver in slot zero.
fn instance_method_runtime_wrapper_sig(class_name: &str, sig: &FunctionSig) -> FunctionSig {
    let mut wrapper_sig = callable_wrapper_sig(sig);
    wrapper_sig.params.insert(
        0,
        (
            RUNTIME_RECEIVER_PARAM.to_string(),
            PhpType::Object(class_name.to_string()),
        ),
    );
    wrapper_sig.defaults.insert(0, None);
    wrapper_sig.ref_params.insert(0, false);
    wrapper_sig.declared_params.insert(0, true);
    if wrapper_sig.variadic.is_some() {
        if let Some((_, ty)) = wrapper_sig.params.last_mut() {
            *ty = PhpType::Iterable;
        }
    }
    wrapper_sig
}

/// Builds the synthetic method body for static method wrapper.
fn static_method_wrapper_body(class_name: &str, method_name: &str, sig: &FunctionSig) -> Vec<Stmt> {
    let last_param_idx = sig.params.len().saturating_sub(1);
    let args: Vec<Expr> = sig
        .params
        .iter()
        .enumerate()
        .map(|(idx, (param_name, _))| {
            let var = Expr::new(ExprKind::Variable(param_name.clone()), Span::dummy());
            if sig.variadic.is_some() && idx == last_param_idx {
                Expr::new(ExprKind::Spread(Box::new(var)), Span::dummy())
            } else {
                var
            }
        })
        .collect();
    let call = Expr::new(
        ExprKind::StaticMethodCall {
            receiver: StaticReceiver::Named(Name::from(class_name.to_string())),
            method: method_name.to_string(),
            args,
        },
        Span::dummy(),
    );

    if sig.return_type == PhpType::Void {
        vec![
            Stmt::new(StmtKind::ExprStmt(call), Span::dummy()),
            Stmt::new(StmtKind::Return(None), Span::dummy()),
        ]
    } else {
        vec![Stmt::new(StmtKind::Return(Some(call)), Span::dummy())]
    }
}

/// Builds the synthetic method body for an instance-method wrapper.
fn instance_method_wrapper_body(method_name: &str, sig: &FunctionSig) -> Vec<Stmt> {
    let last_param_idx = sig.params.len().saturating_sub(1);
    let args: Vec<Expr> = sig
        .params
        .iter()
        .enumerate()
        .skip(1)
        .map(|(idx, (param_name, _))| {
            let var = Expr::new(ExprKind::Variable(param_name.clone()), Span::dummy());
            if sig.variadic.is_some() && idx == last_param_idx {
                Expr::new(ExprKind::Spread(Box::new(var)), Span::dummy())
            } else {
                var
            }
        })
        .collect();
    let receiver = Expr::new(
        ExprKind::Variable(RUNTIME_RECEIVER_PARAM.to_string()),
        Span::dummy(),
    );
    let call = Expr::new(
        ExprKind::MethodCall {
            object: Box::new(receiver),
            method: method_name.to_string(),
            args,
        },
        Span::dummy(),
    );

    if sig.return_type == PhpType::Void {
        vec![
            Stmt::new(StmtKind::ExprStmt(call), Span::dummy()),
            Stmt::new(StmtKind::Return(None), Span::dummy()),
        ]
    } else {
        vec![Stmt::new(StmtKind::Return(Some(call)), Span::dummy())]
    }
}

/// Builds the synthetic function body for an extern wrapper.
fn extern_wrapper_body(name: &str, sig: &FunctionSig) -> Vec<Stmt> {
    function_wrapper_body(name, sig)
}

/// Builds the synthetic method body for builtin wrapper.
fn builtin_wrapper_body(name: &str, sig: &FunctionSig) -> Vec<Stmt> {
    function_wrapper_body(name, sig)
}

/// Builds the synthetic body that forwards visible wrapper parameters to a function call.
fn function_wrapper_body(name: &str, sig: &FunctionSig) -> Vec<Stmt> {
    let last_param_idx = sig.params.len().saturating_sub(1);
    let args: Vec<Expr> = sig
        .params
        .iter()
        .enumerate()
        .map(|(idx, (param_name, _))| {
            let var = Expr::new(ExprKind::Variable(param_name.clone()), Span::dummy());
            if sig.variadic.is_some() && idx == last_param_idx {
                Expr::new(ExprKind::Spread(Box::new(var)), Span::dummy())
            } else {
                var
            }
        })
        .collect();
    let call = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified(name),
            args,
        },
        Span::dummy(),
    );

    if sig.return_type == PhpType::Void {
        vec![
            Stmt::new(StmtKind::ExprStmt(call), Span::dummy()),
            Stmt::new(StmtKind::Return(None), Span::dummy()),
        ]
    } else {
        vec![Stmt::new(StmtKind::Return(Some(call)), Span::dummy())]
    }
}

/// Emits assembly for branch if callable case mismatch.
pub(crate) fn emit_branch_if_callable_case_mismatch(
    selector: &RuntimeCallableSelector<'_>,
    case: &RuntimeCallableCase,
    next_case: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    match selector {
        RuntimeCallableSelector::Address(call_reg) => {
            emit_branch_if_address_mismatch(call_reg, &case.label, next_case, emitter);
        }
        RuntimeCallableSelector::StringNameStack {
            ptr_offset,
            len_offset,
            call_reg,
        } => {
            emit_branch_if_string_name_mismatch(
                case,
                *ptr_offset,
                *len_offset,
                call_reg,
                next_case,
                emitter,
                ctx,
                data,
            );
        }
    }
}

/// Computes the callable signature metadata for specialized runtime case.
fn specialized_runtime_case_sig(
    sig: &FunctionSig,
    source_elem_ty: Option<&PhpType>,
) -> FunctionSig {
    let Some(source_elem_ty) = source_elem_ty else {
        return sig.clone();
    };
    let mut sig = sig.clone();
    let source_ty = source_elem_ty.codegen_repr();
    if matches!(source_ty, PhpType::Void | PhpType::Never) {
        return sig;
    }
    let visible_param_count = sig.params.len();
    let regular_param_count = if sig.variadic.is_some() {
        visible_param_count.saturating_sub(1)
    } else {
        visible_param_count
    };
    for i in 0..regular_param_count {
        if sig.declared_params.get(i).copied().unwrap_or(false)
            || sig.ref_params.get(i).copied().unwrap_or(false)
        {
            continue;
        }
        if let Some((_, param_ty)) = sig.params.get_mut(i) {
            if !matches!(param_ty.codegen_repr(), PhpType::Int) {
                continue;
            }
            *param_ty = source_ty.clone();
        }
    }
    if sig.variadic.is_some() {
        let variadic_idx = visible_param_count.saturating_sub(1);
        if !sig
            .declared_params
            .get(variadic_idx)
            .copied()
            .unwrap_or(false)
        {
            if let Some((_, param_ty)) = sig.params.get_mut(variadic_idx) {
                *param_ty = PhpType::Array(Box::new(source_ty));
            }
        }
    }
    sig
}

/// Emits assembly for branch if address mismatch.
fn emit_branch_if_address_mismatch(
    call_reg: &str,
    candidate_label: &str,
    next_case: &str,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x9", candidate_label);
            emitter.instruction(&format!("cmp {}, x9", call_reg));              // does the runtime callable entry match this AOT signature case?
            emitter.instruction(&format!("b.ne {}", next_case));                // try the next callable signature case when the pointer differs
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "r10", candidate_label);
            emitter.instruction(&format!("cmp {}, r10", call_reg));             // does the runtime callable entry match this AOT signature case?
            emitter.instruction(&format!("jne {}", next_case));                 // try the next callable signature case when the pointer differs
        }
    }
}

/// Emits assembly for branch if string name mismatch.
#[allow(clippy::too_many_arguments)]
fn emit_branch_if_string_name_mismatch(
    case: &RuntimeCallableCase,
    ptr_offset: usize,
    len_offset: usize,
    call_reg: &str,
    next_case: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let Some(php_name) = case.php_name.as_ref() else {
        abi::emit_jump(emitter, next_case);
        return;
    };

    let matched_label = ctx.next_label("callable_string_match");
    let mut candidates = vec![php_name.clone()];
    if !php_name.starts_with('\\') {
        candidates.push(format!("\\{}", php_name));
    }

    for candidate in candidates {
        emit_string_name_compare(
            ptr_offset,
            len_offset,
            candidate.as_bytes(),
            &matched_label,
            emitter,
            data,
        );
    }
    abi::emit_jump(emitter, next_case);

    emitter.label(&matched_label);
    abi::emit_symbol_address(emitter, call_reg, &case.descriptor_label);
}

/// Emits assembly for string name compare.
fn emit_string_name_compare(
    ptr_offset: usize,
    len_offset: usize,
    candidate: &[u8],
    matched_label: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    let (candidate_label, candidate_len) = data.add_string(candidate);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x1", ptr_offset);
            abi::emit_load_temporary_stack_slot(emitter, "x2", len_offset);
            abi::emit_symbol_address(emitter, "x3", &candidate_label);
            abi::emit_load_int_immediate(emitter, "x4", candidate_len as i64);
            abi::emit_call_label(emitter, "__rt_strcasecmp");
            emitter.instruction("cmp x0, #0");                                  // did the runtime string callback name match this userland target?
            emitter.instruction(&format!("b.eq {}", matched_label));            // select this callable case when names match case-insensitively
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "rdi", ptr_offset);
            abi::emit_load_temporary_stack_slot(emitter, "rsi", len_offset);
            abi::emit_symbol_address(emitter, "rdx", &candidate_label);
            abi::emit_load_int_immediate(emitter, "rcx", candidate_len as i64);
            abi::emit_call_label(emitter, "__rt_strcasecmp");
            emitter.instruction("test rax, rax");                               // did the runtime string callback name match this userland target?
            emitter.instruction(&format!("je {}", matched_label));              // select this callable case when names match case-insensitively
        }
    }
}
