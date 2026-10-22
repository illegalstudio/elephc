//! Purpose:
//! Desugars dynamic-name method calls whose target is only known at run time:
//! `$obj->$method(...)` / `$obj->{expr}(...)` with named arguments.
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
//! - A plain variable passed by name is carried back after the call when a by-reference
//!   parameter changed it, through its own storage, so references the caller holds stay intact
//!   (see `with_write_backs`). One variable passed to two names is refused.
//! - The receiver and the method name are evaluated first, then the arguments in source order,
//!   as PHP orders them.

use crate::errors::CompileError;
use crate::names::Name;
use crate::parser::ast::{ArrayEntry, Expr, ExprKind, Stmt, StmtKind};
use crate::span::Span;

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

    // A plain variable passed by name may bind a by-reference parameter of the method that is
    // only resolved at run time. An array element cannot alias a variable, and rebinding the
    // variable to the element would detach it from references the caller already holds
    // (`$alias =& $n`). So a hidden local aliases the element instead
    // (`$args['x'] = $n; $ref =& $args['x']; $snap = $ref;`), the callable invoker writes a
    // by-reference parameter through it, and after the call a changed value is assigned back
    // to `$n` (`$ref !== $snap && ($n = $ref)`), through `$n`'s own storage. A by-value
    // parameter leaves `$ref` unchanged, so `$n` is not written at all.
    let binds_place = |value: &Expr| matches!(&value.kind, ExprKind::Variable(name) if name != "this");
    let has_place_named = named.iter().any(|(_, value, _)| binds_place(value));
    // Two names given the same variable would bind two separate cells, so a method taking both
    // by reference would not see them alias each other, as PHP's do.
    let mut seen_places: Vec<&str> = Vec::new();
    for (_, value, arg_span) in &named {
        if let ExprKind::Variable(variable) = &value.kind {
            if binds_place(value) {
                if seen_places.contains(&variable.as_str()) {
                    return Err(CompileError::new(
                        *arg_span,
                        &format!(
                            "Passing ${variable} to more than one named argument of a call whose method name is only known at run time is not supported; copy it into a separate variable first"
                        ),
                    ));
                }
                seen_places.push(variable);
            }
        }
    }
    let mut write_backs: Vec<WriteBack> = Vec::new();
    let args_value = if spreads == 0 && !has_place_named {
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
            let key = Expr::new(ExprKind::StringLiteral(name.clone()), arg_span);
            let place = match &value.kind {
                ExprKind::Variable(variable) if binds_place(&value) => Some(variable.clone()),
                _ => None,
            };
            let stored = if place.is_some() {
                value
            } else {
                let value_local = local(&format!("value{index}"));
                prelude.push(Stmt::new(StmtKind::Assign { name: value_local.clone(), value }, arg_span));
                var(&value_local)
            };
            if spreads == 0 {
                // No unpacked list can have supplied this name already (names are validated).
                let place = place.map(|variable| write_back_for(&local, index, variable));
                push_named_entry(&mut prelude, &args_local, key, stored, place.as_ref(), arg_span);
                write_backs.extend(place);
                continue;
            }
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
            let place = place.map(|variable| write_back_for(&local, index, variable));
            push_named_entry(&mut prelude, &args_local, key, stored, place.as_ref(), arg_span);
            write_backs.extend(place);
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
    let call = with_write_backs(call, write_backs, span);
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

/// The hidden locals that carry a by-reference write of one named argument back to the
/// caller's variable: `reference` aliases the argument-array element, `snapshot` holds the
/// value it was passed with.
struct WriteBack {
    variable: String,
    reference: String,
    snapshot: String,
}

/// Names the [`WriteBack`] locals of the `index`-th named argument, passed as `variable`.
fn write_back_for(local: &dyn Fn(&str) -> String, index: usize, variable: String) -> WriteBack {
    WriteBack {
        variable,
        reference: local(&format!("ref{index}")),
        snapshot: local(&format!("snap{index}")),
    }
}

/// Appends `$args[key] = value;` to the prelude and, for a plain variable passed by name, binds
/// its [`WriteBack`] locals to the new element (`$ref =& $args[key]; $snap = $ref;`) so a
/// by-reference parameter's write can be carried back after the call.
fn push_named_entry(
    prelude: &mut Vec<Stmt>,
    args_local: &str,
    key: Expr,
    value: Expr,
    write_back: Option<&WriteBack>,
    span: Span,
) {
    prelude.push(Stmt::new(
        StmtKind::ArrayAssign { array: args_local.to_string(), index: key.clone(), value },
        span,
    ));
    if let Some(write_back) = write_back {
        let element = Expr::new(
            ExprKind::ArrayAccess {
                array: Box::new(Expr::new(ExprKind::Variable(args_local.to_string()), span)),
                index: Box::new(key),
            },
            span,
        );
        prelude.push(Stmt::new(
            StmtKind::RefAssign { target: write_back.reference.clone(), source: element },
            span,
        ));
        prelude.push(Stmt::new(
            StmtKind::Assign {
                name: write_back.snapshot.clone(),
                value: Expr::new(ExprKind::Variable(write_back.reference.clone()), span),
            },
            span,
        ));
    }
}

/// Returns `call` followed by the write-backs, as `[call, $ref !== $snap && ($n = $ref),
/// ...][0]`: the call runs first, then each variable whose element a by-reference parameter
/// changed is assigned the new value, and the expression still yields the call's result.
fn with_write_backs(call: Expr, write_backs: Vec<WriteBack>, span: Span) -> Expr {
    if write_backs.is_empty() {
        return call;
    }
    let var = |name: &str| Expr::new(ExprKind::Variable(name.to_string()), span);
    let mut items = vec![call];
    for write_back in write_backs {
        let changed = Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(var(&write_back.reference)),
                op: crate::parser::ast::BinOp::StrictNotEq,
                right: Box::new(var(&write_back.snapshot)),
            },
            span,
        );
        let assign_back = Expr::new(
            ExprKind::Assignment {
                target: Box::new(var(&write_back.variable)),
                value: Box::new(var(&write_back.reference)),
                result_target: None,
                prelude: Vec::new(),
                conditional_value_temp: None,
            },
            span,
        );
        items.push(Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(changed),
                op: crate::parser::ast::BinOp::And,
                right: Box::new(assign_back),
            },
            span,
        ));
    }
    Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(Expr::new(ExprKind::ArrayLiteral(items), span)),
            index: Box::new(Expr::new(ExprKind::IntLiteral(0), span)),
        },
        span,
    )
}

/// Enforces PHP's compile-time argument-list rules that the desugaring would otherwise blur,
/// through the shared `call_args` rule set for callees unknown at compile time.
fn validate_argument_order(args: &[Expr]) -> Result<(), CompileError> {
    use crate::types::call_args::CallArgPlanError;
    crate::types::call_args::validate_unknown_callee_arg_order(args).map_err(|error| match error {
        CallArgPlanError::Duplicate { span, name, .. } => {
            CompileError::new(span, &format!("Duplicate named parameter ${name}"))
        }
        CallArgPlanError::SpreadAfterNamed { span } => {
            CompileError::new(span, "Cannot use argument unpacking after named arguments")
        }
        CallArgPlanError::PositionalAfterNamed { span } => {
            CompileError::new(span, "Cannot use positional argument after named argument")
        }
        CallArgPlanError::PositionalAfterSpread { span } => {
            CompileError::new(span, "Cannot use positional argument after argument unpacking")
        }
        other => unreachable!("validate_unknown_callee_arg_order only reports ordering errors: {other:?}"),
    })
}
