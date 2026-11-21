//! Purpose:
//! Applies the shared opaque-capability policy to statically resolved Reflection calls.
//!
//! Called from:
//! - `super::method_calls::lower_method_call()` before reflected-target dispatch.
//!
//! Key details:
//! - Ordinary PHP private reflection remains available; class contracts own the exception.
//! - Source arguments are evaluated once before denial and their owners unwind safely.
//! - Public handle operations and metadata inspection do not expose native capability storage.

use super::*;
use elephc_builtin_contract::{reflection_capability_denial, ReflectionOperation};

/// Returns the shared refusal only for a capability-sensitive reflected target.
pub(super) fn reflected_capability_denial(
    ctx: &LoweringContext<'_, '_>, object: &Expr, method: &str,
) -> Option<String> {
    match php_symbol_key(method).as_str() {
        "invoke" | "invokeargs" | "getclosure" => {
            let (class, target) = reflection_method_reflected_target(ctx, object)?;
            let info = ctx.classes.get(class.trim_start_matches('\\'))?;
            let key = php_symbol_key(&target);
            let visibility = info.static_method_visibilities.get(&key)
                .or_else(|| info.method_visibilities.get(&key));
            let public = visibility.map_or(true, |visibility| *visibility == Visibility::Public);
            reflection_capability_denial(&class, ReflectionOperation::Method { public })
        }
        "getvalue" | "setvalue" | "getrawvalue" | "setrawvalue"
        | "setrawvaluewithoutlazyinitialization" => {
            let (class, _) = reflection_property_reflected_target(ctx, object)?;
            reflection_capability_denial(&class, ReflectionOperation::Storage)
        }
        _ => None,
    }
}

/// Evaluates raw source arguments without invoking the forbidden operation, then throws Error.
pub(super) fn lower_reflection_capability_denial(
    ctx: &mut LoweringContext<'_, '_>, receiver: LoweredValue, args: &[Expr],
    message: &str, expr: &Expr,
) -> LoweredValue {
    let (_, receiver_owner) = root_owned_call_operand(ctx, receiver, expr.span);
    let mut owners = Vec::new();
    for arg in args {
        let value = match &arg.kind {
            ExprKind::NamedArg { value, .. } | ExprKind::Spread(value) => value.as_ref(),
            _ => arg,
        };
        let value = lower_expr(ctx, value);
        let (_, owner) = root_owned_call_operand(ctx, value, arg.span);
        if let Some(owner) = owner { owners.push((owner, arg.span)); }
    }
    for (owner, span) in owners.into_iter().rev() { retire_owned_call_operand(ctx, owner, span); }
    if let Some(owner) = receiver_owner { retire_owned_call_operand(ctx, owner, expr.span); }
    crate::ir_lower::stmt::lower_throw_access_error_expr(ctx, message, expr.span)
}
