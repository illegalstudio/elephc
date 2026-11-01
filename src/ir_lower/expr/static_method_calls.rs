//! Purpose:
//! Static method calls, enum coercion, and callable descriptors.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers a static method call.
pub(super) fn lower_static_method_call(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    // An argument the checker PROVED cannot bind. php decides this when the call runs, so the
    // build stands and the call site throws php's own `TypeError` -- see
    // `ThrowAccessKind::ArgumentType`.
    if let Some((class_name, message)) = ctx
        .throw_access_sites
        .get(&(ctx.loop_storage_scope.clone(), expr.span))
        .and_then(|info| match &info.kind {
            crate::types::ThrowAccessKind::ArgumentType { message } => {
                Some(("TypeError", message.clone()))
            }
            // A `self::`/`parent::` call to a method the class lacks: php's `Error` when it runs.
            crate::types::ThrowAccessKind::UndefinedMethod { message } => {
                Some(("Error", message.clone()))
            }
            _ => None,
        })
    {
        return crate::ir_lower::stmt::lower_throw_access_class_expr(
            ctx,
            class_name,
            &message,
            expr.span,
        );
    }
    if let Some(call) = crate::types::late_static_dispatch::static_receiver_instance_method_call(
        ctx.classes,
        ctx.current_class.as_deref(),
        !ctx.local_slots.contains_key("this"),
        receiver,
        method,
        args,
        expr.span,
    ) {
        return lower_expr(ctx, &call);
    }
    if let Some(value) = lower_scoped_parent_call(ctx, receiver, method, args, expr) {
        return value;
    }
    if let Some(value) = lower_late_static_dispatch_ladder(ctx, receiver, method, args, expr) {
        return value;
    }
    if matches!(receiver, StaticReceiver::Named(name) if name.trim_start_matches('\\') == "ReflectionReference")
        && php_symbol_key(method) == "fromarrayelement"
    {
        let sig = static_method_implementation_signature(ctx, receiver, method).cloned();
        let operands = lower_args_with_signature(ctx, sig.as_ref(), args);
        release_owned_call_arg_temporaries(
            ctx,
            &operands,
            None,
            &ReturnArgAlias::None,
            expr.span,
        );
        return lower_null(ctx, expr);
    }
    // `Closure::fromCallable($closure)` is the closure itself (the checker typed it `callable`
    // only when the argument already was one).
    if let StaticReceiver::Named(name) = receiver {
        if name.trim_start_matches('\\') == "Closure"
            && php_symbol_key(method) == "fromcallable"
            && args.len() == 1
            && !matches!(args[0].kind, ExprKind::NamedArg { .. } | ExprKind::Spread(_))
            && materialized_expr_type_for_merge(ctx, &args[0]).codegen_repr() == PhpType::Callable
        {
            return lower_expr(ctx, &args[0]);
        }
    }
    // `Closure::fromCallable($value)` of a gradual (`mixed`) value in a closed-world build: the
    // callable normalization every callable-typed boundary already uses turns the value into a
    // callable descriptor (a Closure here), raising php's TypeError when it is not callable.
    if crate::closed_world::is_enabled() {
        if let StaticReceiver::Named(name) = receiver {
            if name.trim_start_matches('\\') == "Closure"
                && php_symbol_key(method) == "fromcallable"
                && args.len() == 1
                && !matches!(args[0].kind, ExprKind::NamedArg { .. } | ExprKind::Spread(_))
                // An array pair whose method name is only known at run time
                // (`Closure::fromCallable([$context, $name])` in Carbon's mixins and Laravel's
                // `Component::createVariableFromMethod()`) took the eval bridge otherwise.
                && matches!(
                    materialized_expr_type_for_merge(ctx, &args[0]).codegen_repr(),
                    PhpType::Mixed
                        | PhpType::Union(_)
                        | PhpType::Array(_)
                        | PhpType::AssocArray { .. }
                )
            {
                use crate::synthetic_class::{e_binop, e_call, e_new, e_not, e_str, e_var, s_if, s_throw};
                // A plain variable is read twice as it is; any other expression is evaluated once
                // into a scratch local first.
                let plain = matches!(args[0].kind, ExprKind::Variable(_));
                const SCRATCH: &str = "__elephc_closure_source";
                let value_name = match &args[0].kind {
                    ExprKind::Variable(name) if plain => name.clone(),
                    _ => SCRATCH.to_string(),
                };
                let callable_value_name: &str = value_name.as_str();
                if !plain {
                    let source = Stmt::new(
                        StmtKind::Assign { name: SCRATCH.to_string(), value: args[0].clone() },
                        expr.span,
                    );
                    super::super::stmt::lower_stmt(ctx, &source);
                }
                // php raises a TypeError for anything it cannot make a Closure of. A two-element
                // array is left to the conversion below: it resolves the method with this call's
                // scope, as php does, while the run-time `is_callable()` sees public methods only
                // (`[$this, 'getEnv']` names a protected method Symfony's container converts).
                let refuse = |message: Expr| s_throw(e_new("TypeError", vec![message]));
                let pair = e_binop(
                    e_call("is_array", vec![e_var(callable_value_name)]),
                    crate::parser::ast::BinOp::And,
                    e_binop(
                        e_call("count", vec![e_var(callable_value_name)]),
                        crate::parser::ast::BinOp::StrictEq,
                        crate::synthetic_class::e_int(2),
                    ),
                );
                let guard = s_if(
                        e_binop(
                            e_not(e_call("is_callable", vec![e_var(callable_value_name)])),
                            crate::parser::ast::BinOp::And,
                            e_not(pair),
                        ),
                    vec![s_if(
                            e_call("is_string", vec![e_var(callable_value_name)]),
                        vec![refuse(e_binop(
                            e_binop(
                                e_str("Failed to create closure from callable: function \""),
                                crate::parser::ast::BinOp::Concat,
                                        e_var(callable_value_name),
                            ),
                            crate::parser::ast::BinOp::Concat,
                            e_str("\" not found or invalid function name"),
                        ))],
                        vec![],
                        Some(vec![refuse(e_str("Failed to create closure from callable: no array or string given"))]),
                    )],
                    vec![],
                    None,
                );
                super::super::stmt::lower_stmt(ctx, &guard);
                // An object pair naming a method its class lacks (and no `__call` answers) is php's
                // TypeError too; the conversion below would only fatal on it.
                {
                    use crate::parser::ast::BinOp::{And, Concat};
                    let v = || e_var(callable_value_name);
                    let element = |key: i64| {
                        crate::synthetic_class::e_index(v(), crate::synthetic_class::e_int(key))
                    };
                    let has_key = |key: i64| {
                        e_call("array_key_exists", vec![crate::synthetic_class::e_int(key), v()])
                    };
                    let missing = [
                        e_call("is_array", vec![v()]),
                        has_key(0),
                        has_key(1),
                        e_call("is_object", vec![element(0)]),
                        e_call("is_string", vec![element(1)]),
                        e_not(e_call("method_exists", vec![element(0), element(1)])),
                        e_not(e_call("method_exists", vec![element(0), e_str("__call")])),
                    ]
                    .into_iter()
                    .reduce(|left, right| e_binop(left, And, right))
                    .expect("the condition has terms");
                    let message = [
                        e_str("Failed to create closure from callable: class "),
                        e_call("get_class", vec![element(0)]),
                        e_str(" does not have a method \""),
                        element(1),
                        e_str("\""),
                    ]
                    .into_iter()
                    .reduce(|left, right| e_binop(left, Concat, right))
                    .expect("the message has parts");
                    let missing_method = s_if(missing, vec![refuse(message)], vec![], None);
                    super::super::stmt::lower_stmt(ctx, &missing_method);
                }
                let value = lower_expr(ctx, &e_var(callable_value_name));
                let callable = crate::ir_lower::gradual_coercions::coerce_gradual_value_to_boundary(
                    ctx,
                    value,
                    &PhpType::Callable,
                    Some(expr.span),
                );
                // The normalization retains the payload it extracts from the boxed value; this
                // result claims that lease so its consumer releases it.
                ctx.builder.set_value_ownership(callable.value, Ownership::Owned);
                return callable;
            }
        }
    }
    // `Closure::bind($closure, $newThis [, $scope])` — static form of bindTo.
    if let StaticReceiver::Named(name) = receiver {
        if name.trim_start_matches('\\') == "Closure"
            && php_symbol_key(method) == "bind"
            && !args.is_empty()
        {
            return lower_static_closure_bind(ctx, args, expr);
        }
    }

    let magic_args;
    let (dispatch_method, call_args) = if let Some(args) =
        magic_static_call_dispatch_args(ctx, receiver, method, args, expr.span)
    {
        magic_args = args;
        ("__callStatic", magic_args.as_slice())
    } else {
        (method, args)
    };
    if matches!(receiver, StaticReceiver::Named(_)) && plain_positional_call_args(args) {
        if let Some(class_name) = static_receiver_class_name(ctx, receiver) {
            if !ctx.classes.contains_key(class_name.as_str()) {
                // A closed world has no autoloader or interpreter that could supply the class:
                // a class that is neither compiled nor a builtin simply does not exist, which is
                // php's own `Class "X" not found` Error.
                if crate::closed_world::is_enabled()
                    && !ctx.enums.contains_key(class_name.as_str())
                    && !ctx.interfaces.contains_key(class_name.as_str())
                    && elephc_builtin_contract::lookup_class(class_name.as_str().trim_start_matches('\\')).is_none()
                {
                    return super::late_bound_call::lower_closed_world_class_not_found(
                        ctx,
                        class_name.as_str(),
                        expr,
                    );
                }
                ctx.declare_eval_context_local();
                let operands = lower_args_with_signature(ctx, None, args);
                let name = format!("{}::{}", class_name, dispatch_method);
                let data = ctx.intern_string(&name);
                let call = ctx.emit_value(
                    Op::EvalStaticMethodCall,
                    operands.clone(),
                    Some(Immediate::Data(data)),
                    PhpType::Mixed,
                    Op::EvalStaticMethodCall.default_effects(),
                    Some(expr.span),
                );
                // The bridge boxes each operand with a reference of its own and answers with a
                // cell it retained, so a temporary argument -- the `[$object, 'm']` of
                // `Closure::fromCallable()` -- is still this frame's to drop.
                release_owned_call_arg_temporaries(
                    ctx,
                    &operands,
                    Some(call.value),
                    &ReturnArgAlias::None,
                    expr.span,
                );
                return call;
            }
        }
    }
    // `Later::method(...$args)` on a class only an eval declares: the plain route above carries one
    // boxed operand per argument, which a spread cannot supply. The same call through the
    // callable-array bridge takes the whole argument array, spreads and all -- PHP's own meaning
    // for it. Laravel's `Stringable::trim()` is `Str::trim(...array_merge([$this->value],
    // func_get_args()))` and every call died on `Class "Illuminate\Support\Str" not found`.
    //
    // Named arguments take the same route: an array key that is a string IS php's named
    // argument for `call_user_func_array`. `Str::ascii()` calls
    // `ASCII::to_ascii($value, $language, replace_single_chars_only: false)` on a class the
    // autoloader supplies, and a named-argument call to a class unknown at compile time was
    // compiled into a `Class "..." not found` fatal that never reached the autoloader.
    if matches!(receiver, StaticReceiver::Named(_)) && !plain_positional_call_args(args) {
        if let Some(class_name) = static_receiver_class_name(ctx, receiver) {
            if !ctx.classes.contains_key(class_name.as_str()) {
                // The positional route's closed-world rule: Laravel's `ApplicationBuilder` calls
                // `Folio::route($pages, middleware: ...)` behind `class_exists(Folio::class)`, and
                // laravel/folio is not installed.
                if crate::closed_world::is_enabled()
                    && !ctx.enums.contains_key(class_name.as_str())
                    && !ctx.interfaces.contains_key(class_name.as_str())
                    && elephc_builtin_contract::lookup_class(class_name.as_str().trim_start_matches('\\')).is_none()
                {
                    return super::late_bound_call::lower_closed_world_class_not_found(
                        ctx,
                        class_name.as_str(),
                        expr,
                    );
                }
                ctx.declare_eval_context_local();
                let callable = Expr::new(
                    ExprKind::ArrayLiteral(vec![
                        Expr::new(ExprKind::StringLiteral(class_name), expr.span),
                        Expr::new(ExprKind::StringLiteral(dispatch_method.to_string()), expr.span),
                    ]),
                    expr.span,
                );
                let arguments = if crate::types::call_args::has_named_args(args) {
                    let entries = args
                        .iter()
                        .map(|arg| match &arg.kind {
                            ExprKind::Spread(inner) => {
                                crate::parser::ast::ArrayEntry::Spread(inner.as_ref().clone())
                            }
                            ExprKind::NamedArg { name, value } => {
                                crate::parser::ast::ArrayEntry::Keyed(
                                    Expr::new(ExprKind::StringLiteral(name.clone()), arg.span),
                                    value.as_ref().clone(),
                                )
                            }
                            _ => crate::parser::ast::ArrayEntry::Value(arg.clone()),
                        })
                        .collect();
                    Expr::new(ExprKind::ArrayLiteralMixed(entries), expr.span)
                } else {
                    Expr::new(ExprKind::ArrayLiteral(args.to_vec()), expr.span)
                };
                let rewritten = Expr::new(
                    ExprKind::FunctionCall {
                        name: crate::names::Name::unqualified("call_user_func_array"),
                        args: vec![callable, arguments],
                    },
                    expr.span,
                );
                return lower_expr(ctx, &rewritten);
            }
        }
    }
    let sig = static_method_implementation_signature(ctx, receiver, dispatch_method)
        .or_else(|| lexical_instance_static_call_signature(ctx, receiver, dispatch_method))
        .cloned();
    let prepared = sig.as_ref().and_then(|signature| {
        ref_place_args::prepare_ref_place_args(ctx, signature, call_args)
    });
    let call_args = prepared
        .as_ref()
        .map(|(call_args, _)| call_args.as_slice())
        .unwrap_or(call_args);
    // The result type and the alias summary are decided by the receiver and the method alone, so
    // they are resolved up front: the reference lease and the ordinary owned result both need
    // their staging published BEFORE any argument expression runs, and the result staging has to
    // be declared with the exact type the call is emitted with.
    let result_type = sig
        .as_ref()
        .map(|signature| normalize_value_php_type(crate::types::eir_value_php_type(&signature.return_type)))
        .unwrap_or_else(|| {
            if ctx.has_eval_barrier() && matches!(receiver, StaticReceiver::Named(_)) {
                PhpType::Mixed
            } else {
                fallback_expr_type(expr)
            }
        });
    let late_static_receiver_type = static_late_binding_receiver_type_for_ir(ctx, receiver);
    let result_type = match (
        static_method_late_static_return_for_ir(ctx, receiver, dispatch_method),
        late_static_receiver_type.as_deref(),
    ) {
        (Some(return_type), Some(receiver_type)) => {
            late_static_return_type_for_ir(ctx, &return_type, receiver_type)
        }
        _ => result_type,
    };
    let return_alias = static_method_return_arg_alias(ctx, receiver, dispatch_method);
    // A by-reference-returning static method transfers a lease that must outlive this caller's
    // argument cleanup, so its staging is published before the arguments are evaluated.
    let reference_staging = begin_reference_return_call(ctx, sig.as_ref(), expr.span);
    let result_staging = prepublish_user_call_result(
        ctx, sig.as_ref(), &return_alias, &result_type, expr.span,
    );
    begin_call_argument_evaluation(ctx);
    let ref_place_rewrite = sig.as_ref().and_then(|signature| {
        ref_place_args::rewrite_typed_property_ref_args(ctx, signature, call_args)
    });
    let call_args = ref_place_rewrite
        .as_ref()
        .map_or(call_args, |rewrite| rewrite.args.as_slice());
    let callee_description = static_receiver_class_name(ctx, receiver)
        .unwrap_or_else(|| receiver_name(receiver).to_string());
    let previous_description = ctx
        .call_description
        .replace(format!("{callee_description}::{dispatch_method}"));
    let operands = lower_user_args_with_signature(ctx, sig.as_ref(), call_args);
    ctx.call_description = previous_description;
    let mut operands =
        coerce_int_backed_enum_string_argument(ctx, receiver, dispatch_method, operands, expr);
    let name = format!("{}::{}", receiver_name(receiver), dispatch_method);
    let data = ctx.intern_string(&name);
    let evaluation_intermediates = finish_call_argument_evaluation(ctx, &mut operands);
    let roots = root_user_call_operands(
        ctx,
        &mut operands,
        sig.as_ref(),
        &return_alias,
        &result_type,
        expr.span,
    );
    let call = ctx.emit_value(
        Op::StaticMethodCall,
        operands.clone(),
        Some(Immediate::Data(data)),
        result_type,
        Op::StaticMethodCall.default_effects(),
        Some(expr.span),
    );
    ctx.invalidate_callable_ref_argument_locals(sig.as_ref(), &operands);
    let call = finish_reference_return_call(
        ctx, call, sig.as_ref(), reference_staging.as_ref(), expr.span,
    );
    stage_call_result(ctx, result_staging.as_ref(), call, expr.span);
    release_owned_call_arg_temporaries_with_roots(
        ctx,
        &operands,
        Some(call.value),
        &return_alias,
        sig.as_ref(),
        &roots,
        expr.span,
    );
    retire_call_argument_intermediates(ctx, &evaluation_intermediates);
    if let Some(rewrite) = ref_place_rewrite {
        ref_place_args::finish_ref_place_rewrite(ctx, rewrite, expr.span);
    }
    let call = take_prepublished_call_result(ctx, result_staging, call, expr.span);
    if let Some((_, plans)) = prepared {
        ref_place_args::write_back_ref_place_args(ctx, plans);
    }
    finish_reference_return_value(ctx, call, reference_staging, expr.span)
}

/// Lowers `parent::method()` written in a closure whose class has no parent.
///
/// php resolves `parent` against the scope the closure is BOUND to (`$f->bindTo($object, $scope)`),
/// so the target is the method of the scope's parent class, called without virtual dispatch and
/// with `$this` forwarded. The scope is only known when the closure runs: it is the hidden scope
/// capture, read through `self::class` (`Op::ClosureScopeClassName`). The call becomes a ladder
/// over the classes that can be somebody's parent and have the method, keyed on
/// `get_parent_class(self::class)`; each arm is an ordinary `Parent::method()` call, which the
/// closure body may make because it carries the scope capture and a bound `$this`
/// (`closure_forwards_this_to_any_class`). It ends in php's own error when no arm matches, so a
/// scope with no parent behaves as php does. The checker types the call `mixed`.
fn lower_scoped_parent_call(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if !matches!(receiver, StaticReceiver::Parent)
        || !ctx.local_slots.contains_key(crate::names::CLOSURE_SCOPE_LOCAL)
    {
        return None;
    }
    let current = ctx.current_class.as_ref()?;
    if ctx.classes.get(current)?.parent.is_some() {
        return None;
    }
    let key = php_symbol_key(method);
    // An arm is only emitted for a method that can take THIS call's positional arguments. php
    // checks the count when the call runs, so a class whose `m` needs more (Symfony's
    // `AbstractAdapter::get(string $key, callable $callback, ...)` beside the parameter bag's
    // `get(string $name)`) merely throws if the scope ever resolves there -- but its arm made the
    // backend refuse the whole closure ("method call ... with 3 operands for 6 physical ... ABI
    // params"), which handed `Descriptor` and its subclasses to the interpreter. Spread or named
    // arguments are left to the ordinary call checks.
    let positional = (!args.iter().any(|arg| {
        matches!(arg.kind, ExprKind::Spread(_) | ExprKind::NamedArg { .. })
    }))
    .then_some(args.len());
    let accepts = |sig: &crate::types::FunctionSig| {
        let Some(count) = positional else {
            return true;
        };
        let regular = crate::types::call_args::regular_param_count(sig);
        let required = sig
            .defaults
            .iter()
            .take(regular)
            .take_while(|default| default.is_none())
            .count();
        count >= required && (sig.variadic.is_some() || count <= regular)
    };
    let mut parents: Vec<&String> = ctx
        .classes
        .values()
        .filter_map(|info| info.parent.as_ref())
        .collect();
    parents.sort();
    parents.dedup();
    let targets: Vec<String> = parents
        .into_iter()
        .filter(|parent| {
            ctx.classes.get(*parent).is_some_and(|info| {
                let callable = |visibility: Option<&Visibility>| visibility != Some(&Visibility::Private);
                (info.methods.get(&key).is_some_and(|sig| accepts(sig))
                    && !info.abstract_methods.contains(&key)
                    && callable(info.method_visibilities.get(&key)))
                    || (info.static_methods.get(&key).is_some_and(|sig| accepts(sig))
                        && !info.abstract_static_methods.contains(&key)
                        && callable(info.static_method_visibilities.get(&key)))
            })
        })
        .cloned()
        .collect();
    let span = expr.span;
    let message = Expr::new(
        ExprKind::StringLiteral("Cannot access \"parent\" when current class scope has no parent".to_string()),
        span,
    );
    let error = Expr::new(
        ExprKind::NewObject {
            class_name: crate::names::Name::unqualified("Error"),
            args: vec![message],
        },
        span,
    );
    let mut ladder = Expr::new(ExprKind::Throw(Box::new(error)), span);
    for target in targets.iter().rev() {
        let scope_parent = Expr::new(
            ExprKind::FunctionCall {
                name: crate::names::Name::unqualified("get_parent_class"),
                args: vec![Expr::new(
                    ExprKind::ClassConstant { receiver: StaticReceiver::Self_ },
                    span,
                )],
            },
            span,
        );
        let condition = Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(scope_parent),
                op: crate::parser::ast::BinOp::StrictEq,
                right: Box::new(Expr::new(ExprKind::StringLiteral(target.clone()), span)),
            },
            span,
        );
        let call = Expr::new(
            ExprKind::StaticMethodCall {
                receiver: StaticReceiver::Named(crate::names::Name::from_parts(
                    crate::names::NameKind::FullyQualified,
                    target.split('\\').map(str::to_string).collect(),
                )),
                method: method.to_string(),
                args: args.to_vec(),
            },
            span,
        );
        ladder = Expr::new(
            ExprKind::Ternary {
                condition: Box::new(condition),
                then_expr: Box::new(call),
                else_expr: Box::new(ladder),
            },
            span,
        );
    }
    let value = lower_expr(ctx, &ladder);
    let value_ty = ctx.builder.value_php_type(value.value);
    if matches!(value_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        return Some(value);
    }
    Some(ctx.box_value_as_mixed(value, value_ty, Some(span)))
}

/// Returns whether this body is a rebindable closure with a bound `$this`, which may forward that
/// `$this` to an instance method of any class through `Class::method()` syntax.
pub(super) fn closure_forwards_this_to_any_class(ctx: &LoweringContext<'_, '_>) -> bool {
    ctx.local_slots.contains_key(crate::names::CLOSURE_SCOPE_LOCAL)
        && ctx.local_slots.contains_key("this")
}

/// Lowers `static::method()` for a method only DESCENDANTS of the current class declare.
///
/// PHP resolves the call against the late-static class when it runs, and a protected method of a
/// subclass is reachable from the parent's scope. The call becomes a ladder over the late-static
/// class name: one `Class::method()` call per descendant that has the method (each carries that
/// exact class as the called class, so `static::` inside the target still resolves right), ending
/// in the ordinary `__callStatic` fallback when the class has one and in PHP's own
/// "Call to undefined method" error otherwise. No stub is added to the parent, so visibility,
/// `method_exists()` and `__callStatic` keep their PHP meaning. The checker types the call
/// `mixed`, and the ladder's value is boxed to match.
fn lower_late_static_dispatch_ladder(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if !matches!(receiver, StaticReceiver::Static) {
        return None;
    }
    if ctx.late_static_ladder_fallbacks.remove(&expr.span) {
        return None;
    }
    let current = ctx.current_class.clone()?;
    let targets = crate::types::late_static_dispatch::child_only_static_targets(
        ctx.classes,
        &current,
        &php_symbol_key(method),
    );
    if targets.is_empty() {
        return None;
    }
    let span = expr.span;
    let static_class = || Expr::new(ExprKind::ClassConstant { receiver: StaticReceiver::Static }, span);
    let has_call_static = ctx
        .classes
        .get(&current)
        .is_some_and(|info| info.static_methods.contains_key("__callstatic"));
    let mut ladder = if has_call_static {
        ctx.late_static_ladder_fallbacks.insert(span);
        expr.clone()
    } else {
        let message = Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(Expr::new(
                    ExprKind::BinaryOp {
                        left: Box::new(Expr::new(
                            ExprKind::StringLiteral("Call to undefined method ".to_string()),
                            span,
                        )),
                        op: crate::parser::ast::BinOp::Concat,
                        right: Box::new(static_class()),
                    },
                    span,
                )),
                op: crate::parser::ast::BinOp::Concat,
                right: Box::new(Expr::new(
                    ExprKind::StringLiteral(format!("::{}()", method)),
                    span,
                )),
            },
            span,
        );
        let error = Expr::new(
            ExprKind::NewObject {
                class_name: crate::names::Name::unqualified("Error"),
                args: vec![message],
            },
            span,
        );
        Expr::new(ExprKind::Throw(Box::new(error)), span)
    };
    for target in targets.iter().rev() {
        let named = StaticReceiver::Named(crate::names::Name::from_parts(
            crate::names::NameKind::FullyQualified,
            target.split('\\').map(str::to_string).collect(),
        ));
        let condition = Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(static_class()),
                op: crate::parser::ast::BinOp::StrictEq,
                right: Box::new(Expr::new(ExprKind::StringLiteral(target.clone()), span)),
            },
            span,
        );
        let call = Expr::new(
            ExprKind::StaticMethodCall {
                receiver: named,
                method: method.to_string(),
                args: args.to_vec(),
            },
            span,
        );
        ladder = Expr::new(
            ExprKind::Ternary {
                condition: Box::new(condition),
                then_expr: Box::new(call),
                else_expr: Box::new(ladder),
            },
            span,
        );
    }
    let value = lower_expr(ctx, &ladder);
    let value_ty = ctx.builder.value_php_type(value.value);
    if matches!(value_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        return Some(value);
    }
    Some(ctx.box_value_as_mixed(value, value_ty, Some(span)))
}

/// Resolves a statically knowable `Closure::bind()` visibility scope for closure lowering.
///
/// The runtime descriptor only needs the rebound receiver, but `self`, `parent`, private members,
/// and static properties inside the closure are lexical constructs that must be resolved while
/// its EIR body is built. Dynamic scope expressions remain gradual and keep the enclosing scope.
pub(super) fn closure_bind_scope_class(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<String> {
    let scope = args.get(2)?;
    match &scope.kind {
        ExprKind::StringLiteral(name) => Some(name.trim_start_matches('\\').to_string()),
        ExprKind::ClassConstant { receiver } => match receiver {
            // A generic receiver is instantiated into an ordinary named one before type checking;
            // a template has no class to reach through.
            StaticReceiver::Generic(_) => unreachable!(
                "StaticReceiver::Generic must be instantiated by generics::classes"
            ),
            StaticReceiver::Named(name) => {
                Some(name.as_str().trim_start_matches('\\').to_string())
            }
            StaticReceiver::Self_ | StaticReceiver::Static => ctx.current_class.clone(),
            StaticReceiver::Parent => ctx
                .current_class
                .as_ref()
                .and_then(|class| ctx.classes.get(class))
                .and_then(|class| class.parent.clone()),
        },
        _ => instance_callable_object_class(ctx, scope),
    }
}

/// How a `Closure::bind` target types the closure's `$this` capture.
pub(super) enum BoundThis {
    /// The target's class is known, so the capture is that precise object type.
    Class(String),
    /// The target names no class this build has, so members dispatch at run time.
    Gradual,
}

/// Returns how `Closure::bind`'s new `$this` types the bound closure's capture, if at all.
///
/// Only a closure LITERAL qualifies — a bind of an already-built closure value cannot retype a
/// body that was compiled elsewhere. A literal `null` target unbinds `$this` rather than typing
/// it. Everything else is the TARGET's type, precise when `instance_callable_object_class`
/// resolves a class and gradual otherwise: a property of an absent extension class, a `clone` of
/// one, a `mixed` return.
///
/// Kept in step with `closure_bind_property_receiver_type` in the checker, which asks the same
/// question of the same two arguments. The two answering differently is the defect this pair
/// exists to prevent: the checker would type `$this->p` against one class and the backend emit it
/// against another — which is what happened while only one side knew, in both directions.
pub(super) fn bound_this_capture(ctx: &LoweringContext<'_, '_>, args: &[Expr]) -> Option<BoundThis> {
    if !matches!(args.first().map(|arg| &arg.kind), Some(ExprKind::Closure { .. })) {
        return None;
    }
    let new_this = args.get(1)?;
    if matches!(new_this.kind, ExprKind::Null) {
        return None;
    }
    Some(match instance_callable_object_class(ctx, new_this) {
        Some(class_name) => BoundThis::Class(class_name),
        None => BoundThis::Gradual,
    })
}

/// Returns preserved late-static return syntax for EIR static dispatch.
pub(super) fn static_method_late_static_return_for_ir(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
) -> Option<TypeExpr> {
    let class_name = static_receiver_class_name(ctx, receiver)?;
    let method_key = php_symbol_key(method);
    let class_info = ctx.classes.get(&class_name)?;
    if static_method_implementation_signature(ctx, receiver, method).is_some() {
        return class_info
            .late_static_static_method_returns
            .get(&method_key)
            .cloned();
    }
    lexical_instance_static_call_signature(ctx, receiver, method)?;
    class_info.late_static_method_returns.get(&method_key).cloned()
}

/// Resolves the receiver type used to bind `static` for an EIR static-style call.
pub(super) fn static_late_binding_receiver_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
) -> Option<String> {
    // A receiver written `Box<int>::of()` names `Box` until instantiation renames
    // it, and this pass can run on a generic function's template body — which is
    // walked and then stripped, never instantiated.
    let receiver = &receiver.written_class_receiver();
    match receiver {
        // A generic receiver is instantiated into an ordinary named one before type checking;
        // a template has no class to reach through.
        StaticReceiver::Generic(_) => unreachable!(
            "StaticReceiver::Generic must be instantiated by generics::classes"
        ),
        StaticReceiver::Named(name) => Some(name.as_str().trim_start_matches('\\').to_string()),
        StaticReceiver::Self_ | StaticReceiver::Static | StaticReceiver::Parent => {
            ctx.current_class.clone()
        }
    }
}

/// PHP coerces a numeric string to the integer backing value for an int-backed enum's
/// `from()`/`tryFrom()`. When the sole argument lowered to a string, insert an explicit
/// `EnumBackingStringToInt` coercion (issue #349) so the enum call receives a plain integer
/// operand: the backing scan then runs on an int rather than a heap string, and a
/// non-numeric string throws `TypeError` inside the coercion at runtime. Non-matching
/// receivers/methods/argument types pass the operands through unchanged.
pub(super) fn coerce_int_backed_enum_string_argument(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
    mut operands: Vec<crate::ir::ValueId>,
    expr: &Expr,
) -> Vec<crate::ir::ValueId> {
    let key = php_symbol_key(method);
    if (key != "from" && key != "tryfrom") || operands.len() != 1 {
        return operands;
    }
    let StaticReceiver::Named(name) = receiver else {
        return operands;
    };
    let enum_name = name.trim_start_matches('\\');
    let is_int_backed = ctx
        .enums
        .get(enum_name)
        .and_then(|info| info.backing_type.as_ref())
        .is_some_and(|backing| matches!(backing, PhpType::Int));
    if !is_int_backed {
        return operands;
    }
    let method_display = if key == "tryfrom" { "tryFrom" } else { "from" };
    // A `string` argument coerces via a strict numeric probe; a `Mixed` argument dispatches
    // on its runtime tag (int/bool/float/null coerce, string coerces, others `TypeError`).
    // The string op carries the full message; the Mixed op carries the message prefix and
    // appends the runtime type word in codegen.
    let (op, message) = match ctx.builder.value_php_type(operands[0]).codegen_repr() {
        PhpType::Str => (
            Op::EnumBackingStringToInt,
            format!(
                "{}::{}(): Argument #1 ($value) must be of type int, string given",
                enum_name, method_display
            ),
        ),
        PhpType::Mixed | PhpType::Union(_) => (
            Op::EnumBackingMixedToInt,
            format!(
                "{}::{}(): Argument #1 ($value) must be of type int, ",
                enum_name, method_display
            ),
        ),
        _ => return operands,
    };
    let message_data = ctx.intern_string(&message);
    let coerced = ctx.emit_value(
        op,
        vec![operands[0]],
        Some(Immediate::Data(message_data)),
        PhpType::Int,
        op.default_effects(),
        Some(expr.span),
    );
    operands[0] = coerced.value;
    operands
}

/// Builds synthetic `__callStatic` arguments when a class lacks the requested static method.
pub(super) fn magic_static_call_dispatch_args(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
    args: &[Expr],
    span: Span,
) -> Option<Vec<Expr>> {
    if static_method_implementation_signature(ctx, receiver, method).is_some()
        || lexical_instance_static_call_signature(ctx, receiver, method).is_some()
    {
        return None;
    }
    let class_name = static_receiver_class_name(ctx, receiver)?;
    let class_info = ctx.classes.get(class_name.as_str())?;
    if class_info.methods.contains_key(&php_symbol_key(method)) {
        return None;
    }
    static_method_implementation_signature(ctx, receiver, "__callStatic")?;
    Some(vec![
        Expr::new(ExprKind::StringLiteral(method.to_string()), span),
        Expr::new(ExprKind::ArrayLiteral(args.to_vec()), span),
    ])
}

/// Lowers a static-method callable-array call through a descriptor invoker.
pub(super) fn lower_static_method_descriptor_call(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let sig = static_method_implementation_signature(ctx, receiver, method).cloned();
    let wrapper_sig = sig
        .as_ref()
        .map(crate::codegen::callable_dispatch::static_method_runtime_wrapper_sig);
    let result_type = sig
        .as_ref()
        .map(|signature| normalize_value_php_type(crate::types::eir_value_php_type(&signature.return_type)))
        .unwrap_or_else(|| fallback_expr_type(expr));
    let target = CallableTarget::StaticMethod {
        receiver: receiver.clone(),
        method: method.to_string(),
    };
    let result_staging = prepublish_call_result(ctx, &result_type, expr.span);
    let descriptor = lower_first_class_callable(ctx, &target, expr);
    let (descriptor, descriptor_owner) = root_owned_call_operand(ctx, descriptor, expr.span);
    let mut operands = vec![descriptor.value];
    operands.extend(lower_args_with_signature(ctx, wrapper_sig.as_ref(), args));
    let result = emit_descriptor_invoker_value(
        ctx,
        Op::ExprCall,
        operands,
        callable_profile_immediate(),
        result_type,
        expr.span,
    );
    stage_call_result(ctx, result_staging.as_ref(), result, expr.span);
    if let Some(slot) = descriptor_owner {
        retire_owned_call_operand(ctx, slot, expr.span);
    }
    take_prepublished_call_result(ctx, result_staging, result, expr.span)
}

/// Lowers a static-method descriptor call when operands have already been evaluated.
pub(super) fn lower_static_method_descriptor_value_call(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
    args: Vec<crate::ir::ValueId>,
    expr: &Expr,
) -> Option<LoweredValue> {
    let sig = static_method_implementation_signature(ctx, receiver, method).cloned();
    let result_type = sig
        .as_ref()
        .map(|signature| normalize_value_php_type(crate::types::eir_value_php_type(&signature.return_type)))
        .unwrap_or_else(|| fallback_expr_type(expr));
    let target = CallableTarget::StaticMethod {
        receiver: receiver.clone(),
        method: method.to_string(),
    };
    let result_staging = prepublish_call_result(ctx, &result_type, expr.span);
    let descriptor = lower_first_class_callable(ctx, &target, expr);
    let (descriptor, descriptor_owner) = root_owned_call_operand(ctx, descriptor, expr.span);
    let mut operands = Vec::with_capacity(args.len() + 1);
    operands.push(descriptor.value);
    operands.extend(args);
    let result = emit_descriptor_invoker_value(
        ctx,
        Op::ExprCall,
        operands,
        callable_profile_immediate(),
        result_type,
        expr.span,
    );
    stage_call_result(ctx, result_staging.as_ref(), result, expr.span);
    if let Some(slot) = descriptor_owner {
        retire_owned_call_operand(ctx, slot, expr.span);
    }
    Some(take_prepublished_call_result(
        ctx,
        result_staging,
        result,
        expr.span,
    ))
}

/// Returns the conservative return-to-argument alias summary for static dispatch.
pub(super) fn static_method_return_arg_alias(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
) -> ReturnArgAlias {
    let Some(class_name) = static_receiver_class_name(ctx, receiver) else {
        return ReturnArgAlias::Unknown;
    };
    let method_key = php_symbol_key(method);
    let Some(class_info) = ctx.classes.get(&class_name) else {
        return ReturnArgAlias::Unknown;
    };
    if !matches!(receiver, StaticReceiver::Static)
        || class_info.is_final
        || class_info.final_static_methods.contains(&method_key)
    {
        return class_static_method_return_arg_alias(ctx, &class_name, &method_key)
            .unwrap_or(ReturnArgAlias::Unknown);
    }

    let mut summary: Option<ReturnArgAlias> = None;
    let family = ctx.return_alias_summaries.class_family(ctx.classes, &class_name);
    for candidate in family.iter() {
        let Some(alias) = class_static_method_return_arg_alias(ctx, candidate, &method_key) else {
            continue;
        };
        summary = Some(match summary {
            Some(current) => current.merge(&alias),
            None => alias,
        });
    }
    summary.unwrap_or(ReturnArgAlias::Unknown)
}

/// Resolves one class's static implementation and its source alias summary.
pub(super) fn class_static_method_return_arg_alias(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    method_key: &str,
) -> Option<ReturnArgAlias> {
    let class_info = ctx.classes.get(class_name)?;
    class_info.static_methods.get(method_key)?;
    let impl_class = class_info
        .static_method_impl_classes
        .get(method_key)
        .map(String::as_str)
        .unwrap_or(class_name);
    Some(
        ctx.return_alias_summaries
            .method(impl_class, method_key)
            .cloned()
            .unwrap_or(ReturnArgAlias::Unknown),
    )
}

/// Returns the implementation signature used by the static method symbol that will run.
pub(super) fn static_method_implementation_signature<'a>(
    ctx: &'a LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
) -> Option<&'a FunctionSig> {
    let class_name = static_receiver_class_name(ctx, receiver)?;
    let key = php_symbol_key(method);
    let receiver_info = ctx.classes.get(class_name.as_str())?;
    let impl_class = receiver_info
        .static_method_impl_classes
        .get(&key)
        .map(String::as_str)
        .unwrap_or(class_name.as_str());
    ctx.classes
        .get(impl_class)
        .and_then(|class_info| class_info.static_methods.get(&key))
}

/// Returns the declared result type for a static method call before its arguments are lowered.
pub(in crate::ir_lower) fn static_method_call_expr_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
) -> Option<PhpType> {
    let nominal = static_method_implementation_signature(ctx, receiver, method)
        .or_else(|| lexical_instance_static_call_signature(ctx, receiver, method))
        .map(|signature| normalize_value_php_type(crate::types::eir_value_php_type(&signature.return_type)))?;
    match (
        static_method_late_static_return_for_ir(ctx, receiver, method),
        static_late_binding_receiver_type_for_ir(ctx, receiver),
    ) {
        (Some(return_type), Some(receiver_type)) => Some(late_static_return_type_for_ir(
            ctx,
            &return_type,
            &receiver_type,
        )),
        _ => Some(nominal),
    }
}

/// Returns the instance-method signature used by `self::method()` or `parent::method()`.
pub(super) fn lexical_instance_static_call_signature<'a>(
    ctx: &'a LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
) -> Option<&'a FunctionSig> {
    if !matches!(receiver, StaticReceiver::Self_ | StaticReceiver::Parent)
        && !named_receiver_is_current_or_ancestor(ctx, receiver)
    {
        return None;
    }
    let class_name = static_receiver_class_name(ctx, receiver)?;
    let key = php_symbol_key(method);
    class_method_signature(ctx, &class_name, &key)
}

/// Returns whether a named static-syntax receiver denotes the current instance class or an ancestor.
fn named_receiver_is_current_or_ancestor(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
) -> bool {
    let StaticReceiver::Named(name) = receiver else {
        return false;
    };
    if closure_forwards_this_to_any_class(ctx) {
        return true;
    }
    let target = name.as_str().trim_start_matches('\\');
    let Some(mut current) = ctx.current_class.as_deref() else {
        return false;
    };
    loop {
        if php_symbol_key(current) == php_symbol_key(target) {
            return true;
        }
        let Some(parent) = ctx
            .classes
            .get(current)
            .and_then(|class_info| class_info.parent.as_deref())
        else {
            return false;
        };
        current = parent;
    }
}

/// Resolves a static receiver to a concrete class name when lexical metadata is available.
pub(crate) fn static_receiver_class_name(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
) -> Option<String> {
    // A receiver written `Box<int>::of()` names `Box` until instantiation renames
    // it, and this pass can run on a generic function's template body — which is
    // walked and then stripped, never instantiated.
    let receiver = &receiver.written_class_receiver();
    match receiver {
        // A generic receiver is instantiated into an ordinary named one before type checking;
        // a template has no class to reach through.
        StaticReceiver::Generic(_) => unreachable!(
            "StaticReceiver::Generic must be instantiated by generics::classes"
        ),
        StaticReceiver::Named(name) => Some(name.as_str().trim_start_matches('\\').to_string()),
        StaticReceiver::Self_ | StaticReceiver::Static => ctx.current_class.clone(),
        StaticReceiver::Parent => {
            let current = ctx.current_class.as_deref()?;
            ctx.classes.get(current).and_then(|class_info| class_info.parent.clone())
        }
    }
}
