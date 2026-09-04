//! Purpose:
//! Builds the plain-PHP expressions that replace `func_num_args()`, `func_get_args()` and
//! `func_get_arg($position)` inside a function scope that received the hidden variadic
//! parameter `mixed ...$__elephc_func_args`.
//!
//! Called from:
//! - `crate::func_args::walk::Rewriter`.
//!
//! Key details:
//! - Mandatory-only scopes derive the count from declared parameters plus the hidden tail.
//!   Optional scopes store the actual PHP argument count as collector metadata, allowing the
//!   rebuilt argument list to omit defaults the caller did not supply.
//! - For a source variadic, `$__elephc_func_args` is an entry snapshot containing only its
//!   positional values. For a non-variadic source function it remains the hidden ABI collector.
//! - `func_get_args()` reports the *current* values of the parameter variables, not the
//!   values originally passed (verified against PHP 8.4: reassigning a parameter, or
//!   writing through a by-reference parameter, changes what `func_get_args()` returns).
//!   Reading the parameter variables at the call point reproduces that exactly.
//! - The array is rebuilt at each use because PHP returns a fresh array every time.
//! - `func_get_arg()` raises `ValueError` — not `ArgumentCountError` — for both a negative
//!   position and a position at or past the argument count, with php-src's two distinct
//!   messages.

use crate::names::{Name, NameKind};
use crate::parser::ast::{BinOp, CastType, Expr, ExprKind};
use crate::span::Span;

use super::{IntrospectionCall, HIDDEN_ARGS_PARAM, POSITION_TEMP};

/// php-src's message when `func_get_arg()` is given a negative position.
const NEGATIVE_POSITION_MESSAGE: &str =
    "func_get_arg(): Argument #1 ($position) must be greater than or equal to 0";

/// php-src's message when `func_get_arg()` is given a position at or past the number of
/// arguments the current call actually passed.
const OUT_OF_RANGE_POSITION_MESSAGE: &str =
    "func_get_arg(): Argument #1 ($position) must be less than the number of the arguments passed to the currently executed function";

/// Builds the replacement expression for one introspection call.
///
/// `param_names` lists the scope's declared regular parameters in order; `args` is the
/// (already rewritten) call-site argument list, which is empty except for
/// `func_get_arg($position)`.
pub(super) fn replacement(
    call: IntrospectionCall,
    param_names: &[String],
    args: &[Expr],
    count_metadata: bool,
    span: Span,
) -> ExprKind {
    match call {
        IntrospectionCall::NumArgs => argc_expr(param_names, count_metadata, span).kind,
        IntrospectionCall::GetArgs => args_array_expr(param_names, count_metadata, span).kind,
        IntrospectionCall::GetArg => {
            get_arg_expr(param_names, &args[0], count_metadata, span)
        }
    }
}

/// Builds the actual PHP argument count from fixed parameters or hidden count metadata.
fn argc_expr(param_names: &[String], count_metadata: bool, span: Span) -> Expr {
    if count_metadata {
        return Expr::new(
            ExprKind::Cast {
                target: CastType::Int,
                expr: Box::new(hidden_arg_at(0, span)),
            },
            span,
        );
    }
    let surplus = count_call(hidden_args_var(span), span);
    if param_names.is_empty() {
        return surplus;
    }
    Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(Expr::new(
                ExprKind::IntLiteral(param_names.len() as i64),
                span,
            )),
            op: BinOp::Add,
            right: Box::new(surplus),
        },
        span,
    )
}

/// Builds `[$p0, …, $pN-1, ...$__elephc_func_args]`, the full argument list in call order.
///
/// The array literal is fresh at every use, matching PHP's copy semantics, and the spread
/// of the hidden variadic keeps the surplus arguments renumbered from `N`.
fn args_array_expr(param_names: &[String], count_metadata: bool, span: Span) -> Expr {
    let mut elements: Vec<Expr> = param_names
        .iter()
        .map(|name| Expr::new(ExprKind::Variable(name.clone()), span))
        .collect();
    let tail = if count_metadata {
        array_slice_call(
            hidden_args_var(span),
            Expr::new(ExprKind::IntLiteral(1), span),
            None,
            span,
        )
    } else {
        hidden_args_var(span)
    };
    elements.push(Expr::new(ExprKind::Spread(Box::new(tail)), span));
    let values = Expr::new(ExprKind::ArrayLiteral(elements), span);
    if count_metadata {
        array_slice_call(
            values,
            Expr::new(ExprKind::IntLiteral(0), span),
            Some(argc_expr(param_names, true, span)),
            span,
        )
    } else {
        values
    }
}

/// Builds the range-checked indexed read behind `func_get_arg($position)`:
///
/// ```text
/// $position < 0
///     ? throw new \ValueError(<negative message>)
///     : ($position < <argc> ? <args>[$position] : throw new \ValueError(<range message>))
/// ```
///
/// The position expression is bound to a hidden local first unless it is already
/// side-effect free, so a call such as `func_get_arg($i++)` evaluates its operand once.
fn get_arg_expr(
    param_names: &[String],
    position: &Expr,
    count_metadata: bool,
    span: Span,
) -> ExprKind {
    let (first_read, later_read) = position_reads(position, span);
    ExprKind::Ternary {
        condition: Box::new(less_than(
            first_read,
            Expr::new(ExprKind::IntLiteral(0), span),
            span,
        )),
        then_expr: Box::new(throw_value_error(NEGATIVE_POSITION_MESSAGE, span)),
        else_expr: Box::new(Expr::new(
            ExprKind::Ternary {
                condition: Box::new(less_than(
                    later_read.clone(),
                    argc_expr(param_names, count_metadata, span),
                    span,
                )),
                then_expr: Box::new(Expr::new(
                    ExprKind::ArrayAccess {
                        array: Box::new(args_array_expr(param_names, count_metadata, span)),
                        index: Box::new(later_read),
                    },
                    span,
                )),
                else_expr: Box::new(throw_value_error(OUT_OF_RANGE_POSITION_MESSAGE, span)),
            },
            span,
        )),
    }
}

/// Returns the two expressions that read the requested position: the first one is
/// evaluated once at the start of the range check, the second is re-read afterwards.
///
/// A literal or a plain variable is safe to re-evaluate, so it is used directly and no
/// hidden local is introduced. Anything else is bound to `$__elephc_func_arg_pos` by the
/// first read (an assignment expression yields the assigned value in PHP) and re-read from
/// that local, which preserves single evaluation of the operand's side effects.
fn position_reads(position: &Expr, span: Span) -> (Expr, Expr) {
    if matches!(
        position.kind,
        ExprKind::IntLiteral(_) | ExprKind::Variable(_)
    ) {
        return (position.clone(), position.clone());
    }
    let temp = Expr::new(ExprKind::Variable(POSITION_TEMP.to_string()), span);
    let bind = Expr::new(
        ExprKind::Assignment {
            target: Box::new(temp.clone()),
            value: Box::new(position.clone()),
            result_target: None,
            prelude: Vec::new(),
            conditional_value_temp: None,
        },
        span,
    );
    (bind, temp)
}

/// Builds `$__elephc_func_args`, the hidden variadic parameter holding the surplus
/// positional arguments.
fn hidden_args_var(span: Span) -> Expr {
    Expr::new(ExprKind::Variable(HIDDEN_ARGS_PARAM.to_string()), span)
}

/// Builds an indexed read from the hidden argument collector.
fn hidden_arg_at(index: i64, span: Span) -> Expr {
    Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(hidden_args_var(span)),
            index: Box::new(Expr::new(ExprKind::IntLiteral(index), span)),
        },
        span,
    )
}

/// Builds an internal fully-qualified `array_slice()` call with an optional length.
fn array_slice_call(value: Expr, offset: Expr, length: Option<Expr>, span: Span) -> Expr {
    let mut args = vec![value, offset];
    if let Some(length) = length {
        args.push(length);
    }
    Expr::new(
        ExprKind::FunctionCall {
            name: Name::from_parts(NameKind::FullyQualified, vec!["array_slice".to_string()]),
            args,
        },
        span,
    )
}

/// Builds `count(<value>)`.
fn count_call(value: Expr, span: Span) -> Expr {
    Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("count"),
            args: vec![value],
        },
        span,
    )
}

/// Builds `<left> < <right>`.
fn less_than(left: Expr, right: Expr, span: Span) -> Expr {
    Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(left),
            op: BinOp::Lt,
            right: Box::new(right),
        },
        span,
    )
}

/// Builds `throw new \ValueError(<message>)` as an expression, PHP 8's throw-expression
/// form, so it can sit in a ternary branch.
fn throw_value_error(message: &str, span: Span) -> Expr {
    Expr::new(
        ExprKind::Throw(Box::new(Expr::new(
            ExprKind::NewObject {
                class_name: Name::from_parts(NameKind::FullyQualified, vec!["ValueError".to_string()]),
                args: vec![Expr::new(ExprKind::StringLiteral(message.to_string()), span)],
            },
            span,
        ))),
        span,
    )
}
