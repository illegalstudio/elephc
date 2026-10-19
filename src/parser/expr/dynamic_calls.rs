//! Purpose:
//! Desugars dynamic-name method calls whose target is only known at run time:
//! `$obj->$method(...)` / `$obj->{expr}(...)` as a first-class callable or with named arguments.
//!
//! Called from:
//! - `crate::parser::expr::pratt`, after `->` / `?->` and a variable or braced member name.
//!
//! Key details:
//! - A named argument cannot be matched to a parameter position at compile time, so the call
//!   becomes `call_user_func_array([$obj, $method], $args)` with the names as string keys of
//!   `$args`; the runtime callable invoker binds them against the resolved method exactly like
//!   a direct call (unknown names, overwrites, positional-after-named all raise PHP's `Error`).
//! - An unpacked argument (`...$list`) can carry string keys of its own. PHP rejects a named
//!   argument that one of them already supplied (`Named parameter $x overwrites previous
//!   argument`); an array literal would silently overwrite it, so a spread is merged into a
//!   temporary and every explicit name is checked against it before it is added. The checks are
//!   `exists ? throw ... : null` assignments because an assignment prelude only holds
//!   assignment-like statements.
//! - The receiver and the method name are evaluated first, then the arguments in source order,
//!   as PHP orders them.

use crate::errors::CompileError;
use crate::names::Name;
use crate::parser::ast::{ArrayEntry, Expr, ExprKind, Stmt, StmtKind};
use crate::span::Span;

/// Builds `$obj->$method(...)` — the Closure of the runtime-named method.
pub(super) fn dynamic_method_first_class_callable(object: Expr, method: Expr, span: Span) -> Expr {
    super::prefix::expr_first_class_callable(
        Expr::new(ExprKind::ArrayLiteral(vec![object, method]), span),
        span,
    )
}

/// Returns `true` when `args` contains a named argument.
pub(super) fn has_named_args(args: &[Expr]) -> bool {
    args.iter().any(|arg| matches!(arg.kind, ExprKind::NamedArg { .. }))
}

/// Builds `$obj->$method(args)` for an argument list that has named arguments.
pub(super) fn dynamic_method_call_with_named_args(
    object: Expr,
    method: Expr,
    args: Vec<Expr>,
    span: Span,
) -> Result<Expr, CompileError> {
    validate_argument_order(&args)?;
    let spreads = args.iter().filter(|arg| matches!(arg.kind, ExprKind::Spread(_))).count();
    if spreads > 1 {
        return Err(CompileError::new(
            span,
            "Named arguments together with more than one unpacked argument (`...$a, ...$b, name: v`) are not supported in a dynamic call; merge the lists first",
        ));
    }
    let local = |stem: &str| {
        crate::names::generated_local_name(&format!(
            "__elephc_dyn_named_{stem}_{}_{}",
            span.line, span.col
        ))
    };
    let var = |name: &str| Expr::new(ExprKind::Variable(name.to_string()), span);
    let (object_local, method_local, args_local, result_local) =
        (local("obj"), local("method"), local("args"), local("result"));
    let mut prelude = vec![
        Stmt::new(StmtKind::Assign { name: object_local.clone(), value: object }, span),
        Stmt::new(StmtKind::Assign { name: method_local.clone(), value: method }, span),
    ];

    let mut leading = Vec::new();
    let mut named = Vec::new();
    for arg in args {
        match arg.kind {
            ExprKind::NamedArg { name, value } => named.push((name, *value, arg.span)),
            ExprKind::Spread(inner) => leading.push(ArrayEntry::Spread(*inner)),
            _ => leading.push(ArrayEntry::Value(arg)),
        }
    }

    let args_value = if spreads == 0 {
        // Every name is distinct (validated) and no positional key is a string, so the literal
        // is exactly the argument list.
        let mut entries = leading;
        for (name, value, arg_span) in named {
            entries.push(ArrayEntry::Keyed(
                Expr::new(ExprKind::StringLiteral(name), arg_span),
                value,
            ));
        }
        Expr::new(ExprKind::ArrayLiteralMixed(entries), span)
    } else {
        prelude.push(Stmt::new(
            StmtKind::Assign {
                name: args_local.clone(),
                value: Expr::new(ExprKind::ArrayLiteralMixed(leading), span),
            },
            span,
        ));
        for (index, (name, value, arg_span)) in named.into_iter().enumerate() {
            let value_local = local(&format!("value{index}"));
            let key = Expr::new(ExprKind::StringLiteral(name.clone()), arg_span);
            prelude.push(Stmt::new(StmtKind::Assign { name: value_local.clone(), value }, arg_span));
            let already_passed = Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified("array_key_exists"),
                    args: vec![key.clone(), var(&args_local)],
                },
                arg_span,
            );
            let error = Expr::new(
                ExprKind::NewObject {
                    class_name: Name::unqualified("Error"),
                    args: vec![Expr::new(
                        ExprKind::StringLiteral(format!(
                            "Named parameter ${name} overwrites previous argument"
                        )),
                        arg_span,
                    )],
                },
                arg_span,
            );
            // Assignment preludes hold assignment-like statements only, so the guard is the
            // expression `exists ? throw new Error(...) : null`.
            prelude.push(Stmt::new(
                StmtKind::Assign {
                    name: local(&format!("guard{index}")),
                    value: Expr::new(
                        ExprKind::Ternary {
                            condition: Box::new(already_passed),
                            then_expr: Box::new(Expr::new(
                                ExprKind::Throw(Box::new(error)),
                                arg_span,
                            )),
                            else_expr: Box::new(Expr::new(ExprKind::Null, arg_span)),
                        },
                        arg_span,
                    ),
                },
                arg_span,
            ));
            prelude.push(Stmt::new(
                StmtKind::ArrayAssign {
                    array: args_local.clone(),
                    index: key,
                    value: var(&value_local),
                },
                arg_span,
            ));
        }
        var(&args_local)
    };

    let call = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("call_user_func_array"),
            args: vec![
                Expr::new(
                    ExprKind::ArrayLiteral(vec![var(&object_local), var(&method_local)]),
                    span,
                ),
                args_value,
            ],
        },
        span,
    );
    Ok(Expr::new(
        ExprKind::Assignment {
            target: Box::new(var(&result_local)),
            value: Box::new(call),
            result_target: None,
            prelude,
            conditional_value_temp: None,
        },
        span,
    ))
}

/// Enforces PHP's compile-time argument-list rules that the desugaring would otherwise blur.
fn validate_argument_order(args: &[Expr]) -> Result<(), CompileError> {
    let mut seen_named: Vec<&str> = Vec::new();
    let mut seen_spread = false;
    for arg in args {
        match &arg.kind {
            ExprKind::NamedArg { name, .. } => {
                if seen_named.contains(&name.as_str()) {
                    return Err(CompileError::new(
                        arg.span,
                        &format!("Duplicate named parameter ${name}"),
                    ));
                }
                seen_named.push(name);
            }
            ExprKind::Spread(_) => {
                if !seen_named.is_empty() {
                    return Err(CompileError::new(
                        arg.span,
                        "Cannot use argument unpacking after named arguments",
                    ));
                }
                seen_spread = true;
            }
            _ => {
                if !seen_named.is_empty() {
                    return Err(CompileError::new(
                        arg.span,
                        "Cannot use positional argument after named argument",
                    ));
                }
                if seen_spread {
                    return Err(CompileError::new(
                        arg.span,
                        "Cannot use positional argument after argument unpacking",
                    ));
                }
            }
        }
    }
    Ok(())
}
