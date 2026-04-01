use crate::codegen::context::Context;
use crate::parser::ast::{CallableTarget, Expr, ExprKind, StaticReceiver, StmtKind};
use crate::types::PhpType;

use super::array_map_expr_is_str::expr_is_str;

/// Infer whether a callback returns a string type from its AST.
pub(super) fn callback_returns_str(args: &[Expr], ctx: &Context) -> bool {
    match &args[0].kind {
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
                    StaticReceiver::Self_ => ctx.current_class.clone(),
                    StaticReceiver::Parent => ctx
                        .current_class
                        .as_ref()
                        .and_then(|class_name| ctx.classes.get(class_name))
                        .and_then(|class_info| class_info.parent.clone()),
                    StaticReceiver::Static => None,
                };
                class_name
                    .as_ref()
                    .and_then(|class_name| ctx.classes.get(class_name))
                    .and_then(|class_info| class_info.static_methods.get(method))
                    .map(|sig| sig.return_type == PhpType::Str)
                    .unwrap_or(false)
            }
            CallableTarget::Method { .. } => false,
        },
        _ => false,
    }
}
