//! Purpose:
//! Lowers a call whose argument the checker proved can never bind to its declared parameter
//! (`ThrowAccessKind::CallArgumentType`) into php's run-time behaviour: evaluate the receiver and
//! every argument in source order, then throw `TypeError`.
//!
//! Called from:
//! - `crate::ir_lower::expr::lower_expr_inner`
//!
//! Key details:
//! - php binds parameters when the call RUNS, so the build stands and the throw happens here.
//! - The message gets php's `, called in FILE on line N` clause, which only lowering can name:
//!   the checker has no file identity for a span, lowering has the path table.
//! - The call's value is null after the throw (dead code), like every statically thrown call.

use super::*;

/// Lowers `expr` when one of its arguments carries a deferred definite mismatch, else `None`.
pub(super) fn lower_definite_argument_mismatch(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
) -> Option<LoweredValue> {
    if ctx.throw_access_sites.is_empty() {
        return None;
    }
    let (receiver, args): (Option<&Expr>, &[Expr]) = match &expr.kind {
        ExprKind::FunctionCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. } => (None, args),
        ExprKind::MethodCall { object, args, .. } => (Some(object), args),
        _ => return None,
    };
    let scope = ctx.loop_storage_scope.clone();
    let message = args.iter().find_map(|arg| {
        let inner = match &arg.kind {
            ExprKind::NamedArg { value, .. } => value,
            _ => arg,
        };
        [arg.span, inner.span].into_iter().find_map(|span| {
            match ctx.throw_access_sites.get(&(scope.clone(), span)).map(|info| &info.kind) {
                Some(crate::types::ThrowAccessKind::CallArgumentType { message }) => {
                    Some(message.clone())
                }
                _ => None,
            }
        })
    })?;
    if let Some(receiver) = receiver {
        let value = lower_expr(ctx, receiver);
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(receiver.span));
    }
    let operands = call_arg_coercion::lower_args_with_signature(ctx, None, args);
    nullable_method_calls::release_owned_call_arg_temporaries(
        ctx,
        &operands,
        None,
        &ReturnArgAlias::None,
        expr.span,
    );
    let file = call_site_file(ctx, expr.span);
    let message = format!("{}, called in {} on line {}", message, file, expr.span.line);
    let thrown = crate::ir_lower::stmt::lower_throw_access_class_expr(
        ctx,
        "TypeError",
        &message,
        expr.span,
    );
    // A `new` expression is consumed as an object of its class (returned, stored, passed on), and
    // the consumers convert from the value's declared type; a void placeholder has no such
    // conversion. The null pointer is never reached -- the throw above leaves first.
    if let ExprKind::NewObject { class_name, .. } = &expr.kind {
        let object_type = PhpType::Object(class_name.as_str().trim_start_matches('\\').to_string());
        let ir_type = IrType::from_php(&object_type);
        let placeholder = ctx
            .builder
            .emit_with_effects(
                Op::ConstNull,
                Vec::new(),
                None,
                ir_type,
                object_type,
                Ownership::Borrowed,
                Op::ConstNull.default_effects(),
                Some(expr.span),
            )
            .expect("const_null produces a value");
        return Some(LoweredValue { value: placeholder, ir_type });
    }
    Some(thrown)
}

/// Names the file the call was written in: the entry script's path for the root source, or the
/// included/autoloaded file's recorded path.
pub(super) fn call_site_file(ctx: &LoweringContext<'_, '_>, span: Span) -> String {
    if span.source_id() != 0 {
        if let Some(path) = Span::source_paths_snapshot().get(&span.source_id()) {
            return path.clone();
        }
    }
    ctx.source_path().unwrap_or("").to_string()
}
