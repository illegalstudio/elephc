//! Purpose:
//! Determines when array_map callback lowering should allocate string element storage.
//! Bridges callable targets, closure metadata, and inferred PHP return types for result arrays.
//!
//! Called from:
//! - `crate::codegen_support::builtins::arrays::array_map::emit()`.
//!
//! Key details:
//! - Return-type guesses must stay conservative so runtime array payload shape remains valid.

use crate::codegen_support::context::Context;
use crate::parser::ast::{CallableTarget, Expr, ExprKind, StaticReceiver, StmtKind};
use crate::types::PhpType;

use super::array_map_expr_is_str::expr_is_str;

/// Infers whether an `array_map` callback returns a PHP `string` type.
///
/// Examines the callback expression's AST to determine if its return type is
/// guaranteed to be `PhpType::Str`. This drives whether the result array's
/// element storage can be sized for string payloads.
///
/// ## Expression handling
///
/// - **Closure**: Scans the body for a terminal `return` statement and delegates
///   to `expr_is_str` on the returned expression. Returns `false` if no return
///   is found (conservative: ambiguous returns default to non-string).
/// - **StringLiteral**: Looks up the function signature by name and checks
///   `return_type == PhpType::Str`.
/// - **Variable**: Looks up the closure signature stored under the variable name
///   in `ctx.closure_sigs` and checks the return type.
/// - **FirstClassCallable**: Dispatches to function, static method, or instance
///   method lookup via `ctx.functions`, `ctx.classes`, and contextual type
///   inference.
///
/// Returns `false` for any unrecognized or dynamic callback form, preserving
/// conservative runtime array layout behavior.
pub(super) fn callback_returns_str(args: &[Expr], ctx: &Context) -> bool {
    callback_expr_returns_str(&args[0], ctx)
}

/// Infers whether one callback expression has a statically string-returning signature.
fn callback_expr_returns_str(callback: &Expr, ctx: &Context) -> bool {
    match &callback.kind {
        ExprKind::Closure { body, .. } => {
            for stmt in body {
                if let StmtKind::Return(Some(expr)) = &stmt.kind {
                    return expr_is_str(expr);
                }
            }
            false
        }
        ExprKind::StringLiteral(name) => {
            if let Some(sig) = ctx.functions.get(name) {
                return sig.return_type == PhpType::Str;
            }
            false
        }
        ExprKind::Variable(name) => ctx
            .closure_sigs
            .get(name)
            .map(|sig| sig.return_type == PhpType::Str)
            .or_else(|| {
                ctx.callable_array_targets
                    .get(name)
                    .map(|target| callable_target_returns_str(target, ctx))
            })
            .or_else(|| {
                ctx.first_class_callable_targets.get(name).map(|target| {
                    crate::codegen_support::expr::calls::first_class_callable_sig(target, ctx)
                        .map(|sig| sig.return_type == PhpType::Str)
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false),
        ExprKind::FirstClassCallable(target) => match target {
            CallableTarget::Function(name) => ctx
                .functions
                .get(name.as_str())
                .map(|sig| sig.return_type == PhpType::Str)
                .unwrap_or(false),
            CallableTarget::StaticMethod { receiver, method } => {
                let class_name = match receiver {
                    StaticReceiver::Named(name) => Some(name.as_str().to_string()),
                    StaticReceiver::Self_ | StaticReceiver::Static => ctx.current_class.clone(),
                    StaticReceiver::Parent => ctx
                        .current_class
                        .as_ref()
                        .and_then(|class_name| ctx.classes.get(class_name))
                        .and_then(|class_info| class_info.parent.clone()),
                };
                class_name
                    .as_ref()
                    .and_then(|class_name| ctx.classes.get(class_name))
                    .and_then(|class_info| class_info.static_methods.get(method))
                    .map(|sig| sig.return_type == PhpType::Str)
                    .unwrap_or(false)
            }
            CallableTarget::Method { object, method } => {
                let object_ty = crate::codegen_support::functions::infer_contextual_type(object, ctx);
                let Some(class_name) = crate::codegen_support::functions::singular_object_class(&object_ty)
                else {
                    return false;
                };
                ctx.classes
                    .get(class_name)
                    .and_then(|class_info| class_info.methods.get(method))
                    .map(|sig| sig.return_type == PhpType::Str)
                    .unwrap_or(false)
            }
        },
        ExprKind::FunctionCall { name, .. } => ctx
            .callable_return_sigs
            .get(name.as_str())
            .map(|sig| sig.return_type == PhpType::Str)
            .unwrap_or(false),
        ExprKind::Assignment { value, .. } => callback_expr_returns_str(value, ctx),
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            callback_expr_returns_str(then_expr, ctx)
                && callback_expr_returns_str(else_expr, ctx)
        }
        ExprKind::ShortTernary { value, default }
        | ExprKind::NullCoalesce { value, default } => {
            callback_expr_returns_str(value, ctx)
                && callback_expr_returns_str(default, ctx)
        }
        _ => false,
    }
}

/// Returns true when a callable-array target has a statically string return type.
fn callable_target_returns_str(target: &CallableTarget, ctx: &Context) -> bool {
    match target {
        CallableTarget::Function(name) => ctx
            .functions
            .get(name.as_str())
            .map(|sig| sig.return_type == PhpType::Str)
            .unwrap_or(false),
        CallableTarget::StaticMethod { receiver, method } => {
            let class_name = match receiver {
                StaticReceiver::Named(name) => Some(name.as_str().to_string()),
                StaticReceiver::Self_ | StaticReceiver::Static => ctx.current_class.clone(),
                StaticReceiver::Parent => ctx
                    .current_class
                    .as_ref()
                    .and_then(|class_name| ctx.classes.get(class_name))
                    .and_then(|class_info| class_info.parent.clone()),
            };
            class_name
                .as_ref()
                .and_then(|class_name| ctx.classes.get(class_name))
                .and_then(|class_info| class_info.static_methods.get(method))
                .map(|sig| sig.return_type == PhpType::Str)
                .unwrap_or(false)
        }
        CallableTarget::Method { object, method } => {
            let object_ty = crate::codegen_support::functions::infer_contextual_type(object, ctx);
            let Some(class_name) = crate::codegen_support::functions::singular_object_class(&object_ty)
            else {
                return false;
            };
            ctx.classes
                .get(class_name)
                .and_then(|class_info| class_info.methods.get(method))
                .map(|sig| sig.return_type == PhpType::Str)
                .unwrap_or(false)
        }
    }
}
