//! Purpose:
//! Models optimizer side effects for calls behavior.
//! Feeds purity, callable alias, builtin, and call-effect decisions into pruning and dead-code elimination.
//!
//! Called from:
//! - `crate::optimize::effects`
//!
//! Key details:
//! - Effect summaries must account for globals, heap/runtime state, output, throws, and by-reference mutation.

use super::*;
use super::builtins::is_pure_non_throwing_builtin;

pub(in crate::optimize) fn function_call_effect(name: &str) -> Effect {
    ACTIVE_FUNCTION_EFFECTS.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|effects| effects.get(name).copied())
    })
    .unwrap_or_else(|| {
        if is_pure_non_throwing_builtin(name) {
            Effect::PURE
        } else {
            Effect::PURE.with_side_effects().with_may_throw()
        }
    })
}

pub(super) fn closure_body_call_effect(body: &[Stmt]) -> Effect {
    block_effect(body)
}

pub(in crate::optimize) fn expr_call_effect(callee: &Expr) -> Effect {
    match &callee.kind {
        ExprKind::FirstClassCallable(target) => callable_target_call_effect(target),
        ExprKind::Closure { body, .. } => closure_body_call_effect(body),
        _ => Effect::PURE.with_side_effects().with_may_throw(),
    }
}

pub(in crate::optimize) fn callable_alias_effect(name: &str) -> Effect {
    ACTIVE_CALLABLE_ALIAS_EFFECTS.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|effects| effects.get(name).copied())
    })
    .unwrap_or_else(|| Effect::PURE.with_side_effects().with_may_throw())
}

pub(super) fn callable_target_call_effect(target: &CallableTarget) -> Effect {
    match target {
        CallableTarget::Function(name) => function_call_effect(name.as_str()),
        CallableTarget::StaticMethod { receiver, method } => static_method_call_effect(receiver, method),
        CallableTarget::Method { object, method } => {
            expr_effect(object).combine(private_instance_method_call_effect(object, method))
        }
    }
}

pub(super) fn closure_alias_effect(expr: &Expr) -> Option<Effect> {
    match &expr.kind {
        ExprKind::Closure { body, .. } => Some(closure_body_call_effect(body)),
        _ => None,
    }
}

pub(super) fn merge_callable_value_effects(
    effects: impl IntoIterator<Item = Option<Effect>>,
) -> Option<Effect> {
    let mut effects = effects.into_iter();
    let first = effects.next().flatten()?;
    if effects.all(|effect| effect == Some(first)) {
        Some(first)
    } else {
        None
    }
}

pub(in crate::optimize) fn static_method_call_effect(
    receiver: &crate::parser::ast::StaticReceiver,
    method_name: &str,
) -> Effect {
    let Some(class_name) = resolve_static_receiver_class(receiver) else {
        return Effect::PURE.with_side_effects().with_may_throw();
    };

    ACTIVE_STATIC_METHOD_EFFECTS.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|effects| effects.get(&method_effect_key(&class_name, method_name)).copied())
    })
    .unwrap_or_else(|| Effect::PURE.with_side_effects().with_may_throw())
}

pub(in crate::optimize) fn private_instance_method_call_effect(object: &Expr, method_name: &str) -> Effect {
    if !matches!(object.kind, ExprKind::This) {
        return Effect::PURE.with_side_effects().with_may_throw();
    }

    let Some(class_name) = ACTIVE_CLASS_EFFECT_CONTEXT
        .with(|slot| slot.borrow().as_ref().map(|context| context.class_name.clone()))
    else {
        return Effect::PURE.with_side_effects().with_may_throw();
    };

    ACTIVE_PRIVATE_INSTANCE_METHOD_EFFECTS.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|effects| effects.get(&method_effect_key(&class_name, method_name)).copied())
    })
    .unwrap_or_else(|| Effect::PURE.with_side_effects().with_may_throw())
}

pub(super) fn resolve_static_receiver_class(receiver: &crate::parser::ast::StaticReceiver) -> Option<String> {
    match receiver {
        crate::parser::ast::StaticReceiver::Named(class_name) => Some(class_name.as_str().to_string()),
        crate::parser::ast::StaticReceiver::Self_ => ACTIVE_CLASS_EFFECT_CONTEXT
            .with(|slot| slot.borrow().as_ref().map(|context| context.class_name.clone())),
        crate::parser::ast::StaticReceiver::Parent => ACTIVE_CLASS_EFFECT_CONTEXT.with(|slot| {
            slot.borrow()
                .as_ref()
                .and_then(|context| context.parent_name.clone())
        }),
        crate::parser::ast::StaticReceiver::Static => None,
    }
}
