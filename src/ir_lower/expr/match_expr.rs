//! Purpose:
//! Lazy match-expression lowering.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers a match expression with lazy arm-result evaluation.
pub(super) fn lower_match(
    ctx: &mut LoweringContext<'_, '_>,
    subject: &Expr,
    arms: &[(Vec<Expr>, Expr)],
    default: Option<&Expr>,
    expr: &Expr,
) -> LoweredValue {
    let subject_expr = subject;
    let subject = lower_expr(ctx, subject);
    // A match without `default` throws PHP's catchable `UnhandledMatchError` naming the subject;
    // its message is built from a synthetic local holding the already-evaluated subject.
    let unhandled_subject = default.is_none().then(|| {
        let local_type = normalize_value_php_type(ctx.builder.value_php_type(subject.value));
        let temp = ctx.declare_synthetic_php_local(local_type.clone());
        let stored = crate::ir_lower::ownership::acquire_if_refcounted(ctx, subject, Some(subject_expr.span));
        ctx.store_local(&temp, stored, local_type, Some(subject_expr.span));
        temp
    });
    let merge = ctx.builder.create_named_block("match.merge", Vec::new());
    // Every arm result is parked and the merge temp is typed from the lowered arm values (see
    // `PendingBranchValue`), so an arm the pre-lowering estimate cannot name keeps its value.
    let mut pending = Vec::new();

    for (conditions, result) in arms {
        let result_block = ctx.builder.create_named_block("match.result", Vec::new());
        let mut fallthrough = ctx.builder.insertion_block();
        for condition in conditions {
            let next_test = ctx.builder.create_named_block("match.next", Vec::new());
            let condition = lower_expr(ctx, condition);
            let matched = ctx.emit_value(
                Op::StrictEq,
                vec![subject.value, condition.value],
                None,
                PhpType::Bool,
                Op::StrictEq.default_effects(),
                Some(expr.span),
            );
            ctx.builder.terminate(Terminator::CondBr {
                cond: matched.value,
                then_target: result_block,
                then_args: Vec::new(),
                else_target: next_test,
                else_args: Vec::new(),
            });
            ctx.builder.position_at_end(next_test);
            fallthrough = Some(next_test);
        }
        ctx.builder.position_at_end(result_block);
        pending.extend(lower_pending_branch(ctx, result));
        if let Some(fallthrough) = fallthrough {
            ctx.builder.position_at_end(fallthrough);
        }
    }
    if let Some(default) = default {
        pending.extend(lower_pending_branch(ctx, default));
    } else if let Some(temp) = unhandled_subject.as_deref() {
        if !ctx.builder.insertion_block_is_terminated() {
            let subject_type = ctx.local_type(temp).codegen_repr();
            let mut enum_names: Vec<String> =
                ctx.enums.keys().map(|name| name.trim_start_matches('\\').to_string()).collect();
            enum_names.sort();
            let throw = unhandled_match_error_throw(temp, &subject_type, &enum_names, expr.span);
            lower_expr(ctx, &throw);
            if !ctx.builder.insertion_block_is_terminated() {
                ctx.builder.terminate(Terminator::Unreachable);
            }
        }
    }
    let (temp_name, _) = store_pending_branches(ctx, pending, merge, Some(expr), None, expr.span);
    ctx.builder.position_at_end(merge);
    // A matched subject is released here, as PHP releases the temporary, rather than at frame exit.
    if let Some(temp) = unhandled_subject.as_deref() {
        let null = lower_null(ctx, expr);
        ctx.unset_local(temp, null, None);
    }
    take_owned_temp(ctx, &temp_name, expr.span)
}

/// Builds `throw new UnhandledMatchError("Unhandled match case " . <subject>)` over the local
/// `temp`, rendering the subject the way PHP 8.5's `smart_str_append_scalar` does:
/// - an int as its digits; a float as its `precision`-14 string form with `.0` appended to an
///   integral finite value (`1.0`, `-0.0`, `1.0E+15`), and `NAN`/`INF`/`-INF`;
/// - a string in single quotes, truncated to 15 bytes plus `...`, with `\\` and the named control
///   escapes (`\n`, `\r`, `\t`, `\f`, `\v`, `\e`) applied;
/// - `true`/`false`, `NULL`, an enum case as `Class::Case`, and `of type <class>` / `of type array`.
///
/// Only the branches the subject's static representation can reach are built, so a typed subject
/// never lowers a builtin or a property read over an impossible operand type.
fn unhandled_match_error_throw(
    temp: &str,
    subject_type: &PhpType,
    enum_names: &[String],
    span: Span,
) -> Expr {
    let e = |kind: ExprKind| Expr::new(kind, span);
    let var = || e(ExprKind::Variable(temp.to_string()));
    let lit = |text: &str| e(ExprKind::StringLiteral(text.to_string()));
    let int = |value: i64| e(ExprKind::IntLiteral(value));
    let call = |function: &str, args: Vec<Expr>| {
        e(ExprKind::FunctionCall { name: Name::unqualified(function), args })
    };
    let on_var = |function: &str| call(function, vec![var()]);
    let binary = |left: Expr, op: BinOp, right: Expr| {
        e(ExprKind::BinaryOp { left: Box::new(left), op, right: Box::new(right) })
    };
    let concat = |left: Expr, right: Expr| binary(left, BinOp::Concat, right);
    let not = |inner: Expr| e(ExprKind::Not(Box::new(inner)));
    let ternary = |condition: Expr, then_expr: Expr, else_expr: Expr| {
        e(ExprKind::Ternary {
            condition: Box::new(condition),
            then_expr: Box::new(then_expr),
            else_expr: Box::new(else_expr),
        })
    };
    let as_string = || e(ExprKind::Cast { target: CastType::String, expr: Box::new(var()) });

    let render_int = || as_string();
    let render_float = || {
        let contains = |needle: &str| call("str_contains", vec![as_string(), lit(needle)]);
        ternary(
            on_var("is_nan"),
            lit("NAN"),
            ternary(
                binary(
                    binary(on_var("is_finite"), BinOp::And, not(contains("."))),
                    BinOp::And,
                    not(contains("E")),
                ),
                concat(as_string(), lit(".0")),
                as_string(),
            ),
        )
    };
    let render_string = || {
        let long = binary(on_var("strlen"), BinOp::Gt, int(15));
        let head = ternary(long.clone(), call("substr", vec![var(), int(0), int(15)]), var());
        let escaped = [
            ("\\", "\\\\"),
            ("\n", "\\n"),
            ("\r", "\\r"),
            ("\t", "\\t"),
            ("\x0c", "\\f"),
            ("\x0b", "\\v"),
            ("\x1b", "\\e"),
        ]
        .into_iter()
        .fold(head, |subject, (search, replace)| {
            call("str_replace", vec![lit(search), lit(replace), subject])
        });
        concat(concat(lit("'"), escaped), ternary(long, lit("...'"), lit("'")))
    };
    let render_bool = || ternary(var(), lit("true"), lit("false"));
    let enum_case = |class: Expr| concat(concat(class, lit("::")), e(ExprKind::PropertyAccess {
        object: Box::new(var()),
        property: "name".to_string(),
    }));
    let render_object = |with_enums: bool| {
        let of_type = concat(lit("of type "), on_var("get_class"));
        if !with_enums || enum_names.is_empty() {
            return of_type;
        }
        let names = e(ExprKind::ArrayLiteral(enum_names.iter().map(|name| lit(name)).collect()));
        ternary(
            call("in_array", vec![on_var("get_class"), names, e(ExprKind::BoolLiteral(true))]),
            enum_case(on_var("get_class")),
            of_type,
        )
    };

    let rendered = match subject_type {
        PhpType::Int => render_int(),
        PhpType::Float => render_float(),
        PhpType::Str => render_string(),
        PhpType::Bool | PhpType::False => render_bool(),
        PhpType::Void | PhpType::Never => lit("NULL"),
        PhpType::Array(_) | PhpType::AssocArray { .. } => lit("of type array"),
        PhpType::Object(class) if enum_names.iter().any(|name| name == class.trim_start_matches('\\')) => {
            enum_case(lit(class.trim_start_matches('\\')))
        }
        PhpType::Object(_) => render_object(false),
        _ => ternary(
            on_var("is_string"),
            render_string(),
            ternary(
                on_var("is_int"),
                render_int(),
                ternary(
                    on_var("is_float"),
                    render_float(),
                    ternary(
                        on_var("is_bool"),
                        render_bool(),
                        ternary(
                            on_var("is_null"),
                            lit("NULL"),
                            ternary(on_var("is_object"), render_object(true), lit("of type array")),
                        ),
                    ),
                ),
            ),
        ),
    };
    e(ExprKind::Throw(Box::new(e(ExprKind::NewObject {
        class_name: Name::from("UnhandledMatchError".to_string()),
        args: vec![concat(lit("Unhandled match case "), rendered)],
    }))))
}
