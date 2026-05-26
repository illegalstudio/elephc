//! Purpose:
//! Lowers first-class callable creation for functions, methods, and builtins.
//! Resolves the callable shape, prepares arguments, and leaves the call result for expression consumers.
//!
//! Called from:
//! - `crate::codegen::expr::calls`
//!
//! Key details:
//! - Callable metadata and argument signatures must stay synchronized with type checking and runtime dispatch.

use crate::codegen::abi;
use crate::codegen::context::{Context, DeferredClosure, HeapOwnership};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::names::Name;
use crate::parser::ast::{CallableTarget, Expr, ExprKind, StaticReceiver, Stmt, StmtKind};
use crate::span::Span;
use crate::types::{callable_wrapper_sig, first_class_callable_builtin_sig, FunctionSig, PhpType};

const FCC_CALLED_CLASS_ID_PARAM: &str = "__elephc_fcc_called_class_id";
const FCC_THIS_PARAM: &str = "__elephc_fcc_this";
const FCC_RECEIVER_PARAM: &str = "__elephc_fcc_receiver";

/// Returns a unique temporary name for first-class callable method receiver storage.
pub(crate) fn method_receiver_temp_name(span: Span) -> String {
    format!("__elephc_fcc_receiver_{}_{}", span.line, span.col)
}

/// Provides the Resolved static callable target helper used by the first class module.
fn resolved_static_callable_target(receiver: &StaticReceiver, ctx: &Context) -> Option<StaticReceiver> {
    // Resolves `self` and `parent` static receivers to their concrete class names.
    // Returns `None` for `Static` receiver, which relies on late-static binding and
    // cannot be resolved at compile time.
    match receiver {
        StaticReceiver::Named(name) => Some(StaticReceiver::Named(name.clone())),
        StaticReceiver::Self_ => ctx
            .current_class
            .as_ref()
            .map(|name| StaticReceiver::Named(Name::from(name.clone()))),
        StaticReceiver::Parent => {
            let current_class = ctx.current_class.as_ref()?;
            let parent = ctx.classes.get(current_class)?.parent.clone()?;
            Some(StaticReceiver::Named(Name::from(parent)))
        }
        StaticReceiver::Static => None,
    }
}

/// Provides the Static callable lookup class helper used by the first class module.
fn static_callable_lookup_class(receiver: &StaticReceiver, ctx: &Context) -> Option<String> {
    // Looks up the concrete class name for a static callable receiver.
    // - `Named`: returns the explicit class name.
    // - `Self_` / `Static`: returns the current class from context.
    // - `Parent`: returns the parent class of the current class.
    // Returns `None` when the current class is unset or the parent chain is exhausted.
    match receiver {
        StaticReceiver::Named(name) => Some(name.as_str().to_string()),
        StaticReceiver::Self_ | StaticReceiver::Static => ctx.current_class.clone(),
        StaticReceiver::Parent => {
            let current_class = ctx.current_class.as_ref()?;
            ctx.classes.get(current_class)?.parent.clone()
        }
    }
}

/// Returns the wrapper signature for a first-class callable from a callable target, if resolvable.
pub(super) fn first_class_callable_sig(target: &CallableTarget, ctx: &Context) -> Option<FunctionSig> {
    let sig = match target {
        CallableTarget::Function(name) => ctx
            .functions
            .get(name.as_str())
            .cloned()
            .or_else(|| first_class_callable_builtin_sig(name.as_str())),
        CallableTarget::StaticMethod { receiver, method } => {
            let class_name = static_callable_lookup_class(receiver, ctx)?;
            ctx.classes
                .get(&class_name)
                .and_then(|class_info| class_info.static_methods.get(method))
                .cloned()
        }
        CallableTarget::Method { object, method } => {
            let object_ty = crate::codegen::functions::infer_contextual_type(object, ctx);
            let class_name = crate::codegen::functions::singular_object_class(&object_ty)?;
            ctx.classes
                .get(class_name)
                .and_then(|class_info| class_info.methods.get(method))
                .cloned()
        }
    }?;

    Some(callable_wrapper_sig(&sig))
}

/// Builds the parameter metadata for unique hidden.
fn unique_hidden_param(base: &str, sig: &FunctionSig) -> String {
    // Generates a unique hidden parameter name by appending an index suffix if `base`
    // already exists in `sig.params`. Checks all existing parameter names to avoid
    // collisions when multiple hidden params are needed.
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

/// Provides the Capture for static target helper used by the first class module.
fn capture_for_static_target(ctx: &Context) -> Option<(String, PhpType)> {
    // Captures the late-static binding context for a static method first-class callable.
    // Prefers `__elephc_called_class_id` (set when `static::` is used) over `this`.
    // Returns `None` if neither is available in the variable scope.
    if ctx.variables.contains_key("__elephc_called_class_id") {
        return Some(("__elephc_called_class_id".to_string(), PhpType::Int));
    }
    ctx.variables
        .get("this")
        .map(|var| ("this".to_string(), var.ty.clone()))
}

/// Builds the codegen diagnostic shown when first-class callable creation
/// rejects a target. Pinpoints the specific limitation (complex method
/// receiver, missing late-static binding context, etc.) so the developer
/// understands which form is unsupported instead of seeing a generic warning.
fn unsupported_fcc_diagnostic(target: &CallableTarget) -> String {
    match target {
        CallableTarget::Method { object, method } => match &object.kind {
            ExprKind::Variable(_) | ExprKind::This => format!(
                "WARNING: unsupported first-class callable target for method ->{}() (internal: capture failed)",
                method
            ),
            _ => format!(
                "WARNING: first-class callable creation for ->{}() requires a simple receiver (\\$variable or \\$this); complex receiver expressions are not captured yet",
                method
            ),
        },
        CallableTarget::StaticMethod { method, .. } => format!(
            "WARNING: unsupported first-class callable target for static method ::{}() (late-static binding requires \\$this or __elephc_called_class_id in the enclosing frame)",
            method
        ),
        CallableTarget::Function(name) => format!(
            "WARNING: unsupported first-class callable target for function {}()",
            name.as_str()
        ),
    }
}

/// Provides the Capture for method receiver helper used by the first class module.
fn capture_for_method_receiver(
    object: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<(String, PhpType)> {
    // Captures the receiver expression for a method first-class callable.
    // - For variables and `this`, captures the existing variable directly.
    // - For complex expressions, creates a temporary storage slot, emits the
    //   receiver expression, increments its refcount if needed, and stores the
    //   result to the temporary. Returns the temporary name and its inferred type.
    // Returns `None` if the variable lookup or type inference fails.
    match &object.kind {
        ExprKind::Variable(name) => {
            let ty = ctx
                .variables
                .get(name)
                .map(|var| var.ty.clone())
                .unwrap_or_else(|| crate::codegen::functions::infer_contextual_type(object, ctx));
            Some((name.clone(), ty))
        }
        ExprKind::This => ctx
            .variables
            .get("this")
            .map(|var| ("this".to_string(), var.ty.clone())),
        _ => {
            let temp_name = method_receiver_temp_name(object.span);
            let receiver_static_ty = crate::codegen::functions::infer_contextual_type(object, ctx);
            let receiver_ty = crate::codegen::expr::emit_expr(object, emitter, ctx, data);
            if receiver_ty.is_refcounted()
                && super::super::expr_result_heap_ownership(object) != HeapOwnership::Owned
            {
                abi::emit_incref_if_refcounted(emitter, &receiver_ty);
            }
            let Some(temp_offset) = ctx.variables.get(&temp_name).map(|info| info.stack_offset) else {
                emitter.comment(&format!(
                    "WARNING: missing first-class callable receiver temp ${}",
                    temp_name
                ));
                return None;
            };
            abi::emit_store(emitter, &receiver_ty, temp_offset);
            ctx.update_var_type_static_and_ownership(
                &temp_name,
                receiver_ty.codegen_repr(),
                receiver_static_ty,
                HeapOwnership::local_owner_for_type(&receiver_ty),
            );
            Some((temp_name, receiver_ty))
        }
    }
}

/// Provides the Normalized target and captures helper used by the first class module.
fn normalized_target_and_captures(
    target: &CallableTarget,
    sig: &FunctionSig,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<(
    CallableTarget,
    Vec<(String, PhpType, bool)>,
    Vec<(String, PhpType, bool)>,
)> {
    // Normalizes a callable target and computes its captures and hidden parameters.
    // - For `Static` receiver with late-static binding: captures `__elephc_called_class_id`
    //   or `this` as the visible capture and adds a hidden param for the receiver.
    // - For `Self_`/`Parent` receiver: resolves to concrete class name, no captures needed.
    // - For instance method: captures the receiver variable or temporary, adds a hidden
    //   param to pass the captured value when the wrapper is called.
    // Returns `None` if static resolution fails or receiver capture is impossible.
    match target {
        CallableTarget::StaticMethod { receiver, method } => match receiver {
            StaticReceiver::Static => {
                let capture = capture_for_static_target(ctx)?;
                let hidden_name = if capture.0 == "this" {
                    FCC_THIS_PARAM.to_string()
                } else {
                    FCC_CALLED_CLASS_ID_PARAM.to_string()
                };
                let hidden_ty = capture.1.clone();
                Some((
                    CallableTarget::StaticMethod {
                        receiver: StaticReceiver::Static,
                        method: method.clone(),
                    },
                    vec![(capture.0, capture.1, false)],
                    vec![(hidden_name, hidden_ty, false)],
                ))
            }
            _ => {
                let receiver = resolved_static_callable_target(receiver, ctx)?;
                Some((
                    CallableTarget::StaticMethod {
                        receiver,
                        method: method.clone(),
                    },
                    Vec::new(),
                    Vec::new(),
                ))
            }
        },
        CallableTarget::Method { object, method } => {
            let capture = capture_for_method_receiver(object, emitter, ctx, data)?;
            let hidden_name = unique_hidden_param(FCC_RECEIVER_PARAM, sig);
            let hidden_ty = capture.1.clone();
            Some((
                CallableTarget::Method {
                    object: Box::new(Expr::new(
                        ExprKind::Variable(hidden_name.clone()),
                        object.span,
                    )),
                    method: method.clone(),
                },
                vec![(capture.0, capture.1, false)],
                vec![(hidden_name, hidden_ty, false)],
            ))
        }
        other => Some((other.clone(), Vec::new(), Vec::new())),
    }
}

/// Builds the synthetic method body for wrapper.
fn wrapper_body(target: &CallableTarget, sig: &FunctionSig) -> Vec<Stmt> {
    // Builds the AST body for a first-class callable wrapper function.
    // Creates parameter variables, forwards them as arguments to the underlying call,
    // and wraps the result in a return statement. For void return types, emits an
    // expression statement followed by an empty return to satisfy the ABI.
    let last_param_idx = sig.params.len().saturating_sub(1);
    let args: Vec<Expr> = sig
        .params
        .iter()
        .enumerate()
        .map(|(idx, (name, _))| {
            let var_expr = Expr::new(ExprKind::Variable(name.clone()), crate::span::Span::dummy());
            if sig.variadic.is_some() && idx == last_param_idx {
                Expr::new(
                    ExprKind::Spread(Box::new(var_expr)),
                    crate::span::Span::dummy(),
                )
            } else {
                var_expr
            }
        })
        .collect();

    let call_expr = match target {
        CallableTarget::Function(name) => Expr::new(
            ExprKind::FunctionCall {
                name: name.clone(),
                args,
            },
            crate::span::Span::dummy(),
        ),
        CallableTarget::StaticMethod { receiver, method } => Expr::new(
            ExprKind::StaticMethodCall {
                receiver: receiver.clone(),
                method: method.clone(),
                args,
            },
            crate::span::Span::dummy(),
        ),
        CallableTarget::Method { object, method } => Expr::new(
            ExprKind::MethodCall {
                object: object.clone(),
                method: method.clone(),
                args,
            },
            crate::span::Span::dummy(),
        ),
    };

    if sig.return_type == PhpType::Void {
        vec![
            Stmt::new(StmtKind::ExprStmt(call_expr), crate::span::Span::dummy()),
            Stmt::new(StmtKind::Return(None), crate::span::Span::dummy()),
        ]
    } else {
        vec![Stmt::new(
            StmtKind::Return(Some(call_expr)),
            crate::span::Span::dummy(),
        )]
    }
}

/// Emits first-class callable creation for functions, methods, and builtins.
pub(super) fn emit_first_class_callable(
    target: &CallableTarget,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let Some(base_sig) = first_class_callable_sig(target, ctx) else {
        emitter.comment("WARNING: unsupported first-class callable target");
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
        return PhpType::Callable;
    };
    let sig = ctx
        .expected_first_class_callable_sig
        .clone()
        .unwrap_or(base_sig);

    let Some((normalized_target, captures, hidden_params)) =
        normalized_target_and_captures(target, &sig, emitter, ctx, data)
    else {
        emitter.comment(&unsupported_fcc_diagnostic(target));
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
        return PhpType::Callable;
    };

    let wrapper_label = ctx.next_label("fcc");
    let param_names: Vec<String> = sig.params.iter().map(|(name, _)| name.clone()).collect();
    let body = wrapper_body(&normalized_target, &sig);

    ctx.deferred_closures.push(DeferredClosure {
        label: wrapper_label.clone(),
        params: param_names,
        body,
        sig,
        captures,
        hidden_params,
        current_class: ctx.current_class.clone(),
        // Safe default: assume the wrapper is reached at runtime. The local
        // assignment site downgrades this to `false` when it can prove the
        // FCC value cannot escape, and `emit_variable` flips it back to `true`
        // if the variable's value is read outside the short-circuit.
        needed: true,
    });

    emitter.comment("first-class callable: load descriptor");
    crate::codegen::callable_descriptor::emit_load_descriptor_address(
        emitter,
        data,
        abi::int_result_reg(emitter),
        &wrapper_label,
        None,
        crate::codegen::callable_descriptor::CALLABLE_DESC_KIND_FIRST_CLASS,
    );
    PhpType::Callable
}
