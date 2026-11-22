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
//!   the checker has no file identity for a span, lowering has the path table. A callee php
//!   implements internally (a prelude stand-in such as GD's `imagepng()`) gets no clause.
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
    let (message, internal_callee, callee_decl) = args.iter().find_map(|arg| {
        let inner = match &arg.kind {
            ExprKind::NamedArg { value, .. } => value,
            _ => arg,
        };
        [arg.span, inner.span].into_iter().find_map(|span| {
            match ctx.throw_access_sites.get(&(scope.clone(), span)).map(|info| &info.kind) {
                Some(crate::types::ThrowAccessKind::CallArgumentType {
                    message,
                    internal_callee,
                    callee_decl,
                }) => Some((message.clone(), *internal_callee, *callee_decl)),
                _ => None,
            }
        })
    })?;
    // php binds the parameter inside the callee, so the error carries the callee's frame.
    if let (Some(decl), ExprKind::FunctionCall { .. }) = (callee_decl, &expr.kind) {
        if let Some(thrown) = lower_through_callee_frame(ctx, expr, args, &message, decl) {
            return Some(thrown);
        }
    }
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
    // php names no call site for a function it implements internally (`imagepng(123)`).
    let message = if internal_callee {
        message
    } else {
        let file = call_site_file(ctx, expr.span);
        format!("{}, called in {} on line {}", message, file, expr.span.line)
    };
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

/// Name prefix of the generated functions that raise a binding `TypeError` in the callee's frame.
pub(crate) const BIND_ERROR_THUNK_PREFIX: &str = "__elephc_bind_error_";

/// Lowers a definite argument `TypeError` of a user function the way php raises it: inside the
/// callee (see [`call_binding_error_thunk`]).
///
/// `None` keeps the call-site throw for shapes the thunk cannot mirror (named or spread
/// arguments).
fn lower_through_callee_frame(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
    args: &[Expr],
    message: &str,
    decl: Span,
) -> Option<LoweredValue> {
    if args.is_empty()
        || args.iter().any(|arg| matches!(arg.kind, ExprKind::NamedArg { .. } | ExprKind::Spread(_)))
    {
        return None;
    }
    let (callee_label, _) = message.split_once("(): ")?;
    let file = call_site_file(ctx, expr.span);
    let full_message = format!("{}, called in {} on line {}", message, file, expr.span.line);
    let operands: Vec<crate::ir::ValueId> = args
        .iter()
        .map(|arg| {
            let value = lower_expr(ctx, arg);
            ctx.box_value_as_mixed(value, PhpType::Mixed, Some(arg.span)).value
        })
        .collect();
    Some(call_binding_error_thunk(ctx, expr.span, callee_label, &full_message, decl, &operands))
}

/// Raises a binding `TypeError` from a generated function named and located like the callee, so
/// the exception carries php's callee frame (`#0 F(3): f('x')`, `thrown in F on line
/// <declaration>`). The thunk takes one `mixed` parameter per operand and throws; the call site
/// only calls it. Operands must already be boxed `mixed` values.
pub(super) fn call_binding_error_thunk(
    ctx: &mut LoweringContext<'_, '_>,
    call_span: Span,
    callee_label: &str,
    full_message: &str,
    decl: Span,
    operands: &[crate::ir::ValueId],
) -> LoweredValue {
    let site = Span::new_in_source(decl.line, decl.col.max(1), decl.source_id());
    let params: Vec<(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)> =
        (0..operands.len())
            .map(|index| {
                (
                    format!("arg{index}"),
                    Some(crate::parser::ast::TypeExpr::Named(crate::names::Name::unqualified("mixed"))),
                    None,
                    false,
                )
            })
            .collect();
    let error = Expr::new(
        ExprKind::NewObject {
            class_name: crate::names::Name::unqualified("TypeError"),
            args: vec![Expr::new(ExprKind::StringLiteral(full_message.to_string()), site)],
        },
        site,
    );
    let body = vec![crate::synthetic_class::s_throw(error)];
    let name = format!("{}{}", BIND_ERROR_THUNK_PREFIX, ctx.next_closure_name());
    ctx.set_closure_display_label(&name, callee_label.to_string());
    let loop_storage_scope = crate::types::nested_loop_storage_scope(&ctx.loop_storage_scope, site);
    let _signature = crate::ir_lower::function::lower_closure_function(
        ctx,
        &name,
        &params,
        None,
        false,
        None,
        &body,
        &[],
        &[],
        &[],
        None,
        false,
        true,
        loop_storage_scope,
        false,
    );
    let data = ctx.intern_function_name(&name);
    ctx.emit_void(
        Op::Call,
        operands.to_vec(),
        Some(Immediate::Data(data)),
        Op::Call.default_effects(),
        Some(call_span),
    );
    let void_type = crate::ir_lower::context::value_ir_type(&PhpType::Void);
    let placeholder = ctx
        .builder
        .emit_with_effects(
            Op::ConstNull,
            Vec::new(),
            None,
            void_type,
            PhpType::Void,
            Ownership::NonHeap,
            Op::ConstNull.default_effects(),
            Some(call_span),
        )
        .expect("const_null produces a value");
    LoweredValue { value: placeholder, ir_type: void_type }
}
