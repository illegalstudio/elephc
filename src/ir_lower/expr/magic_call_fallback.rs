//! Purpose:
//! Gives a method call on an UNKNOWN receiver php's `__call()` fallback.
//!
//! Called from:
//! - `crate::ir_lower::expr::method_calls::lower_method_call`.
//!
//! Key details:
//! - The class-id ladder behind `$unknown->m()` lists the classes that DECLARE `m`. An object of a
//!   class that has `__call()` but no `m` is not on it, so the call died with "Call to undefined
//!   method" where php hands the name and arguments to `__call()`. A receiver whose static class
//!   is known takes that route already (`method_calls::magic_call_dispatch_args`); this covers
//!   the `mixed`, union and bare-`object` receivers an untyped parameter or array element has.
//! - The call is rewritten to
//!   `is_object($r) && !method_exists($r, 'm') ? __elephc_magic_call($r, 'm', [args]) : $r->m(args)`
//!   over a hidden copy of the receiver. The receiver was evaluated once, before this point, and
//!   each argument sits in exactly one branch, so evaluation order and counts are unchanged.
//!   Calling `__call()` through the single helper keeps a second dispatch ladder (one arm per
//!   class that declares `__call()`) out of every call site.
//! - Only programs that declare a class with `__call()` and name a class that lacks the method
//!   are touched, so the common case pays nothing. Spread and named arguments keep the plain
//!   path, and so do `__call` itself and the object-protocol methods a closure answers.

use super::*;

/// Name prefix of the hidden temporaries `declare_hidden_temp` hands out.
const HIDDEN_TEMP_PREFIX: &str = "__eir_tmp";

/// Lowers `$receiver->method(args)` with the `__call()` fallback when it applies.
///
/// `object` is the already-lowered receiver. Returns `None` when the plain dispatch should run.
pub(super) fn lower_magic_call_fallback(
    ctx: &mut LoweringContext<'_, '_>,
    object_expr: &Expr,
    object: LoweredValue,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if !receiver_may_miss_the_method(ctx, object.value) {
        return None;
    }
    if matches!(&object_expr.kind, ExprKind::Variable(name) if name.starts_with(HIDDEN_TEMP_PREFIX)) {
        return None;
    }
    if args.iter().any(|arg| {
        matches!(
            arg.kind,
            ExprKind::Spread(_) | ExprKind::NamedArg { .. }
        )
    }) {
        return None;
    }
    let key = php_symbol_key(method);
    if matches!(
        key.as_str(),
        "__call" | "__callstatic" | "__construct" | "__invoke" | "bindto" | "call" | "__get" | "__set"
    ) {
        return None;
    }
    let helper = crate::magic_call_prelude::MAGIC_CALL_HELPER_NAME;
    if !ctx.functions.contains_key(helper) || ctx.builder.function().name == helper {
        return None;
    }
    if !some_class_has_magic_call_but_not(ctx, &key) {
        return None;
    }
    let span = expr.span;
    let receiver_ty = ctx.builder.value_php_type(object.value).clone();
    let receiver = ctx.declare_hidden_temp(receiver_ty.clone());
    let object = super::nullable_method_calls::acquire_borrowed_for_hidden_temp(ctx, object, span);
    ctx.store_local(&receiver, object, receiver_ty, Some(span));
    // Each argument is spelled in BOTH branches below, so it was lowered twice, and an argument
    // that is itself a call on an unknown receiver doubled again inside: a chain of n such calls
    // lowered its innermost argument 2^n times. Arguments are now evaluated once, in order, into
    // hidden temporaries the branches share -- unless one of them is a place a by-reference
    // parameter could bind, which must reach the call as itself. Only `is_object` and
    // `method_exists`, which have no effects, move after the arguments.
    let hoisted: Vec<Expr>;
    let args: &[Expr] = if args_can_be_evaluated_once(args) {
        hoisted = args
            .iter()
            .map(|arg| {
                let value = lower_expr(ctx, arg);
                let ty = ctx.builder.value_php_type(value.value).clone();
                let temp = ctx.declare_hidden_temp(ty.clone());
                let value =
                    super::nullable_method_calls::acquire_borrowed_for_hidden_temp(ctx, value, arg.span);
                ctx.store_local(&temp, value, ty, Some(arg.span));
                Expr::new(ExprKind::Variable(temp), arg.span)
            })
            .collect();
        #[cfg(test)]
        MAGIC_CALL_HOISTED_ARGS.with(|count| count.set(count.get() + hoisted.len()));
        &hoisted
    } else {
        args
    };
    let receiver_var = || Expr::new(ExprKind::Variable(receiver.clone()), span);
    let call_named = |name: &str, call_args: Vec<Expr>| {
        Expr::new(
            ExprKind::FunctionCall {
                name: crate::names::Name::from_parts(
                    crate::names::NameKind::FullyQualified,
                    vec![name.to_string()],
                ),
                args: call_args,
            },
            span,
        )
    };
    let is_object = call_named("is_object", vec![receiver_var()]);
    let missing = Expr::new(
        ExprKind::Not(Box::new(call_named(
            "method_exists",
            vec![receiver_var(), Expr::new(ExprKind::StringLiteral(method.to_string()), span)],
        ))),
        span,
    );
    let magic = Expr::new(
        ExprKind::FunctionCall {
            name: crate::names::Name::unqualified(helper),
            args: vec![
                receiver_var(),
                Expr::new(ExprKind::StringLiteral(method.to_string()), span),
                Expr::new(ExprKind::ArrayLiteral(args.to_vec()), span),
            ],
        },
        span,
    );
    let plain = Expr::new(
        ExprKind::MethodCall {
            object: Box::new(receiver_var()),
            method: method.to_string(),
            args: args.to_vec(),
        },
        span,
    );
    let rewritten = Expr::new(
        ExprKind::Ternary {
            condition: Box::new(Expr::new(
                ExprKind::BinaryOp {
                    left: Box::new(is_object),
                    op: BinOp::And,
                    right: Box::new(missing),
                },
                span,
            )),
            then_expr: Box::new(magic),
            else_expr: Box::new(plain),
        },
        span,
    );
    Some(lower_expr(ctx, &rewritten))
}

/// Whether the arguments may be evaluated once into temporaries both branches share: some
/// argument does work worth sharing, and none is a place a by-reference parameter could bind.
fn args_can_be_evaluated_once(args: &[Expr]) -> bool {
    let is_place = |arg: &Expr| {
        matches!(
            arg.kind,
            ExprKind::Variable(_)
                | ExprKind::ArrayAccess { .. }
                | ExprKind::PropertyAccess { .. }
                | ExprKind::DynamicPropertyAccess { .. }
                | ExprKind::NullsafePropertyAccess { .. }
                | ExprKind::NullsafeDynamicPropertyAccess { .. }
                | ExprKind::StaticPropertyAccess { .. }
                | ExprKind::DynamicStaticPropertyAccess { .. }
        )
    };
    let is_literal = |arg: &Expr| {
        matches!(
            arg.kind,
            ExprKind::StringLiteral(_)
                | ExprKind::IntLiteral(_)
                | ExprKind::FloatLiteral(_)
                | ExprKind::BoolLiteral(_)
                | ExprKind::Null
        )
    };
    !args.iter().any(is_place) && !args.iter().all(is_literal)
}

#[cfg(test)]
thread_local! {
    /// Arguments evaluated once for both branches; read by `ir_lower::tests::magic_call_fallback`.
    pub(crate) static MAGIC_CALL_HOISTED_ARGS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Returns whether the receiver is typed loosely enough that its class is not statically known.
fn receiver_may_miss_the_method(ctx: &LoweringContext<'_, '_>, value: ValueId) -> bool {
    let ty = ctx.builder.value_php_type(value);
    // A nullable receiver of ONE class (`?WeakMap`) is boxed as Mixed but its class is known: the
    // nullable path dispatches it (and `__call()`) statically. Rewriting it into the ternary below
    // hid a by-reference-returning method from `$x = &$nullable->m()` and from the WeakMap
    // nested-write desugaring (`$this->weakMap[$k][] = $v`) once any class declared `__call()`.
    if super::property_access::singular_object_class(&ty)
        .is_some_and(|(name, _)| !name.trim_start_matches('\\').is_empty())
    {
        return false;
    }
    match ty.codegen_repr() {
        PhpType::Mixed => true,
        PhpType::Union(_) => super::property_access::singular_object_class(&ty).is_none(),
        PhpType::Object(name) => name.trim_start_matches('\\').is_empty(),
        _ => false,
    }
}

/// Returns whether some class declares `__call()` but not the method named by `key`.
fn some_class_has_magic_call_but_not(ctx: &LoweringContext<'_, '_>, key: &str) -> bool {
    let call_key = php_symbol_key("__call");
    ctx.classes.values().any(|class_info| {
        class_info.methods.contains_key(&call_key) && !class_info.methods.contains_key(key)
    })
}
