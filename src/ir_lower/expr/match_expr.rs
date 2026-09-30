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
    let subject_span = subject.span;
    let subject = lower_expr(ctx, subject);
    // A match with no default names the SUBJECT in the error it throws, and by then the subject is
    // an SSA value — nothing an AST can point at. Parking it in a named temp is what lets the
    // message be written as ordinary PHP below. Only when there is no default: a match that cannot
    // fall through pays nothing for it.
    let subject_temp = default.is_none().then(|| {
        let subject_type = ctx.builder.value_php_type(subject.value);
        // An ordinary PHP local, NOT `declare_owned_hidden_temp`: that one is one-shot, and its
        // read is treated as an owning temporary the consumer releases. The message below reads
        // the subject many times — a predicate, then the value — so the first read freed what the
        // rest saw: `match ($mixed)` spelled every subject `NULL` or `''`, and a float as its
        // raw bit pattern. This slot retains on store and is released once at function exit.
        let name = ctx.declare_synthetic_php_local(subject_type.clone());
        store_value_into_temp(ctx, &name, subject_type.clone(), subject, subject_span);
        (name, subject_type)
    });
    let result_type = match_merge_result_type(ctx, arms, default, expr);
    let temp_name = ctx.declare_owned_hidden_temp(result_type.clone());
    let merge = ctx.builder.create_named_block("match.merge", Vec::new());

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
        store_expr_into_temp(ctx, &temp_name, result_type.clone(), result, expr.span);
        branch_to(ctx, merge);
        if let Some(fallthrough) = fallthrough {
            ctx.builder.position_at_end(fallthrough);
        }
    }
    if let Some(default) = default {
        store_expr_into_temp(ctx, &temp_name, result_type.clone(), default, expr.span);
        branch_to(ctx, merge);
    } else if !ctx.builder.insertion_block_is_terminated() {
        let (subject_temp, subject_type) =
            subject_temp.expect("a default-less match parks its subject");
        lower_unhandled_match_throw(ctx, &subject_temp, &subject_type, expr.span);
    }
    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp_name, expr.span)
}


/// Builds `$name` for a hidden temp the lowering owns.
fn var_expr(name: &str, span: Span) -> Expr {
    Expr::new(ExprKind::Variable(name.to_string()), span)
}

/// Builds a PHP string literal.
fn str_expr(value: &str, span: Span) -> Expr {
    Expr::new(ExprKind::StringLiteral(value.to_string()), span)
}

/// Builds a PHP integer literal.
fn int_expr(value: i64, span: Span) -> Expr {
    Expr::new(ExprKind::IntLiteral(value), span)
}

/// Builds a call to a builtin by name.
fn call_expr(name: &str, args: Vec<Expr>, span: Span) -> Expr {
    Expr::new(
        ExprKind::FunctionCall {
            name: crate::names::Name::unqualified(name),
            args,
        },
        span,
    )
}

/// Builds `$left . $right`.
fn concat_expr(left: Expr, right: Expr, span: Span) -> Expr {
    Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(left),
            op: BinOp::Concat,
            right: Box::new(right),
        },
        span,
    )
}

/// Builds `$condition ? $then : $otherwise`.
fn ternary_expr(condition: Expr, then_expr: Expr, otherwise: Expr, span: Span) -> Expr {
    Expr::new(
        ExprKind::Ternary {
            condition: Box::new(condition),
            then_expr: Box::new(then_expr),
            else_expr: Box::new(otherwise),
        },
        span,
    )
}

/// Throws php's `UnhandledMatchError` for a `match` that no arm and no default answered.
///
/// php makes this CATCHABLE: `try { match (99) { 1 => "a" }; } catch (Throwable $e)` prints
/// `UnhandledMatchError: Unhandled match case 99` and the program carries on. elephc ended the
/// process with `Terminator::Fatal`, so the `catch` never ran and every statement after it was
/// lost. `builtin_class_gate::UNRAISED_THROWABLES` named this as the reason the class had no
/// producer at all; it has one now.
///
/// The message is composed as ORDINARY PHP rather than a runtime routine, because php's wording is
/// a per-type shape that `var_export` already matches exactly for the four scalar kinds — `1.0`
/// keeps its `.0` and `null` prints `NULL`, neither of which a string cast gives. What `var_export`
/// does NOT match is the rest of the table, measured on `php -n` 8.5.6:
///
/// ```text
/// 99          Unhandled match case 99                      var_export
/// 1.0         Unhandled match case 1.0                     var_export
/// true        Unhandled match case true                    var_export
/// null        Unhandled match case NULL                    var_export
/// "it's"      Unhandled match case 'it's'                  quoted, and NOT escaped as var_export escapes
/// 40 z's      Unhandled match case 'zzzzzzzzzzzzzzz...'    cut at 15, ellipsis INSIDE the quotes
/// [1, 2]      Unhandled match case of type array
/// stdClass    Unhandled match case of type stdClass        the RUNTIME class, so `get_class`
/// STDIN       Unhandled match case of type resource
/// ```
///
/// The same ladder written by hand compiles and runs under elephc, producing that table byte for
/// byte, which is what made building it out of builtins the cheap answer here: a runtime routine
/// would have needed a tag dispatch, a class-name lookup and php's float spelling, twice, once per
/// architecture.
fn lower_unhandled_match_throw(
    ctx: &mut LoweringContext<'_, '_>,
    subject_temp: &str,
    subject_type: &PhpType,
    span: Span,
) {
    let label = match_case_label_expr(subject_temp, subject_type, span);
    crate::ir_lower::stmt::lower_throw_builtin_with_message(
        ctx,
        "UnhandledMatchError",
        concat_expr(str_expr("Unhandled match case ", span), label, span),
        span,
    );
}

/// The expression that spells the subject the way php spells it in this message.
///
/// Chosen by the subject's STATIC type rather than always emitting the full ladder, because a
/// ladder is lowered WHOLE: every arm reaches the backend even when its guard is a constant, and
/// `strlen($v)` on an `int` subject is refused outright (`strlen cannot lower checked operand type
/// Int`). Only a subject that really is dynamic gets the run-time tests.
///
/// Every spelling is built from ordinary builtins rather than `var_export`, which spells all four
/// scalars exactly right and cannot be used: it is an injected PRELUDE, gated on the DETECTOR
/// finding a call in the source, and this call is synthesised during lowering long after that runs.
/// Teaching the detector about a default-less `match` would have bound the message to whatever
/// `var_export` the program has — and a program that declares its OWN suppresses the prelude, so
/// php's wording would have become the user's.
fn match_case_label_expr(subject_temp: &str, subject_type: &PhpType, span: Span) -> Expr {
    let subject = || var_expr(subject_temp, span);
    // The PHP type, NOT `codegen_repr()`: that maps a resource onto its machine representation,
    // so `match (STDIN)` took the integer arm and spelled the subject `Resource id #1` — the string
    // cast — where php says `of type resource`.
    match subject_type {
        PhpType::Str => quoted_string_label(subject_temp, span),
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => {
            str_expr("of type array", span)
        }
        PhpType::Object(_) => object_label(subject_temp, span),
        PhpType::Resource(_) => str_expr("of type resource", span),
        PhpType::Int => string_cast(subject(), span),
        PhpType::Bool => bool_label(subject_temp, span),
        PhpType::False => str_expr("false", span),
        PhpType::Void => str_expr("NULL", span),
        PhpType::Float => float_label(subject_temp, span),
        // Mixed, a union, and anything this list does not name: ask at RUN TIME. A wrong static
        // guess here would be a silently wrong message; a construct the ladder cannot lower is a
        // loud refusal instead.
        _ => ternary_expr(
            call_expr("is_string", vec![subject()], span),
            quoted_string_label(subject_temp, span),
            ternary_expr(
                call_expr("is_array", vec![subject()], span),
                str_expr("of type array", span),
                ternary_expr(
                    call_expr("is_object", vec![subject()], span),
                    object_label(subject_temp, span),
                    ternary_expr(
                        call_expr("is_resource", vec![subject()], span),
                        str_expr("of type resource", span),
                        ternary_expr(
                            call_expr("is_int", vec![subject()], span),
                            string_cast(subject(), span),
                            ternary_expr(
                                call_expr("is_bool", vec![subject()], span),
                                bool_label(subject_temp, span),
                                ternary_expr(
                                    call_expr("is_float", vec![subject()], span),
                                    float_label(subject_temp, span),
                                    // Nothing else is left: php has no other value kind here.
                                    str_expr("NULL", span),
                                    span,
                                ),
                                span,
                            ),
                            span,
                        ),
                        span,
                    ),
                    span,
                ),
                span,
            ),
            span,
        ),
    }
}

/// `(string) $expr`.
fn string_cast(expr: Expr, span: Span) -> Expr {
    Expr::new(
        ExprKind::Cast {
            target: CastType::String,
            expr: Box::new(expr),
        },
        span,
    )
}

/// `of type ` . get_class($v) — php names the RUNTIME class, so the lookup is a call.
fn object_label(subject_temp: &str, span: Span) -> Expr {
    concat_expr(
        str_expr("of type ", span),
        call_expr("get_class", vec![var_expr(subject_temp, span)], span),
        span,
    )
}

/// `$v ? 'true' : 'false'` — php spells a bool as the keyword, where a string cast gives `1`/``.
fn bool_label(subject_temp: &str, span: Span) -> Expr {
    ternary_expr(
        var_expr(subject_temp, span),
        str_expr("true", span),
        str_expr("false", span),
        span,
    )
}

/// php's float spelling, which is the string cast with `.0` restored on an integral value.
///
/// MEASURED on `php -n` 8.5.6, against the cast for the same value:
///
/// ```text
///  1.0        -> 1.0          cast 1
/// -2.0        -> -2.0         cast -2
///  0.0        -> 0.0          cast 0
///  1.5        -> 1.5          cast 1.5           already carries a point
///  1.0E+25    -> 1.0E+25      cast 1.0E+25       already carries a point
///  1.0E-7     -> 1.0E-7       cast 1.0E-7
///  INF/-INF   -> INF/-INF     cast INF/-INF
///  NAN        -> NAN          cast raises `unexpected NAN value was coerced to string`
/// ```
///
/// NAN is tested FIRST because of that last line: php composes this message without warning, so
/// letting the cast see a NAN would have added a warning php never prints.
fn float_label(subject_temp: &str, span: Span) -> Expr {
    let subject = || var_expr(subject_temp, span);
    let cast = || string_cast(subject(), span);
    let with_point = ternary_expr(
        Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(call_expr(
                    "str_contains",
                    vec![cast(), str_expr(".", span)],
                    span,
                )),
                op: BinOp::Or,
                right: Box::new(call_expr(
                    "str_contains",
                    vec![cast(), str_expr("E", span)],
                    span,
                )),
            },
            span,
        ),
        cast(),
        concat_expr(cast(), str_expr(".0", span), span),
        span,
    );
    ternary_expr(
        call_expr("is_nan", vec![subject()], span),
        str_expr("NAN", span),
        ternary_expr(
            call_expr("is_infinite", vec![subject()], span),
            ternary_expr(
                Expr::new(
                    ExprKind::BinaryOp {
                        left: Box::new(subject()),
                        op: BinOp::Gt,
                        right: Box::new(int_expr(0, span)),
                    },
                    span,
                ),
                str_expr("INF", span),
                str_expr("-INF", span),
                span,
            ),
            with_point,
            span,
        ),
        span,
    )
}

/// `'` . (strlen($v) > 15 ? substr($v, 0, 15) . '...' : $v) . `'`
///
/// php quotes the string and cuts it at 15 characters with an ellipsis INSIDE the quotes, and does
/// NOT escape what is inside — `"it's"` prints as `'it's'`, which is why `var_export` cannot serve
/// this arm even though it serves every scalar one.
fn quoted_string_label(subject_temp: &str, span: Span) -> Expr {
    let subject = || var_expr(subject_temp, span);
    concat_expr(
        concat_expr(
            str_expr("'", span),
            ternary_expr(
                Expr::new(
                    ExprKind::BinaryOp {
                        left: Box::new(call_expr("strlen", vec![subject()], span)),
                        op: BinOp::Gt,
                        right: Box::new(int_expr(15, span)),
                    },
                    span,
                ),
                concat_expr(
                    call_expr(
                        "substr",
                        vec![subject(), int_expr(0, span), int_expr(15, span)],
                        span,
                    ),
                    str_expr("...", span),
                    span,
                ),
                subject(),
                span,
            ),
            span,
        ),
        str_expr("'", span),
        span,
    )
}
