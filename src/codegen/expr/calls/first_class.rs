use crate::codegen::context::{Context, DeferredClosure};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::names::Name;
use crate::parser::ast::{CallableTarget, Expr, ExprKind, StaticReceiver, Stmt, StmtKind};
use crate::types::{first_class_callable_builtin_sig, FunctionSig, PhpType};

fn callable_wrapper_sig(sig: &FunctionSig) -> FunctionSig {
    let Some(variadic_name) = sig.variadic.as_ref() else {
        return sig.clone();
    };
    if sig
        .params
        .last()
        .is_some_and(|(name, ty)| name == variadic_name && matches!(ty, PhpType::Array(_)))
    {
        return sig.clone();
    }

    let mut wrapper_sig = sig.clone();
    wrapper_sig.params.push((
        variadic_name.clone(),
        PhpType::Array(Box::new(PhpType::Mixed)),
    ));
    wrapper_sig.defaults.push(None);
    wrapper_sig.ref_params.push(false);
    wrapper_sig
}

fn resolved_static_callable_target(
    receiver: &StaticReceiver,
    ctx: &Context,
) -> Option<StaticReceiver> {
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

pub(super) fn first_class_callable_sig(target: &CallableTarget, ctx: &Context) -> Option<FunctionSig> {
    let sig = match target {
        CallableTarget::Function(name) => ctx
            .functions
            .get(name.as_str())
            .cloned()
            .or_else(|| first_class_callable_builtin_sig(name.as_str())),
        CallableTarget::StaticMethod { receiver, method } => {
            let StaticReceiver::Named(class_name) = resolved_static_callable_target(receiver, ctx)? else {
                return None;
            };
            ctx.classes
                .get(class_name.as_str())
                .and_then(|class_info| class_info.static_methods.get(method))
                .cloned()
        }
        CallableTarget::Method { .. } => None,
    }?;

    Some(callable_wrapper_sig(&sig))
}

fn wrapper_body(target: &CallableTarget, sig: &FunctionSig) -> Vec<Stmt> {
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
        CallableTarget::Method { .. } => unreachable!("instance method callables are rejected by checker"),
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

pub(super) fn emit_first_class_callable(
    target: &CallableTarget,
    emitter: &mut Emitter,
    ctx: &mut Context,
    _data: &mut DataSection,
) -> PhpType {
    let Some(sig) = first_class_callable_sig(target, ctx) else {
        emitter.comment("WARNING: unsupported first-class callable target");
        emitter.instruction("mov x0, #0");                                          // unsupported callable lowers to null pointer sentinel
        return PhpType::Callable;
    };

    let normalized_target = match target {
        CallableTarget::StaticMethod { receiver, method } => {
            let Some(receiver) = resolved_static_callable_target(receiver, ctx) else {
                emitter.comment("WARNING: unsupported first-class static:: callable target");
                emitter.instruction("mov x0, #0");                                  // unsupported callable lowers to null pointer sentinel
                return PhpType::Callable;
            };
            CallableTarget::StaticMethod {
                receiver,
                method: method.clone(),
            }
        }
        other => other.clone(),
    };

    let wrapper_label = ctx.next_label("fcc");
    let param_names: Vec<String> = sig.params.iter().map(|(name, _)| name.clone()).collect();
    let body = wrapper_body(&normalized_target, &sig);

    ctx.deferred_closures.push(DeferredClosure {
        label: wrapper_label.clone(),
        params: param_names,
        body,
        sig,
        captures: vec![],
    });

    emitter.comment("first-class callable: load wrapper address");
    emitter.instruction(&format!("adrp x0, {}@PAGE", wrapper_label));               // load page base of synthesized callable wrapper
    emitter.instruction(&format!("add x0, x0, {}@PAGEOFF", wrapper_label));         // resolve callable wrapper address
    PhpType::Callable
}
