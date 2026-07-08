//! Purpose:
//! Implements constant propagation expression support: substitutes scalar
//! facts at variable reads, folds constant-index reads of array-fact locals,
//! and masks by-ref argument positions so they stay lvalues.
//!
//! Called from:
//! - `crate::optimize::propagate`
//!
//! Key details:
//! - Substitution runs against the environment minus the expression's own
//!   `Invalidation` write set; array facts never substitute at plain variable
//!   reads (materializing a literal would break by-ref argument passing).

use super::*;

/// Extracts constants for by-value closure captures while excluding by-reference captures.
pub(crate) fn captured_constant_env(
    captures: &[String],
    capture_refs: &[String],
    env: &ConstantEnv,
) -> ConstantEnv {
    captures
        .iter()
        .filter(|name| !capture_refs.contains(name))
        .filter_map(|name| env.get(name).cloned().map(|value| (name.clone(), value)))
        .collect()
}

/// Recursively propagates constant facts through an expression, substituting known scalar
/// variables with their constant values and folding constant-index reads of array-fact
/// locals. Substitution runs against the environment minus the expression's own write set
/// so a stale fact is never propagated across an in-expression write. Returns a new
/// expression with substitutions applied, followed by constant folding.
pub(crate) fn propagate_expr(expr: Expr, env: &ConstantEnv) -> Expr {
    let reduced_env;
    // Drop the names this expression may write before substituting, so a stale
    // constant is never propagated into a read sequenced after the write in the
    // same expression — e.g. `$i` after a by-reference-mutating `bump($i)` in
    // `match(bump($i)) . "|" . $i` (issue #384). Sub-expression evaluation order
    // is not tracked, so a written name is conservatively blocked everywhere in
    // the expression; unwritten names keep folding. `All` (include, yield,
    // spread into a by-ref callee, top-level global-writing call) blocks
    // everything.
    let env = match expr_invalidation(&expr) {
        Invalidation::Names(writes) if writes.is_empty() => env,
        Invalidation::Names(writes) => {
            reduced_env = env
                .iter()
                .filter(|(name, _)| !writes.contains(*name))
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect();
            &reduced_env
        }
        Invalidation::All => {
            reduced_env = HashMap::new();
            &reduced_env
        }
    };
    let span = expr.span;
    let kind = match expr.kind {
        // `IncludeValue` is a transient parser node fully expanded by the resolver;
        // it can never reach this pass.
        ExprKind::IncludeValue { .. } => unreachable!(
            "ExprKind::IncludeValue must be expanded by the resolver"
        ),
        ExprKind::StringLiteral(value) => ExprKind::StringLiteral(value),
        ExprKind::IntLiteral(value) => ExprKind::IntLiteral(value),
        ExprKind::FloatLiteral(value) => ExprKind::FloatLiteral(value),
        ExprKind::Variable(name) => match env.get(&name).and_then(PropagatedValue::as_scalar) {
            // Only scalar facts substitute at variable reads; an array fact
            // must keep the variable (materializing the literal would break
            // by-ref argument passing and duplicate the allocation).
            Some(value) => value.clone().into_expr_kind(),
            None => ExprKind::Variable(name),
        },
        ExprKind::BinaryOp { left, op, right } => ExprKind::BinaryOp {
            left: Box::new(propagate_expr(*left, env)),
            op,
            right: Box::new(propagate_expr(*right, env)),
        },
        ExprKind::InstanceOf { value, target } => ExprKind::InstanceOf {
            value: Box::new(propagate_expr(*value, env)),
            target: propagate_instanceof_target(target, env),
        },
        ExprKind::BoolLiteral(value) => ExprKind::BoolLiteral(value),
        ExprKind::Null => ExprKind::Null,
        ExprKind::Negate(inner) => ExprKind::Negate(Box::new(propagate_expr(*inner, env))),
        ExprKind::Not(inner) => ExprKind::Not(Box::new(propagate_expr(*inner, env))),
        ExprKind::BitNot(inner) => ExprKind::BitNot(Box::new(propagate_expr(*inner, env))),
        ExprKind::Throw(inner) => ExprKind::Throw(Box::new(propagate_expr(*inner, env))),
        ExprKind::ErrorSuppress(inner) => {
            ExprKind::ErrorSuppress(Box::new(propagate_expr(*inner, env)))
        }
        ExprKind::Print(inner) => ExprKind::Print(Box::new(propagate_expr(*inner, env))),
        ExprKind::NullCoalesce { value, default } => ExprKind::NullCoalesce {
            value: Box::new(propagate_expr(*value, env)),
            default: Box::new(propagate_expr(*default, env)),
        },
        ExprKind::Pipe { value, callable } => ExprKind::Pipe {
            value: Box::new(propagate_expr(*value, env)),
            callable: Box::new(propagate_expr(*callable, env)),
        },
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            conditional_value_temp,
        } => ExprKind::Assignment {
            target,
            value: Box::new(propagate_expr(*value, env)),
            result_target,
            prelude,
            conditional_value_temp,
        },
        ExprKind::PreIncrement(name) => ExprKind::PreIncrement(name),
        ExprKind::PostIncrement(name) => ExprKind::PostIncrement(name),
        ExprKind::PreDecrement(name) => ExprKind::PreDecrement(name),
        ExprKind::PostDecrement(name) => ExprKind::PostDecrement(name),
        ExprKind::FunctionCall { name, args } => {
            let arg_env = (!function_call_effect(name.as_str()).has_side_effects).then_some(env);
            let by_ref = function_by_ref_params(name.as_str());
            ExprKind::FunctionCall {
                name,
                args: propagate_args(args, arg_env, by_ref.as_deref()),
            }
        }
        ExprKind::ArrayLiteral(items) => {
            ExprKind::ArrayLiteral(items.into_iter().map(|item| propagate_expr(item, env)).collect())
        }
        ExprKind::ArrayLiteralAssoc(items) => ExprKind::ArrayLiteralAssoc(
            items.into_iter()
                .map(|(key, value)| (propagate_expr(key, env), propagate_expr(value, env)))
                .collect(),
        ),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => ExprKind::Match {
            subject: Box::new(propagate_expr(*subject, env)),
            arms: arms
                .into_iter()
                .map(|(patterns, value)| {
                    (
                        patterns
                            .into_iter()
                            .map(|pattern| propagate_expr(pattern, env))
                            .collect(),
                        propagate_expr(value, env),
                    )
                })
                .collect(),
            default: default.map(|expr| Box::new(propagate_expr(*expr, env))),
        },
        ExprKind::ArrayAccess { array, index } => {
            let array = propagate_expr(*array, env);
            let index = propagate_expr(*index, env);
            // A variable carrying an array-literal fact folds constant-index
            // reads through the same helper as inline literals. Fact elements
            // are all scalar literals, so a successful fold always yields a
            // scalar; a miss (unknown index, out of range) keeps the runtime
            // access and its warning behavior.
            if let ExprKind::Variable(name) = &array.kind {
                if let Some(PropagatedValue::ArrayLit(fact)) = env.get(name) {
                    if let Some(kind) = try_fold_array_access(fact, &index) {
                        return fold_expr(Expr { kind, span });
                    }
                }
            }
            ExprKind::ArrayAccess {
                array: Box::new(array),
                index: Box::new(index),
            }
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => ExprKind::Ternary {
            condition: Box::new(propagate_expr(*condition, env)),
            then_expr: Box::new(propagate_expr(*then_expr, env)),
            else_expr: Box::new(propagate_expr(*else_expr, env)),
        },
        ExprKind::ShortTernary { value, default } => ExprKind::ShortTernary {
            value: Box::new(propagate_expr(*value, env)),
            default: Box::new(propagate_expr(*default, env)),
        },
        ExprKind::Cast { target, expr } => ExprKind::Cast {
            target,
            expr: Box::new(propagate_expr(*expr, env)),
        },
        ExprKind::Closure {
            params,
            variadic,
            variadic_type,
            return_type,
            body,
            is_arrow,
            is_static,
            captures,
            capture_refs,
            by_ref_return,
        } => {
            // A by-ref capture aliases the outer variable: any later invocation of
            // the closure can rewrite it, so it must never carry a constant.
            for name in &capture_refs {
                super::stmt::mark_reference_volatile(name);
            }
            ExprKind::Closure {
                params: propagate_params(params),
                variadic,
                variadic_type,
                return_type,
                body: super::stmt::with_function_scope(|| {
                    propagate_block(body, captured_constant_env(&captures, &capture_refs, env)).0
                }),
                is_arrow,
                is_static,
                captures,
                capture_refs,
                by_ref_return,
            }
        }
        ExprKind::NamedArg { name, value } => ExprKind::NamedArg {
            name,
            value: Box::new(propagate_expr(*value, env)),
        },
        ExprKind::Spread(inner) => ExprKind::Spread(Box::new(propagate_expr(*inner, env))),
        ExprKind::ClosureCall { var, args } => {
            let arg_env = (!callable_alias_effect(&var).has_side_effects).then_some(env);
            ExprKind::ClosureCall {
                var,
                args: propagate_args(args, arg_env, None),
            }
        }
        ExprKind::ExprCall { callee, args } => {
            let callee = propagate_expr(*callee, env);
            let arg_env = (!expr_call_effect(&callee).has_side_effects).then_some(env);
            ExprKind::ExprCall {
                callee: Box::new(callee),
                args: propagate_args(args, arg_env, None),
            }
        }
        ExprKind::ConstRef(name) => ExprKind::ConstRef(name),
        ExprKind::NewObject { class_name, args } => ExprKind::NewObject {
            class_name,
            args: propagate_args(args, None, None),
        },
        ExprKind::NewDynamic { name_expr, args } => ExprKind::NewDynamic {
            name_expr: Box::new(propagate_expr(*name_expr, env)),
            args: propagate_args(args, None, None),
        },
        ExprKind::NewDynamicObject {
            class_name,
            fallback_class,
            required_parent,
            args,
        } => ExprKind::NewDynamicObject {
            class_name: Box::new(propagate_expr(*class_name, env)),
            fallback_class,
            required_parent,
            args: propagate_args(args, None, None),
        },
        ExprKind::PropertyAccess { object, property } => ExprKind::PropertyAccess {
            object: Box::new(propagate_expr(*object, env)),
            property,
        },
        ExprKind::DynamicPropertyAccess { object, property } => {
            ExprKind::DynamicPropertyAccess {
                object: Box::new(propagate_expr(*object, env)),
                property: Box::new(propagate_expr(*property, env)),
            }
        }
        ExprKind::NullsafePropertyAccess { object, property } => {
            ExprKind::NullsafePropertyAccess {
                object: Box::new(propagate_expr(*object, env)),
                property,
            }
        }
        ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            ExprKind::NullsafeDynamicPropertyAccess {
                object: Box::new(propagate_expr(*object, env)),
                property: Box::new(propagate_expr(*property, env)),
            }
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            ExprKind::StaticPropertyAccess { receiver, property }
        }
        ExprKind::MethodCall {
            object,
            method,
            args,
        } => {
            let object = propagate_expr(*object, env);
            let arg_env =
                (!private_instance_method_call_effect(&object, &method).has_side_effects)
                    .then_some(env);
            let by_ref = method_by_ref_params(&method);
            ExprKind::MethodCall {
                object: Box::new(object),
                method,
                args: propagate_args(args, arg_env, by_ref.as_deref()),
            }
        }
        ExprKind::NullsafeMethodCall {
            object,
            method,
            args,
        } => {
            let object = propagate_expr(*object, env);
            ExprKind::NullsafeMethodCall {
                object: Box::new(object),
                method,
                args: propagate_args(args, None, None),
            }
        }
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => {
            let arg_env =
                (!static_method_call_effect(&receiver, &method).has_side_effects).then_some(env);
            let by_ref = method_by_ref_params(&method);
            ExprKind::StaticMethodCall {
                receiver,
                method,
                args: propagate_args(args, arg_env, by_ref.as_deref()),
            }
        }
        ExprKind::FirstClassCallable(target) => {
            ExprKind::FirstClassCallable(propagate_callable_target(target, env))
        }
        ExprKind::This => ExprKind::This,
        ExprKind::PtrCast { target_type, expr } => ExprKind::PtrCast {
            target_type,
            expr: Box::new(propagate_expr(*expr, env)),
        },
        ExprKind::BufferNew { element_type, len } => ExprKind::BufferNew {
            element_type,
            len: Box::new(propagate_expr(*len, env)),
        },
        ExprKind::ClassConstant { receiver } => ExprKind::ClassConstant { receiver },
        ExprKind::ScopedConstantAccess { receiver, name } => {
            ExprKind::ScopedConstantAccess { receiver, name }
        }
        ExprKind::NewScopedObject { receiver, args } => ExprKind::NewScopedObject {
            receiver,
            args: propagate_args(args, None, None),
        },
        ExprKind::Yield { key, value } => ExprKind::Yield {
            key: key.map(|k| Box::new(propagate_expr(*k, env))),
            value: value.map(|v| Box::new(propagate_expr(*v, env))),
        },
        ExprKind::YieldFrom(inner) => ExprKind::YieldFrom(Box::new(propagate_expr(*inner, env))),
        ExprKind::MagicConstant(_) => {
            unreachable!("MagicConstant must be lowered before optimizer passes")
        }
    };

    fold_expr(Expr { kind, span })
}

/// Propagates constants into the target of an instanceof expression. If the target is a bare
/// expression (not a class name), recursively applies constant propagation to it.
fn propagate_instanceof_target(
    target: InstanceOfTarget,
    env: &ConstantEnv,
) -> InstanceOfTarget {
    match target {
        InstanceOfTarget::Name(name) => InstanceOfTarget::Name(name),
        InstanceOfTarget::Expr(expr) => {
            InstanceOfTarget::Expr(Box::new(propagate_expr(*expr, env)))
        }
    }
}

/// Propagates constants into a callable target. Only the `Method` variant contains an
/// expression (the object) that may hold a substitutable variable; `Function` and `StaticMethod`
/// targets are returned unchanged since they contain no propagatable sub-expressions.
pub(crate) fn propagate_callable_target(target: CallableTarget, env: &ConstantEnv) -> CallableTarget {
    match target {
        CallableTarget::Function(name) => CallableTarget::Function(name),
        CallableTarget::StaticMethod { receiver, method } => {
            CallableTarget::StaticMethod { receiver, method }
        }
        CallableTarget::Method { object, method } => CallableTarget::Method {
            object: Box::new(propagate_expr(*object, env)),
            method,
        },
    }
}

/// Applies constant propagation to a list of call arguments. When `env` is `Some`,
/// propagates into all arguments normally. When `env` is `None` (side-effecting call),
/// uses an empty environment so no constants are propagated into arguments.
///
/// `by_ref` carries the callee's `(param name, is_by_ref)` signature when known:
/// an argument sitting at a by-ref position is passed through untouched — it must
/// stay an lvalue for the backend to take the slot address, so neither variable
/// substitution nor folding may rewrite it.
pub(crate) fn propagate_args(
    args: Vec<Expr>,
    env: Option<&ConstantEnv>,
    by_ref: Option<&[(String, bool)]>,
) -> Vec<Expr> {
    let empty_env = HashMap::new();
    let env = env.unwrap_or(&empty_env);
    // Positional arguments cannot legally follow a spread, but if one does the
    // positional cursor no longer matches the signature — treat everything
    // after the spread as potentially by-ref when the callee has any.
    let has_by_ref = by_ref.is_some_and(|sig| sig.iter().any(|(_, is_ref)| *is_ref));
    let mut spread_seen = false;
    args.into_iter()
        .enumerate()
        .map(|(position, arg)| {
            let masked = match &arg.kind {
                ExprKind::NamedArg { name, .. } => by_ref.is_some_and(|sig| {
                    sig.iter().any(|(param, is_ref)| param == name && *is_ref)
                }),
                ExprKind::Spread(_) => {
                    spread_seen = true;
                    false
                }
                _ if spread_seen => has_by_ref,
                _ => by_ref.is_some_and(|sig| {
                    sig.get(position).is_some_and(|(_, is_ref)| *is_ref)
                }),
            };
            if masked {
                arg
            } else {
                propagate_expr(arg, env)
            }
        })
        .collect()
}

/// Constructs an if/elseif/else statement from its components, performing local
/// restructuring optimizations: collapses adjacent else-if chains when the else body
/// contains only a single if statement, and simplifies consecutive ternary-like
/// conditions into combined conditions using `combine_if_conditions` or `combine_if_chain_conditions`.
pub(crate) fn build_if_stmt(
    condition: Expr,
    then_body: Vec<Stmt>,
    elseif_clauses: Vec<(Expr, Vec<Stmt>)>,
    else_body: Option<Vec<Stmt>>,
    span: crate::span::Span,
) -> Stmt {
    if elseif_clauses.is_empty() {
        if let Some(else_body_ref) = else_body.as_ref() {
            if else_body_ref.len() == 1 {
                if let StmtKind::If {
                    condition: inner_condition,
                    then_body: inner_then_body,
                    elseif_clauses: inner_elseifs,
                    else_body: inner_else,
                } = &else_body_ref[0].kind
                {
                    if inner_elseifs.is_empty() && *inner_then_body == then_body {
                        return build_if_stmt(
                            combine_if_chain_conditions(condition, inner_condition.clone()),
                            then_body,
                            Vec::new(),
                            inner_else.clone(),
                            span,
                        );
                    }

                    if inner_elseifs.is_empty() && inner_else.as_ref() == Some(&then_body) {
                        return build_if_stmt(
                            combine_if_conditions(
                                invert_condition(condition),
                                inner_condition.clone(),
                            ),
                            inner_then_body.clone(),
                            Vec::new(),
                            Some(then_body),
                            span,
                        );
                    }
                }
            }
        }

        if else_body.is_none() && then_body.len() == 1 {
            if let StmtKind::If {
                condition: inner_condition,
                then_body: inner_then_body,
                elseif_clauses: inner_elseifs,
                else_body: inner_else,
            } = &then_body[0].kind
            {
                if inner_elseifs.is_empty() && inner_else.is_none() {
                    return Stmt {
                        kind: StmtKind::If {
                            condition: combine_if_conditions(condition, inner_condition.clone()),
                            then_body: inner_then_body.clone(),
                            elseif_clauses: Vec::new(),
                            else_body: None,
                        },
                        span,
                        attributes: Vec::new(),
                    };
                }
            }
        }
    }

    Stmt {
        kind: StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        },
        span,
        attributes: Vec::new(),
    }
}
