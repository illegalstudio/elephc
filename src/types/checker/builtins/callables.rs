//! Purpose:
//! Type-checks the callables PHP builtin family.
//! Validates arity, argument types, warning-producing cases, and inferred return types for direct calls.
//!
//! Called from:
//! - `crate::types::checker::builtins::check_builtin()`
//!
//! Key details:
//! - Signatures, callable aliases, optimizer effects, and codegen builtin dispatch must remain in lockstep.

use crate::errors::CompileError;
use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{CallableTarget, Expr, ExprKind, StaticReceiver};
use crate::types::array_constants::ARRAY_INT_CONSTANTS;
use crate::types::{FunctionSig, PhpType, TypeEnv};

use super::canonical_builtin_function_name;
use super::super::Checker;

mod preg_replace_callback;

type BuiltinResult = Result<Option<PhpType>, CompileError>;

/// Type-checks a first-class `preg_replace_callback(...)` invocation with callback context.
pub(crate) fn check_preg_replace_callback_first_class_call(
    checker: &mut Checker,
    args: &[Expr],
    span: crate::span::Span,
    env: &TypeEnv,
) -> Result<PhpType, CompileError> {
    preg_replace_callback::check(checker, args, span, env)?
        .ok_or_else(|| CompileError::new(span, "preg_replace_callback() is not available"))
}

/// Specializes a user callback variadic parameter when runtime associative arrays can
/// provide named arguments through `call_user_func_array()`.
pub(super) fn specialize_dynamic_assoc_variadic_user_callback(
    checker: &mut Checker,
    name: &str,
    sig: &FunctionSig,
) -> Result<(), CompileError> {
    if sig.variadic.is_none() {
        return Ok(());
    }
    let Some(decl) = checker.fn_decls.get(name).cloned() else {
        return Ok(());
    };
    let mut param_types = sig.params.clone();
    if let Some((_, variadic_ty)) = param_types.last_mut() {
        *variadic_ty = PhpType::Iterable;
    }
    checker.resolve_function_signature(name, &decl, param_types)?;
    Ok(())
}

/// Validates call user func array dynamic arg array and returns a compile error when it is unsupported.
fn validate_call_user_func_array_dynamic_arg_array(
    checker: &mut Checker,
    _sig: &crate::types::FunctionSig,
    arg_array: &Expr,
    _span: crate::span::Span,
    env: &TypeEnv,
) -> Result<(), CompileError> {
    let arg_array_ty = checker.infer_type(arg_array, env)?;
    if !call_user_func_array_arg_container_is_supported(&arg_array_ty) {
        return Err(CompileError::new(
            arg_array.span,
            "call_user_func_array() second argument must be an array",
        ));
    }
    Ok(())
}

/// Returns true when `call_user_func_array()` can validate the argument container statically or at runtime.
fn call_user_func_array_arg_container_is_supported(arg_array_ty: &PhpType) -> bool {
    matches!(
        arg_array_ty.codegen_repr(),
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed
    )
}

/// Returns true when `call_user_func_array()` must defer argument-shape checks to runtime.
fn call_user_func_array_arg_container_is_runtime_opaque(arg_array_ty: &PhpType) -> bool {
    matches!(arg_array_ty.codegen_repr(), PhpType::Mixed)
}

/// Returns true when an array type can be a PHP callable array selected at runtime.
pub(crate) fn runtime_callable_array_type(ty: &PhpType) -> bool {
    match ty.codegen_repr() {
        PhpType::Array(elem_ty) => matches!(elem_ty.codegen_repr(), PhpType::Mixed | PhpType::Str),
        _ => false,
    }
}

/// Provides the Specialize dynamic assoc variadic first class callback helper used by the callables module.
fn specialize_dynamic_assoc_variadic_first_class_callback(
    checker: &mut Checker,
    target: &CallableTarget,
    sig: &FunctionSig,
    arg_array_ty: &PhpType,
) -> Result<(), CompileError> {
    if !matches!(arg_array_ty, PhpType::AssocArray { .. }) || sig.variadic.is_none() {
        return Ok(());
    }
    if let CallableTarget::Function(name) = target {
        specialize_dynamic_assoc_variadic_user_callback(checker, name.as_str(), sig)?;
    }
    Ok(())
}

/// Produces a dummy expression of the appropriate scalar type for an array's element.
///
/// Selects `Str`, `Float`, `Bool`, or `Int` based on the element type of `arr_ty`.
/// Used to fabricate placeholder call arguments when type-checking array-callback builtins.
fn dummy_arg_for_array_scalar_elem(arr_ty: &PhpType, span: crate::span::Span) -> Expr {
    let elem_ty = match arr_ty {
        PhpType::Array(elem_ty) => elem_ty.as_ref(),
        PhpType::AssocArray { value, .. } => value.as_ref(),
        _ => &PhpType::Int,
    };
    match elem_ty {
        PhpType::Str => Expr::new(ExprKind::StringLiteral(String::new()), span),
        PhpType::Float => Expr::new(ExprKind::FloatLiteral(0.0), span),
        PhpType::Bool => Expr::new(ExprKind::BoolLiteral(false), span),
        _ => Expr::new(ExprKind::IntLiteral(0), span),
    }
}

/// Returns the element type carried by an array/associative-array type.
///
/// Falls back to `Int` for non-array types so callers can build a placeholder
/// comparator argument without special-casing every caller.
fn array_element_type(arr_ty: &PhpType) -> PhpType {
    match arr_ty {
        PhpType::Array(elem_ty) => (**elem_ty).clone(),
        PhpType::AssocArray { value, .. } => (**value).clone(),
        _ => PhpType::Int,
    }
}

/// Reserved synthetic variable name used to give a comparator's dummy argument an
/// object element type. It begins with a digit, so it can never collide with a
/// real PHP variable name (`[A-Za-z_]\w*`).
const COMPARATOR_ELEM_PLACEHOLDER: &str = "0__elephc_cmp_elem";

/// Builds a dummy comparator argument for one array element, plus an optional
/// `(name, type)` environment binding for element types that have no literal form.
///
/// Scalar elements use a literal placeholder, exactly like
/// [`dummy_arg_for_array_scalar_elem`]. Object elements have no literal, so a
/// reserved synthetic variable (see [`COMPARATOR_ELEM_PLACEHOLDER`]) is bound to
/// the element type and returned as the binding; the caller must insert it into
/// the environment used for callback validation so a typed comparator parameter
/// (`function (DateTime $a, DateTime $b)`) is checked against the real type.
fn comparator_dummy_arg_for_elem(
    elem_ty: &PhpType,
    span: crate::span::Span,
) -> (Expr, Option<(String, PhpType)>) {
    match elem_ty {
        PhpType::Str => (Expr::new(ExprKind::StringLiteral(String::new()), span), None),
        PhpType::Float => (Expr::new(ExprKind::FloatLiteral(0.0), span), None),
        PhpType::Bool => (Expr::new(ExprKind::BoolLiteral(false), span), None),
        PhpType::Object(_) => (
            Expr::new(ExprKind::Variable(COMPARATOR_ELEM_PLACEHOLDER.to_string()), span),
            Some((COMPARATOR_ELEM_PLACEHOLDER.to_string(), elem_ty.clone())),
        ),
        _ => (Expr::new(ExprKind::IntLiteral(0), span), None),
    }
}

/// Checks object or array callable call and reports a compile error when it is invalid.
fn check_object_or_array_callable_call(
    checker: &mut Checker,
    callback: &Expr,
    callback_args: &[Expr],
    _span: crate::span::Span,
    env: &TypeEnv,
    allow_by_ref_spread: bool,
    allow_runtime_callable_array: bool,
) -> Result<Option<PhpType>, CompileError> {
    if let ExprKind::Variable(var_name) = &callback.kind {
        if let Some(target) = checker.callable_array_targets.get(var_name).cloned() {
            return check_callable_target_call(
                checker,
                &target,
                callback_args,
                callback,
                env,
                allow_by_ref_spread,
            )
                .map(Some);
        }
    }

    let callback_ty = checker.infer_type(callback, env)?;
    if runtime_callable_array_type(&callback_ty) {
        if !allow_runtime_callable_array {
            return Err(CompileError::new(
                callback.span,
                "callback runtime does not yet support dynamically selected callable arrays",
            ));
        }
        for arg in callback_args {
            checker.infer_type(arg, env)?;
        }
        return Ok(Some(PhpType::Mixed));
    }
    if let Some(class_name) = checker.invokable_class_for_type(&callback_ty) {
        if checker
            .classes
            .get(&class_name)
            .is_some_and(|class_info| class_info.methods.contains_key("__invoke"))
        {
            return if allow_by_ref_spread {
                checker.infer_method_call_on_class_type_allowing_by_ref_spread(
                    &class_name,
                    "__invoke",
                    callback_args,
                    callback,
                    env,
                )
            } else {
                checker.infer_method_call_on_class_type(
                    &class_name,
                    "__invoke",
                    callback_args,
                    callback,
                    env,
                )
            }
            .map(Some);
        }
    }

    let Some((receiver, method)) = callable_array_parts(callback) else {
        return Ok(None);
    };
    if let Some(receiver) = static_callable_receiver(checker, receiver, callback.span)? {
        return if allow_by_ref_spread {
            checker.infer_static_method_call_type_allowing_by_ref_spread(
                &receiver,
                method,
                callback_args,
                callback,
                env,
            )
        } else {
            checker.infer_static_method_call_type(&receiver, method, callback_args, callback, env)
        }
        .map(Some);
    }
    let receiver_ty = checker.infer_type(receiver, env)?;
    let Some(class_name) = checker.invokable_class_for_type(&receiver_ty) else {
        return Ok(None);
    };
    if allow_by_ref_spread {
        checker
            .infer_method_call_on_class_type_allowing_by_ref_spread(
                &class_name,
                method,
                callback_args,
                callback,
                env,
            )
            .map(Some)
    } else {
        checker
            .infer_method_call_on_class_type(&class_name, method, callback_args, callback, env)
            .map(Some)
    }
}

/// Resolves a receiver-bound callable return type when argument details are runtime-only.
fn infer_object_or_array_callable_runtime_return(
    checker: &mut Checker,
    callback: &Expr,
    env: &TypeEnv,
) -> Result<Option<PhpType>, CompileError> {
    if let ExprKind::Variable(var_name) = &callback.kind {
        if let Some(target) = checker.callable_array_targets.get(var_name).cloned() {
            return infer_callable_target_runtime_return(checker, &target, callback, env)
                .map(Some);
        }
    }

    let callback_ty = checker.infer_type(callback, env)?;
    if let Some(class_name) = checker.invokable_class_for_type(&callback_ty) {
        if checker
            .classes
            .get(&class_name)
            .is_some_and(|class_info| class_info.methods.contains_key("__invoke"))
        {
            return infer_instance_method_callable_runtime_return(
                checker,
                &class_name,
                "__invoke",
                callback,
            )
            .map(Some);
        }
    }

    let Some((receiver, method)) = callable_array_parts(callback) else {
        return Ok(None);
    };
    if let Some(receiver) = static_callable_receiver(checker, receiver, callback.span)? {
        return infer_static_method_callable_runtime_return(checker, &receiver, method, callback)
            .map(Some);
    }
    let receiver_ty = checker.infer_type(receiver, env)?;
    let Some(class_name) = checker.invokable_class_for_type(&receiver_ty) else {
        return Ok(None);
    };
    infer_instance_method_callable_runtime_return(checker, &class_name, method, callback).map(Some)
}

/// Resolves a stored callable target's return type without static argument materialization.
fn infer_callable_target_runtime_return(
    checker: &mut Checker,
    target: &CallableTarget,
    callback: &Expr,
    env: &TypeEnv,
) -> Result<PhpType, CompileError> {
    match target {
        CallableTarget::Method { object, method } => {
            let receiver_ty = checker.infer_type(object, env)?;
            let Some(class_name) = checker.invokable_class_for_type(&receiver_ty) else {
                return Err(CompileError::new(
                    callback.span,
                    "callable array receiver must be an object",
                ));
            };
            infer_instance_method_callable_runtime_return(checker, &class_name, method, callback)
        }
        CallableTarget::StaticMethod { receiver, method } => {
            infer_static_method_callable_runtime_return(checker, receiver, method, callback)
        }
        CallableTarget::Function(name) => checker
            .functions
            .get(name.as_str())
            .map(|sig| sig.return_type.clone())
            .ok_or_else(|| {
                CompileError::new(
                    callback.span,
                    &format!("Undefined function: {}", name.as_str()),
                )
            }),
    }
}

/// Resolves an instance-method callable return type without checking runtime-only arguments.
fn infer_instance_method_callable_runtime_return(
    checker: &Checker,
    class_name: &str,
    method: &str,
    callback: &Expr,
) -> Result<PhpType, CompileError> {
    let method_key = php_symbol_key(method);
    let Some(class_info) = checker.classes.get(class_name) else {
        return Err(CompileError::new(
            callback.span,
            &format!("Undefined class: {}", class_name),
        ));
    };
    if let Some(sig) = class_info.methods.get(&method_key) {
        if let Some(visibility) = class_info.method_visibilities.get(&method_key) {
            let declaring_class = class_info
                .method_declaring_classes
                .get(&method_key)
                .map(String::as_str)
                .unwrap_or(class_name);
            if !checker.can_access_member(declaring_class, visibility) {
                return Err(CompileError::new(
                    callback.span,
                    &format!(
                        "Cannot access {} method: {}::{}",
                        Checker::visibility_label(visibility),
                        class_name,
                        method
                    ),
                ));
            }
        }
        let declared_flags = Checker::declared_method_param_flags(class_info, &method_key, false);
        let effective_sig = Checker::callable_sig_for_declared_params(sig, &declared_flags);
        return Ok(effective_sig.return_type);
    }
    if let Some(sig) = class_info.methods.get("__call") {
        let declared_flags = Checker::declared_method_param_flags(class_info, "__call", false);
        let effective_sig = Checker::callable_sig_for_declared_params(sig, &declared_flags);
        return Ok(effective_sig.return_type);
    }
    Err(CompileError::new(
        callback.span,
        &format!("Undefined method: {}::{}", class_name, method),
    ))
}

/// Resolves a static-method callable return type without checking runtime-only arguments.
fn infer_static_method_callable_runtime_return(
    checker: &Checker,
    receiver: &StaticReceiver,
    method: &str,
    callback: &Expr,
) -> Result<PhpType, CompileError> {
    let class_name = resolve_static_receiver_class(checker, receiver, callback.span)?;
    let Some(class_info) = checker.classes.get(&class_name) else {
        return Err(CompileError::new(
            callback.span,
            &format!("Undefined class: {}", class_name),
        ));
    };
    let sig = class_info.static_methods.get(method).ok_or_else(|| {
        if class_info.methods.contains_key(method) {
            CompileError::new(
                callback.span,
                &format!("Cannot call instance method statically: {}::{}", class_name, method),
            )
        } else {
            CompileError::new(
                callback.span,
                &format!("Undefined method: {}::{}", class_name, method),
            )
        }
    })?;
    if let Some(visibility) = class_info.static_method_visibilities.get(method) {
        let declaring_class = class_info
            .static_method_declaring_classes
            .get(method)
            .map(String::as_str)
            .unwrap_or(class_name.as_str());
        if !checker.can_access_member(declaring_class, visibility) {
            return Err(CompileError::new(
                callback.span,
                &format!(
                    "Cannot access {} method: {}::{}",
                    Checker::visibility_label(visibility),
                    class_name,
                    method
                ),
            ));
        }
    }
    let declared_flags = Checker::declared_method_param_flags(class_info, method, true);
    let effective_sig = Checker::callable_sig_for_declared_params(sig, &declared_flags);
    Ok(effective_sig.return_type)
}

/// Checks callable target call and reports a compile error when it is invalid.
fn check_callable_target_call(
    checker: &mut Checker,
    target: &CallableTarget,
    callback_args: &[Expr],
    callback: &Expr,
    env: &TypeEnv,
    allow_by_ref_spread: bool,
) -> Result<PhpType, CompileError> {
    match target {
        CallableTarget::Method { object, method } => {
            let receiver_ty = checker.infer_type(object, env)?;
            let Some(class_name) = checker.invokable_class_for_type(&receiver_ty) else {
                return Err(CompileError::new(
                    callback.span,
                    "callable array receiver must be an object",
                ));
            };
            if allow_by_ref_spread {
                checker.infer_method_call_on_class_type_allowing_by_ref_spread(
                    &class_name,
                    method,
                    callback_args,
                    callback,
                    env,
                )
            } else {
                checker.infer_method_call_on_class_type(
                    &class_name,
                    method,
                    callback_args,
                    callback,
                    env,
                )
            }
        }
        CallableTarget::StaticMethod { receiver, method } => {
            if allow_by_ref_spread {
                checker.infer_static_method_call_type_allowing_by_ref_spread(
                    receiver,
                    method,
                    callback_args,
                    callback,
                    env,
                )
            } else {
                checker.infer_static_method_call_type(receiver, method, callback_args, callback, env)
            }
        }
        CallableTarget::Function(name) => {
            checker.check_function_call(name.as_str(), callback_args, callback.span, env)
        }
    }
}

/// Provides the Callable array parts helper used by the callables module.
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

/// Provides the Static callable receiver helper used by the callables module.
fn static_callable_receiver(
    checker: &Checker,
    receiver: &Expr,
    span: crate::span::Span,
) -> Result<Option<StaticReceiver>, CompileError> {
    let class_name = match &receiver.kind {
        ExprKind::StringLiteral(class_name) => resolve_class_name(checker, class_name)
            .map(str::to_string),
        ExprKind::ClassConstant { receiver } => {
            Some(resolve_static_receiver_class(checker, receiver, span)?)
        }
        _ => None,
    };
    Ok(class_name.map(|class_name| StaticReceiver::Named(Name::from(class_name))))
}

/// Resolves static receiver class using the available compile-time metadata.
fn resolve_static_receiver_class(
    checker: &Checker,
    receiver: &StaticReceiver,
    span: crate::span::Span,
) -> Result<String, CompileError> {
    match receiver {
        StaticReceiver::Named(name) => resolve_class_name(checker, name.as_str())
            .map(str::to_string)
            .ok_or_else(|| CompileError::new(span, &format!("Undefined class: {}", name))),
        StaticReceiver::Self_ | StaticReceiver::Static => checker
            .current_class
            .clone()
            .ok_or_else(|| CompileError::new(span, "Cannot use self::class outside a class context")),
        StaticReceiver::Parent => {
            let current_class = checker.current_class.as_ref().ok_or_else(|| {
                CompileError::new(span, "Cannot use parent::class outside a class context")
            })?;
            checker
                .classes
                .get(current_class)
                .and_then(|class_info| class_info.parent.clone())
                .ok_or_else(|| {
                    CompileError::new(
                        span,
                        &format!("Class '{}' has no parent class", current_class),
                    )
                })
        }
    }
}

/// Resolves class name using the available compile-time metadata.
fn resolve_class_name<'a>(checker: &'a Checker, class_name: &str) -> Option<&'a str> {
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    checker
        .classes
        .keys()
        .find(|existing| php_symbol_key(existing) == class_key)
        .map(String::as_str)
}

#[derive(Clone, Copy, PartialEq, Eq)]
/// Ownership class for a callable descriptor selected by a callback expression.
enum CallbackDescriptorEnvOwnership {
    NonHeap,
    Owned,
    Borrowed,
    MaybeOwned,
}

impl CallbackDescriptorEnvOwnership {
    /// Merges ownership from two expression branches using the same conservative lattice as codegen.
    fn merge(self, other: Self) -> Self {
        use CallbackDescriptorEnvOwnership::*;
        match (self, other) {
            (NonHeap, NonHeap) => NonHeap,
            (Owned, Owned) => Owned,
            (Borrowed, Borrowed) => Borrowed,
            (MaybeOwned, _) | (_, MaybeOwned) => MaybeOwned,
            (Owned, Borrowed) | (Borrowed, Owned) => MaybeOwned,
            (NonHeap, x) | (x, NonHeap) => x,
        }
    }
}

/// Returns true when a callback builtin can use a descriptor-backed callback wrapper.
fn callback_builtin_allows_complex_descriptor_env(
    label: &str,
    callback: &Expr,
) -> bool {
    matches!(
        label,
        "array_map() callback"
            | "array_filter() callback"
            | "array_reduce() callback"
            | "array_walk() callback"
            | "preg_replace_callback() callback"
            | "usort() callback"
            | "uksort() callback"
            | "uasort() callback"
            | "iterator_apply() callback"
    )
        && callback_supports_complex_descriptor_env(callback)
}

/// Returns true when a complex callback expression can be retained in a descriptor environment.
pub(crate) fn callback_supports_complex_descriptor_env(callback: &Expr) -> bool {
    matches!(
        callback_descriptor_env_ownership(callback),
        CallbackDescriptorEnvOwnership::Owned | CallbackDescriptorEnvOwnership::Borrowed
    )
}

/// Classifies descriptor ownership for callback expressions accepted by descriptor wrappers.
fn callback_descriptor_env_ownership(callback: &Expr) -> CallbackDescriptorEnvOwnership {
    match &callback.kind {
        ExprKind::Variable(_) => CallbackDescriptorEnvOwnership::Borrowed,
        ExprKind::Assignment { .. } => CallbackDescriptorEnvOwnership::Borrowed,
        ExprKind::Closure { .. }
        | ExprKind::FirstClassCallable(_)
        | ExprKind::FunctionCall { .. }
        | ExprKind::ClosureCall { .. }
        | ExprKind::ExprCall { .. }
        | ExprKind::MethodCall { .. }
        | ExprKind::NullsafeMethodCall { .. }
        | ExprKind::StaticMethodCall { .. } => CallbackDescriptorEnvOwnership::Owned,
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => callback_descriptor_env_ownership(then_expr)
            .merge(callback_descriptor_env_ownership(else_expr)),
        ExprKind::ShortTernary { value, default }
        | ExprKind::NullCoalesce { value, default } => {
            callback_descriptor_env_ownership(value)
                .merge(callback_descriptor_env_ownership(default))
        }
        _ => CallbackDescriptorEnvOwnership::NonHeap,
    }
}

/// Type-checks a callback expression passed to an array-callback builtin (e.g., `array_map()`).
///
/// Resolves the callback to its signature, checks arity, validates parameter types,
/// and returns the inferred return type. Handles `FirstClassCallable`, `Variable`,
/// `StringLiteral`, and `resolve_expr_callable_sig` callback forms.
///
/// Returns the callback's return type on success, or an error if the callback
/// does not have a statically known callable signature.
pub(crate) fn check_callback_builtin_call(
    checker: &mut Checker,
    callback: &Expr,
    callback_args: &[Expr],
    span: crate::span::Span,
    env: &TypeEnv,
    label: &str,
) -> Result<PhpType, CompileError> {
    if checker.expr_call_complex_callee_needs_runtime_capture(callback)
        && !callback_builtin_allows_complex_descriptor_env(label, callback)
    {
        return Err(CompileError::new(
            callback.span,
            &format!(
                "{} does not support complex expressions that select captured callables at runtime",
                label
            ),
        ));
    }

    if let ExprKind::FirstClassCallable(target) = &callback.kind {
        let sig = checker.specialize_first_class_callable_target(target, callback_args, span, env)?;
        return checker.check_known_callable_call(&sig, callback_args, span, env, label);
    }

    if let ExprKind::Variable(var_name) = &callback.kind {
        if let Some(target) = checker.first_class_callable_targets.get(var_name).cloned() {
            let sig =
                checker.specialize_first_class_callable_target(&target, callback_args, span, env)?;
            checker.callable_sigs.insert(var_name.clone(), sig.clone());
            checker
                .closure_return_types
                .insert(var_name.clone(), sig.return_type.clone());
            return checker.check_known_callable_call(&sig, callback_args, span, env, label);
        }
    }

    if let ExprKind::StringLiteral(cb_name) = &callback.kind {
        if let Some(sig) = checker.functions.get(cb_name.as_str()).cloned() {
            return checker.check_known_callable_call(&sig, callback_args, span, env, label);
        }
        if let Some(decl) = checker.fn_decls.get(cb_name.as_str()).cloned() {
            let effective_arg_count = callback_args.len();
            let required = decl.defaults.iter().filter(|default| default.is_none()).count();
            if decl.variadic.is_some() {
                if effective_arg_count < required {
                    return Err(CompileError::new(
                        span,
                        &format!(
                            "Function '{}' expects at least {} arguments, got {}",
                            cb_name, required, effective_arg_count
                        ),
                    ));
                }
            } else if effective_arg_count < required || effective_arg_count > decl.params.len() {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Function '{}' expects {} arguments, got {}",
                        cb_name,
                        Checker::format_fixed_or_range_arity(required, decl.params.len()),
                        effective_arg_count
                    ),
                ));
            }
            // Keep function-variant discovery, but do not treat scalar dummy args
            // as authoritative parameter types for callbacks over refcounted arrays.
            let _ = checker.check_function_call(cb_name, callback_args, span, env);
            return Ok(PhpType::Int);
        }
        return checker.check_function_call(cb_name, callback_args, span, env);
    }

    if let Some(sig) = checker.resolve_expr_callable_sig(callback, env)? {
        return checker.check_known_callable_call(&sig, callback_args, span, env, label);
    }

    if let Some(ret_ty) =
        check_object_or_array_callable_call(
            checker,
            callback,
            callback_args,
            span,
            env,
            false,
            callback_builtin_allows_runtime_callable_array(label),
        )?
    {
        return Ok(ret_ty);
    }

    let callback_ty = checker.infer_type(callback, env)?;
    if callback_builtin_allows_runtime_string_descriptor(label) && callback_ty == PhpType::Str {
        for arg in callback_args {
            checker.infer_type(arg, env)?;
        }
        return Ok(PhpType::Mixed);
    }

    Err(CompileError::new(
        callback.span,
        &format!("{} must have a statically known callable signature", label),
    ))
}

/// Returns true when a callback builtin can resolve string callbacks at runtime.
fn callback_builtin_allows_runtime_string_descriptor(label: &str) -> bool {
    matches!(
        label,
        "array_map() callback"
            | "array_filter() callback"
            | "array_reduce() callback"
            | "array_walk() callback"
            | "preg_replace_callback() callback"
            | "usort() callback"
            | "uksort() callback"
            | "uasort() callback"
    )
}

/// Returns true when a callback builtin has codegen support for runtime-selected callable arrays.
fn callback_builtin_allows_runtime_callable_array(label: &str) -> bool {
    matches!(
        label,
        "array_map() callback"
            | "array_filter() callback"
            | "array_reduce() callback"
            | "array_walk() callback"
            | "preg_replace_callback() callback"
            | "usort() callback"
            | "uksort() callback"
            | "uasort() callback"
            | "iterator_apply() callback"
    )
}

/// Type-checks a callable-family builtin call.
///
/// Validates arity, argument types, warning-producing cases, and inferred return types.
/// Returns `Ok(Some(PhpType))` for handled builtins, `Ok(None)` for unknown names,
/// or a `CompileError` for type/arity violations.
pub(super) fn check_builtin(
    checker: &mut Checker,
    name: &str,
    args: &[Expr],
    span: crate::span::Span,
    env: &TypeEnv,
) -> BuiltinResult {
    match name {
        "preg_replace_callback" => preg_replace_callback::check(checker, args, span, env),
        "array_map" => {
            if args.len() != 2 {
                return Err(CompileError::new(span, "array_map() takes exactly 2 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            let arr_ty = checker.infer_type(&args[1], env)?;
            match arr_ty {
                PhpType::Array(elem_ty) => {
                    let arr_ty = PhpType::Array(elem_ty.clone());
                    let dummy_args = vec![dummy_arg_for_array_scalar_elem(&arr_ty, span)];
                    let callback_ret_ty = check_callback_builtin_call(
                        checker,
                        &args[0],
                        &dummy_args,
                        span,
                        env,
                        "array_map() callback",
                    )?;
                    let result_elem_ty = if callback_ret_ty == PhpType::Mixed {
                        Box::new(PhpType::Mixed)
                    } else {
                        elem_ty
                    };
                    Ok(Some(PhpType::Array(result_elem_ty)))
                }
                _ => Err(CompileError::new(
                    span,
                    "array_map() second argument must be array",
                )),
            }
        }
        "array_filter" => {
            if args.len() < 2 || args.len() > 3 {
                return Err(CompileError::new(
                    span,
                    "array_filter() takes 2 or 3 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            let arr_ty = checker.infer_type(&args[0], env)?;
            match arr_ty {
                PhpType::Array(elem_ty) => {
                    let arr_ty = PhpType::Array(elem_ty.clone());
                    let dummy_args = array_filter_callback_dummy_args(&arr_ty, args.get(2), span);
                    check_callback_builtin_call(
                        checker,
                        &args[1],
                        &dummy_args,
                        span,
                        env,
                        "array_filter() callback",
                    )?;
                    Ok(Some(PhpType::Array(elem_ty)))
                }
                _ => Err(CompileError::new(
                    span,
                    "array_filter() first argument must be array",
                )),
            }
        }
        "array_reduce" => {
            if args.len() != 3 {
                return Err(CompileError::new(
                    span,
                    "array_reduce() takes exactly 3 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            let arr_ty = checker.infer_type(&args[0], env)?;
            let dummy_args = vec![
                Expr::new(ExprKind::IntLiteral(0), span),
                dummy_arg_for_array_scalar_elem(&arr_ty, span),
            ];
            check_callback_builtin_call(
                checker,
                &args[1],
                &dummy_args,
                span,
                env,
                "array_reduce() callback",
            )?;
            Ok(Some(PhpType::Int))
        }
        "array_walk" => {
            if args.len() != 2 {
                return Err(CompileError::new(span, "array_walk() takes exactly 2 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            let arr_ty = checker.infer_type(&args[0], env)?;
            let dummy_args = vec![dummy_arg_for_array_scalar_elem(&arr_ty, span)];
            check_callback_builtin_call(
                checker,
                &args[1],
                &dummy_args,
                span,
                env,
                "array_walk() callback",
            )?;
            Ok(Some(PhpType::Void))
        }
        "usort" | "uksort" | "uasort" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 2 arguments", name),
                ));
            }
            // Infer the array first so a value comparator can be typed from the
            // element. `usort`/`uasort` compare values; `uksort` compares keys.
            let arr_ty = checker.infer_type(&args[0], env)?;
            let cmp_ty = if name == "uksort" {
                PhpType::Int
            } else {
                array_element_type(&arr_ty)
            };
            let label = format!("{}() callback", name);
            if let PhpType::Object(_) = cmp_ty {
                // Object-element value comparators receive the object handle: type
                // both comparator parameters as that object so an unannotated
                // comparator (`$a <=> $b`, `$a->method()`) checks against the real
                // type instead of the default `Int` placeholder.
                if let ExprKind::Closure {
                    params,
                    variadic,
                    return_type,
                    body,
                    captures,
                    capture_refs,
                    ..
                } = &args[1].kind
                {
                    checker.infer_closure_type_with_param_hints(
                        params,
                        variadic,
                        return_type,
                        body,
                        captures,
                        capture_refs,
                        &args[1],
                        env,
                        &[cmp_ty.clone(), cmp_ty.clone()],
                    )?;
                } else {
                    checker.infer_type(&args[1], env)?;
                    let (cmp_arg, elem_binding) = comparator_dummy_arg_for_elem(&cmp_ty, span);
                    let dummy_args = vec![cmp_arg.clone(), cmp_arg];
                    let mut env_with_elem;
                    let cb_env: &TypeEnv = match &elem_binding {
                        Some((binding_name, binding_ty)) => {
                            env_with_elem = env.clone();
                            env_with_elem.insert(binding_name.clone(), binding_ty.clone());
                            &env_with_elem
                        }
                        None => env,
                    };
                    check_callback_builtin_call(checker, &args[1], &dummy_args, span, cb_env, &label)?;
                }
            } else {
                // Scalar (and unsupported) element comparators keep the original
                // validation: the comparator body is checked against the default
                // placeholder element and the EIR backend decides which element
                // payloads it can actually sort.
                checker.infer_type(&args[1], env)?;
                let cmp_arg = if name == "uksort" {
                    Expr::new(ExprKind::IntLiteral(0), span)
                } else {
                    dummy_arg_for_array_scalar_elem(&arr_ty, span)
                };
                let dummy_args = vec![cmp_arg.clone(), cmp_arg];
                check_callback_builtin_call(checker, &args[1], &dummy_args, span, env, &label)?;
            }
            Ok(Some(PhpType::Void))
        }
        "call_user_func_array" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "call_user_func_array() takes exactly 2 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            if let ExprKind::FirstClassCallable(target) = &args[0].kind {
                let sig = if let ExprKind::ArrayLiteral(elems) = &args[1].kind {
                    checker.specialize_first_class_callable_target(target, elems, span, env)?
                } else {
                    checker.resolve_first_class_callable_sig(target, span, env)?
                };
                validate_call_user_func_array_dynamic_arg_array(checker, &sig, &args[1], span, env)?;
                let arg_array_ty = checker.infer_type(&args[1], env)?;
                specialize_dynamic_assoc_variadic_first_class_callback(
                    checker,
                    target,
                    &sig,
                    &arg_array_ty,
                )?;
                if let ExprKind::ArrayLiteral(elems) = &args[1].kind {
                    let ret_ty = checker.check_known_callable_call(
                        &sig,
                        elems,
                        span,
                        env,
                        "call_user_func_array() callback",
                    )?;
                    return Ok(Some(ret_ty));
                }
                return Ok(Some(sig.return_type));
            }
            if let ExprKind::Variable(var_name) = &args[0].kind {
                if let Some(target) = checker.first_class_callable_targets.get(var_name).cloned() {
                    let sig = if let ExprKind::ArrayLiteral(elems) = &args[1].kind {
                        checker.specialize_first_class_callable_target(&target, elems, span, env)?
                    } else {
                        checker.resolve_first_class_callable_sig(&target, span, env)?
                    };
                    checker.callable_sigs.insert(var_name.clone(), sig.clone());
                    checker
                        .closure_return_types
                        .insert(var_name.clone(), sig.return_type.clone());
                    validate_call_user_func_array_dynamic_arg_array(
                        checker,
                        &sig,
                        &args[1],
                        span,
                        env,
                    )?;
                    let arg_array_ty = checker.infer_type(&args[1], env)?;
                    specialize_dynamic_assoc_variadic_first_class_callback(
                        checker,
                        &target,
                        &sig,
                        &arg_array_ty,
                    )?;
                    if let ExprKind::ArrayLiteral(elems) = &args[1].kind {
                        let ret_ty = checker.check_known_callable_call(
                            &sig,
                            elems,
                            span,
                            env,
                            "call_user_func_array() callback",
                        )?;
                        return Ok(Some(ret_ty));
                    }
                    return Ok(Some(sig.return_type));
                }
            }
            if let ExprKind::StringLiteral(cb_name) = &args[0].kind {
                if let Some(extern_name) = checker.canonical_extern_function_name_folded(cb_name) {
                    if let ExprKind::ArrayLiteral(elems) = &args[1].kind {
                        let ret_ty =
                            checker.check_extern_function_call(&extern_name, elems, span, env)?;
                        return Ok(Some(ret_ty));
                    }
                    if let Some(sig) = checker.functions.get(extern_name.as_str()).cloned() {
                        return Ok(Some(sig.return_type));
                    }
                }
                if let Some(builtin_name) = canonical_builtin_function_name(cb_name) {
                    if let ExprKind::ArrayLiteral(elems) = &args[1].kind {
                        if let Some(ret_ty) =
                            checker.check_builtin(&builtin_name, elems, span, env)?
                        {
                            return Ok(Some(ret_ty));
                        }
                    }
                    if let Some(sig) = crate::types::first_class_callable_builtin_sig(&builtin_name)
                    {
                        return Ok(Some(sig.return_type));
                    }
                }
                let cb_name = checker
                    .canonical_function_name_folded(cb_name)
                    .unwrap_or_else(|| cb_name.clone());
                if !checker.functions.contains_key(cb_name.as_str()) {
                    if let Some(decl) = checker.fn_decls.get(cb_name.as_str()).cloned() {
                        if decl.ref_params.iter().any(|is_ref| *is_ref)
                            && !matches!(args[1].kind, ExprKind::ArrayLiteral(_))
                        {
                            let param_types =
                                checker.initial_function_param_types(&cb_name, &decl)?;
                            checker.resolve_function_signature(&cb_name, &decl, param_types)?;
                        }
                    }
                }
                if let Some(sig) = checker.functions.get(cb_name.as_str()).cloned() {
                    validate_call_user_func_array_dynamic_arg_array(
                        checker,
                        &sig,
                        &args[1],
                        span,
                        env,
                    )?;
                    let arg_array_ty = checker.infer_type(&args[1], env)?;
                    if matches!(arg_array_ty, PhpType::AssocArray { .. }) && sig.variadic.is_some()
                    {
                        specialize_dynamic_assoc_variadic_user_callback(
                            checker,
                            &cb_name,
                            &sig,
                        )?;
                    }
                    if let ExprKind::ArrayLiteral(elems) = &args[1].kind {
                        let ret_ty = checker.check_known_callable_call(
                            &sig,
                            elems,
                            span,
                            env,
                            "call_user_func_array() callback",
                        )?;
                        return Ok(Some(ret_ty));
                    }
                    return Ok(Some(sig.return_type.clone()));
                }
                if let ExprKind::ArrayLiteral(elems) = &args[1].kind {
                    let ret_ty = checker.check_function_call(&cb_name, elems, span, env)?;
                    return Ok(Some(ret_ty));
                }
                if checker.fn_decls.contains_key(cb_name.as_str()) {
                    let spread_args = vec![Expr::new(
                        ExprKind::Spread(Box::new(args[1].clone())),
                        args[1].span,
                    )];
                    let ret_ty = checker.check_function_call(&cb_name, &spread_args, span, env)?;
                    return Ok(Some(ret_ty));
                }
                // A string-literal callback that matched no extern, builtin, user function,
                // or fn_decl is an undefined function. Reject plain function-name callbacks
                // here instead of falling through to the generic `Str` acceptance below
                // (which returns Mixed and defers the unknown name to a dangling symbol /
                // mangle escape at codegen). Strings containing "::" are static-method
                // forms and keep their existing fall-through path.
                if !cb_name.contains("::") {
                    return Err(CompileError::new(
                        args[0].span,
                        &format!("Undefined function: {}", cb_name),
                    ));
                }
            }
            let arg_array_ty = checker.infer_type(&args[1], env)?;
            if call_user_func_array_arg_container_is_runtime_opaque(&arg_array_ty) {
                if let Some(ret_ty) =
                    infer_object_or_array_callable_runtime_return(checker, &args[0], env)?
                {
                    return Ok(Some(ret_ty));
                }
            }
            let spread_args = vec![Expr::new(
                ExprKind::Spread(Box::new(args[1].clone())),
                args[1].span,
            )];
            if let Some(ret_ty) =
                check_object_or_array_callable_call(
                    checker,
                    &args[0],
                    &spread_args,
                    span,
                    env,
                    true,
                    true,
                )?
            {
                if !call_user_func_array_arg_container_is_supported(&arg_array_ty) {
                    return Err(CompileError::new(
                        args[1].span,
                        "call_user_func_array() second argument must be an array",
                    ));
                }
                return Ok(Some(ret_ty));
            }
            if let Some(sig) = checker.resolve_expr_callable_sig(&args[0], env)? {
                validate_call_user_func_array_dynamic_arg_array(checker, &sig, &args[1], span, env)?;
                if let ExprKind::ArrayLiteral(elems) = &args[1].kind {
                    let ret_ty = checker.check_known_callable_call(
                        &sig,
                        elems,
                        span,
                        env,
                        "call_user_func_array() callback",
                    )?;
                    return Ok(Some(ret_ty));
                }
                return Ok(Some(sig.return_type.clone()));
            }
            let callback_ty = checker.infer_type(&args[0], env)?;
            if callback_ty == PhpType::Str
                && call_user_func_array_arg_container_is_supported(&arg_array_ty)
            {
                return Ok(Some(PhpType::Mixed));
            }
            if callback_ty == PhpType::Callable
                && call_user_func_array_arg_container_is_supported(&arg_array_ty)
            {
                return Ok(Some(PhpType::Mixed));
            }
            Err(CompileError::new(
                args[0].span,
                "call_user_func_array() callback must be callable",
            ))
        }
        "call_user_func" => {
            if args.is_empty() {
                return Err(CompileError::new(
                    span,
                    "call_user_func() takes at least 1 argument",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            if let ExprKind::FirstClassCallable(target) = &args[0].kind {
                let sig =
                    checker.specialize_first_class_callable_target(target, &args[1..], span, env)?;
                let ret_ty = checker.check_known_callable_call(
                    &sig,
                    &args[1..],
                    span,
                    env,
                    "call_user_func() callback",
                )?;
                return Ok(Some(ret_ty));
            }
            if let ExprKind::Variable(var_name) = &args[0].kind {
                if let Some(target) = checker.first_class_callable_targets.get(var_name).cloned() {
                    let sig = checker.specialize_first_class_callable_target(
                        &target,
                        &args[1..],
                        span,
                        env,
                    )?;
                    checker.callable_sigs.insert(var_name.clone(), sig.clone());
                    checker
                        .closure_return_types
                        .insert(var_name.clone(), sig.return_type.clone());
                    let ret_ty = checker.check_known_callable_call(
                        &sig,
                        &args[1..],
                        span,
                        env,
                        "call_user_func() callback",
                    )?;
                    return Ok(Some(ret_ty));
                }
            }
            if let ExprKind::StringLiteral(cb_name) = &args[0].kind {
                if let Some(extern_name) = checker.canonical_extern_function_name_folded(cb_name) {
                    let ret_ty =
                        checker.check_extern_function_call(&extern_name, &args[1..], span, env)?;
                    return Ok(Some(ret_ty));
                }
                if let Some(builtin_name) = canonical_builtin_function_name(cb_name) {
                    if let Some(ret_ty) =
                        checker.check_builtin(&builtin_name, &args[1..], span, env)?
                    {
                        return Ok(Some(ret_ty));
                    }
                }
                let cb_name = checker
                    .canonical_function_name_folded(cb_name)
                    .unwrap_or_else(|| cb_name.clone());
                if let Some(sig) = checker.functions.get(cb_name.as_str()).cloned() {
                    let ret_ty = checker.check_known_callable_call(
                        &sig,
                        &args[1..],
                        span,
                        env,
                        "call_user_func() callback",
                    )?;
                    return Ok(Some(ret_ty));
                }
                let cb_args = args[1..].to_vec();
                let ret_ty = checker.check_function_call(&cb_name, &cb_args, span, env)?;
                return Ok(Some(ret_ty));
            }
            if let Some(ret_ty) =
                check_object_or_array_callable_call(
                    checker,
                    &args[0],
                    &args[1..],
                    span,
                    env,
                    true,
                    true,
                )?
            {
                return Ok(Some(ret_ty));
            }
            if let Some(sig) = checker.resolve_expr_callable_sig(&args[0], env)? {
                let ret_ty = checker.check_known_callable_call(
                    &sig,
                    &args[1..],
                    span,
                    env,
                    "call_user_func() callback",
                )?;
                return Ok(Some(ret_ty));
            }
            let callback_ty = checker.infer_type(&args[0], env)?;
            if callback_ty == PhpType::Str {
                for arg in &args[1..] {
                    checker.infer_type(arg, env)?;
                }
                return Ok(Some(PhpType::Mixed));
            }
            if callback_ty == PhpType::Callable {
                for arg in &args[1..] {
                    checker.infer_type(arg, env)?;
                }
                return Ok(Some(PhpType::Mixed));
            }
            Err(CompileError::new(
                args[0].span,
                "call_user_func() callback must be callable",
            ))
        }
        "class_alias" => {
            if args.len() < 2 || args.len() > 3 {
                return Err(CompileError::new(
                    span,
                    "class_alias() takes 2 or 3 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            return Err(CompileError::new(
                span,
                "class_alias() is only supported as a top-level statement with literal class names",
            ));
        }
        "class_exists" | "interface_exists" | "trait_exists" | "enum_exists" => {
            if args.is_empty() || args.len() > 2 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes 1 or 2 arguments", name),
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            if !matches!(args[0].kind, ExprKind::StringLiteral(_)) {
                return Err(CompileError::new(
                    span,
                    &format!("{}() first argument must be a string literal in AOT mode", name),
                ));
            }
            if let Some(autoload_arg) = args.get(1) {
                if !matches!(
                    autoload_arg.kind,
                    ExprKind::BoolLiteral(_) | ExprKind::IntLiteral(_)
                ) {
                    return Err(CompileError::new(
                        span,
                        &format!(
                            "{}() autoload argument must be a literal bool or int in AOT mode",
                            name
                        ),
                    ));
                }
            }
            Ok(Some(PhpType::Bool))
        }
        "class_implements" | "class_parents" | "class_uses" => {
            if args.is_empty() || args.len() > 2 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes 1 or 2 arguments", name),
                ));
            }
            let first_ty = checker.infer_type(&args[0], env)?;
            if !matches!(first_ty, PhpType::Object(_))
                && !matches!(args[0].kind, ExprKind::StringLiteral(_))
            {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "{}() first argument must be an object or string literal in AOT mode",
                        name
                    ),
                ));
            }
            if let Some(autoload_arg) = args.get(1) {
                checker.infer_type(autoload_arg, env)?;
                if !matches!(
                    autoload_arg.kind,
                    ExprKind::BoolLiteral(_) | ExprKind::IntLiteral(_)
                ) {
                    return Err(CompileError::new(
                        span,
                        &format!(
                            "{}() autoload argument must be a literal bool or int in AOT mode",
                            name
                        ),
                    ));
                }
            }
            Ok(Some(PhpType::Union(vec![
                PhpType::AssocArray {
                    key: Box::new(PhpType::Str),
                    value: Box::new(PhpType::Str),
                },
                PhpType::Bool,
            ])))
        }
        "get_class" => {
            if args.len() > 1 {
                return Err(CompileError::new(
                    span,
                    "get_class() takes at most 1 argument",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Str))
        }
        "get_parent_class" => {
            if args.len() > 1 {
                return Err(CompileError::new(
                    span,
                    "get_parent_class() takes at most 1 argument",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Str))
        }
        "is_a" | "is_subclass_of" => {
            if args.len() < 2 || args.len() > 3 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes 2 or 3 arguments", name),
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Bool))
        }
        "get_declared_classes" | "get_declared_interfaces" | "get_declared_traits" => {
            if !args.is_empty() {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes no arguments", name),
                ));
            }
            Ok(Some(PhpType::Array(Box::new(PhpType::Str))))
        }
        "function_exists" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "function_exists() takes exactly 1 argument",
                ));
            }
            checker.infer_type(&args[0], env)?;
            if let ExprKind::StringLiteral(cb_name) = &args[0].kind {
                let cb_name = cb_name.trim_start_matches('\\');
                let cb_name = checker
                    .canonical_function_name_folded(cb_name)
                    .unwrap_or_else(|| cb_name.to_string());
                if checker.fn_decls.contains_key(cb_name.as_str())
                    && !checker.functions.contains_key(cb_name.as_str())
                {
                    if let Some(decl) = checker.fn_decls.get(cb_name.as_str()).cloned() {
                        let dummy_args: Vec<Expr> = decl
                            .params
                            .iter()
                            .map(|_| Expr::new(ExprKind::IntLiteral(0), span))
                            .collect();
                        let _ = checker.check_function_call(&cb_name, &dummy_args, span, env);
                    }
                } else if checker.function_variant_groups.contains_key(cb_name.as_str())
                    && !checker.functions.contains_key(cb_name.as_str())
                {
                    let _ = checker.ensure_function_variant_group_signature(&cb_name, span);
                }
            }
            Ok(Some(PhpType::Bool))
        }
        _ => Ok(None),
    }
}

/// Builds synthetic callback arguments for `array_filter()` based on a static mode.
///
/// Unknown or invalid runtime modes use the default value-only shape for type checking;
/// runtime validation still throws before invoking the callback when the mode is invalid.
fn array_filter_callback_dummy_args(
    arr_ty: &PhpType,
    mode_arg: Option<&Expr>,
    span: crate::span::Span,
) -> Vec<Expr> {
    match mode_arg.and_then(static_array_filter_mode_value) {
        Some(1) => vec![
            dummy_arg_for_array_scalar_elem(arr_ty, span),
            Expr::new(ExprKind::IntLiteral(0), span),
        ],
        Some(2) => vec![Expr::new(ExprKind::IntLiteral(0), span)],
        _ => vec![dummy_arg_for_array_scalar_elem(arr_ty, span)],
    }
}

/// Returns a compile-time `array_filter()` mode value for integer literals and predefined constants.
fn static_array_filter_mode_value(expr: &Expr) -> Option<i64> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::ConstRef(name) => ARRAY_INT_CONSTANTS
            .iter()
            .find_map(|(constant, value)| (*constant == name.as_str()).then_some(*value)),
        _ => None,
    }
}
