//! Purpose:
//! Lowers unresolved direct function calls to PHP-compatible catchable `Error` throws.
//!
//! Called from:
//! - `crate::ir_lower::expr::function_calls::lower_function_call()` before argument lowering.
//!
//! Key details:
//! - PHP resolves an absent function before evaluating its arguments, so this path emits no
//!   argument side effects.
//! - The unreachable placeholder is boxed `Mixed` so downstream EIR remains well typed.

use crate::names::Name;
use crate::parser::ast::{Expr, ExprKind};

use super::super::context::{LoweredValue, LoweringContext};

/// Resolves a compiled runtime binding before evaluating any call arguments.
pub(super) fn guard_runtime_function_binding(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    call: &Expr,
) {
    if !ctx.runtime_bound_functions.contains(&crate::names::php_symbol_key(name)) {
        return;
    }
    use crate::parser::ast::{Stmt, StmtKind};
    let span = call.span;
    let condition = Expr::new(ExprKind::FunctionCall {
        name: Name::unqualified("function_exists"),
        args: vec![Expr::new(ExprKind::StringLiteral(name.to_string()), span)],
    }, span);
    let error = Expr::new(ExprKind::NewObject {
        class_name: Name::unqualified("Error"),
        args: vec![Expr::new(ExprKind::StringLiteral(
            format!("Call to undefined function {}()", name),
        ), span)],
    }, span);
    let guard = Stmt::new(StmtKind::If {
        condition,
        then_body: Vec::new(),
        elseif_clauses: Vec::new(),
        else_body: Some(vec![Stmt::new(StmtKind::Throw(error), span)]),
    }, span);
    super::super::stmt::lower_stmt(ctx, &guard);
}

/// Lowers one unresolved direct call to `throw new Error(...)`, or returns `None` otherwise.
pub(super) fn lower_late_bound_undefined_call(
    ctx: &mut LoweringContext<'_, '_>,
    canonical_name: &str,
    call_expr: &Expr,
) -> Option<LoweredValue> {
    if !crate::types::checker::builtins::is_late_bound_undefined_function(canonical_name) {
        return None;
    }
    let span = call_expr.span;
    let message = format!("Call to undefined function {}()", canonical_name);
    let message_expr = Expr::new(ExprKind::StringLiteral(message), span);
    let new_error_expr = Expr::new(
        ExprKind::NewObject {
            class_name: Name::unqualified("Error"),
            args: vec![message_expr],
        },
        span,
    );
    let error_value = super::lower_expr(ctx, &new_error_expr);
    ctx.emit_void(
        crate::ir::Op::ThrowException,
        vec![error_value.value],
        None,
        crate::ir::Op::ThrowException.default_effects(),
        Some(span),
    );
    let null = super::lower_null(ctx, call_expr);
    Some(ctx.box_value_as_mixed(null, crate::types::PhpType::Mixed, Some(span)))
}

/// Lowers an `eval()` of a computed string in a closed-world build to `throw new Error(...)`.
///
/// The build links no interpreter, so nothing could run the string. The throw is catchable like
/// php's own `Error`s, and uncaught it reports a fatal.
pub(super) fn lower_closed_world_eval_unavailable(
    ctx: &mut LoweringContext<'_, '_>,
    call_expr: &Expr,
) -> LoweredValue {
    let span = call_expr.span;
    let message_expr = Expr::new(
        ExprKind::StringLiteral(crate::closed_world::EVAL_UNAVAILABLE_MESSAGE.to_string()),
        span,
    );
    let new_error_expr = Expr::new(
        ExprKind::NewObject {
            class_name: Name::unqualified("Error"),
            args: vec![message_expr],
        },
        span,
    );
    let error_value = super::lower_expr(ctx, &new_error_expr);
    ctx.emit_void(
        crate::ir::Op::ThrowException,
        vec![error_value.value],
        None,
        crate::ir::Op::ThrowException.default_effects(),
        Some(span),
    );
    let null = super::lower_null(ctx, call_expr);
    ctx.box_value_as_mixed(null, crate::types::PhpType::Mixed, Some(span))
}

/// Lowers a static call on a class that is not part of a closed-world build to php's own
/// `Error: Class "X" not found`.
pub(super) fn lower_closed_world_class_not_found(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    call_expr: &Expr,
) -> LoweredValue {
    let span = call_expr.span;
    let message = format!("Class \"{}\" not found", class_name.trim_start_matches('\\'));
    let new_error_expr = Expr::new(
        ExprKind::NewObject {
            class_name: Name::unqualified("Error"),
            args: vec![Expr::new(ExprKind::StringLiteral(message), span)],
        },
        span,
    );
    let error_value = super::lower_expr(ctx, &new_error_expr);
    ctx.emit_void(
        crate::ir::Op::ThrowException,
        vec![error_value.value],
        None,
        crate::ir::Op::ThrowException.default_effects(),
        Some(span),
    );
    let null = super::lower_null(ctx, call_expr);
    ctx.box_value_as_mixed(null, crate::types::PhpType::Mixed, Some(span))
}
