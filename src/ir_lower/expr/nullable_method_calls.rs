//! Purpose:
//! Nullable method calls and ownership-aware call cleanup.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Emits the PHP fatal terminator for an ordinary method call on null.
pub(super) fn terminate_method_call_on_null(
    ctx: &mut LoweringContext<'_, '_>,
    method: &str,
    span: Span,
) {
    terminate_method_call_on_non_object(ctx, method, "null", span);
}

/// Emits the PHP fatal terminator for an ordinary method call on `false`.
pub(super) fn terminate_method_call_on_false(
    ctx: &mut LoweringContext<'_, '_>,
    method: &str,
    span: Span,
) {
    terminate_method_call_on_non_object(ctx, method, "false", span);
}

/// Emits the PHP fatal terminator naming the runtime receiver type.
fn terminate_method_call_on_non_object(
    ctx: &mut LoweringContext<'_, '_>,
    method: &str,
    receiver: &str,
    span: Span,
) {
    let message = format!("Call to a member function {}() on {}", method, receiver);
    let message = ctx.intern_string(&message);
    ctx.emit_void(
        Op::ThrowError,
        Vec::new(),
        Some(Immediate::Data(message)),
        Op::ThrowError.default_effects(),
        Some(span),
    );
    ctx.builder.terminate(Terminator::Unreachable);
}

/// Lowers `bindTo()`, `call()` and `__invoke()` on a closure the compiler only knows as `mixed`.
///
/// A closure kept in a static property or returned from `??=` reads back boxed, and a boxed
/// receiver has no Closure candidate in the class-id dispatch: `bindTo` fell through to the eval
/// bridge, which cannot rebind a compiled closure, and failed with "eval() runtime failed".
/// DeepClone keeps its hydrator exactly that way (`self::$hydrators['stdClass'] ??= static fn`,
/// then `->bindTo(null, $class)`). When no class declares a method of that name the receiver can
/// only be a closure, so it is narrowed to a callable (a guarded conversion) and takes the same
/// path a `callable`-typed receiver takes.
fn lower_boxed_closure_method_call(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if !matches!(
        ctx.builder.value_php_type(object.value).codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return None;
    }
    let key = php_symbol_key(method);
    if !matches!(key.as_str(), "bindto" | "call" | "__invoke") {
        return None;
    }
    if ctx.classes.iter().any(|(class_name, class_info)| {
        php_symbol_key(class_name.trim_start_matches('\\')) != "closure"
            && class_info.methods.contains_key(&key)
    }) {
        return None;
    }
    let callable = crate::ir_lower::gradual_coercions::coerce_gradual_value_to_boundary(
        ctx,
        object,
        &PhpType::Callable,
        Some(expr.span),
    );
    if key == "__invoke" {
        return Some(lower_expr_call_from_value(ctx, callable, args, expr));
    }
    let receiver_expr = match &expr.kind {
        ExprKind::MethodCall { object, .. } | ExprKind::NullsafeMethodCall { object, .. } => {
            object.as_ref()
        }
        _ => expr,
    };
    lower_closure_bind_method(ctx, receiver_expr, &callable, method, args, expr)
}

/// Lowers `$boxed->m(...)` as `call_user_func_array([$boxed, 'm'], [...])` when some class
/// declares `m` variadic.
///
/// A receiver only known as `mixed` has no signature at lowering time, so its arguments go out
/// one operand each and the class-id dispatch in codegen picks the method. That dispatch has no
/// way to gather a variadic tail into the array the callee expects, so it dropped every such
/// class from its candidates and the call fataled "on null" -- `ReflectionClass::newInstance()`
/// on a reflector kept in an array, or any user `m(...$xs)`. The callable path resolves the class
/// at run time and binds positional, variadic and named arguments as a direct call would.
/// By-reference parameters are left to the direct dispatch: an argument array carries values.
pub(super) fn lower_boxed_receiver_variadic_method_call(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let receiver_ty = ctx.builder.value_php_type(object.value).codegen_repr();
    let boxed = matches!(receiver_ty, PhpType::Mixed | PhpType::Union(_));
    // A receiver typed only `object` is as open as a boxed one: any class with this method,
    // compiled or interpreted, may answer. Only named arguments need the runtime path there.
    let generic_object = matches!(
        &receiver_ty,
        PhpType::Object(name) if name.trim_start_matches('\\').is_empty()
    );
    // A class the runtime autoloader supplies is named by the program but has no compiled
    // signature to place named arguments with; lowered positionally, Laravel's
    // `$record->with(extra: [...])` filled Monolog's `...$args` with a list, and rebuilding the
    // record from it failed. The runtime binds the names.
    let runtime_only_class = matches!(&receiver_ty, PhpType::Object(name) if !name.is_empty())
        && ctx.program_loads_code_at_run_time()
        && super::method_metadata::method_signature(ctx, object.value, method).is_none();
    if !boxed
        && !((generic_object || runtime_only_class) && crate::types::call_args::has_named_args(args))
    {
        return None;
    }
    // The hidden `func_get_args()` collector is not a source variadic: the class-id dispatch
    // packs it, and once eval or a backtrace is in the program every method carries one.
    let shape = super::array_access_types::boxed_method_shape(ctx, &php_symbol_key(method));
    if shape.by_ref_param {
        return None;
    }
    let declares_variadic = shape.source_variadic;
    // Named arguments against a boxed receiver cannot be matched to positions at compile time
    // either: the object may be an instance of any class with this method, or of one only the
    // interpreter knows, and the positional ABI dropped the names — `$builder->withRouting(web:
    // $w)` on an interpreted builder handed `$w` to the FIRST parameter. `call_user_func_array`
    // with the names as keys is php's own spelling of the same call.
    if !declares_variadic && !crate::types::call_args::has_named_args(args) {
        return None;
    }
    let span = expr.span;
    let argument_array = boxed_receiver_argument_array(args, span)?;
    // A plain hidden temp holding a reference of its own, as the dynamic-method path does. A
    // one-shot OWNED temp was read twice -- the callable array's element load moved it out, and
    // the explicit take after the call released it again -- which only balanced while that array
    // leaked the element it had retained.
    let receiver = ctx.declare_hidden_temp(receiver_ty.clone());
    let object = acquire_borrowed_for_hidden_temp(ctx, object, span);
    ctx.store_local(&receiver, object, receiver_ty, Some(span));
    let call = Expr::new(
        ExprKind::FunctionCall {
            name: crate::names::Name::unqualified("call_user_func_array"),
            args: vec![
                Expr::new(
                    ExprKind::ArrayLiteral(vec![
                        Expr::new(ExprKind::Variable(receiver.clone()), span),
                        Expr::new(ExprKind::StringLiteral(method.to_string()), span),
                    ]),
                    span,
                ),
                argument_array,
            ],
        },
        span,
    );
    Some(lower_expr(ctx, &call))
}

/// Returns the argument array `call_user_func_array()` takes for one call's source arguments:
/// positional arguments at their index, named ones under their name, spreads spliced in.
/// A spread mixed with named arguments has no literal spelling here and keeps the direct path.
fn boxed_receiver_argument_array(args: &[Expr], span: crate::span::Span) -> Option<Expr> {
    let has_named = args
        .iter()
        .any(|arg| matches!(arg.kind, ExprKind::NamedArg { .. }));
    if !has_named {
        return Some(Expr::new(ExprKind::ArrayLiteral(args.to_vec()), span));
    }
    let mut entries = Vec::with_capacity(args.len());
    for (index, arg) in args.iter().enumerate() {
        match &arg.kind {
            ExprKind::Spread(_) => return None,
            ExprKind::NamedArg { name, value } => entries.push((
                Expr::new(ExprKind::StringLiteral(name.clone()), span),
                value.as_ref().clone(),
            )),
            _ => entries.push((
                Expr::new(ExprKind::IntLiteral(index as i64), span),
                arg.clone(),
            )),
        }
    }
    Some(Expr::new(ExprKind::ArrayLiteralAssoc(entries), span))
}

/// Lowers a nullsafe method call with lazy argument evaluation for nullable receivers.
pub(super) fn lower_nullsafe_method_call(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let object = lower_expr(ctx, object);
    lower_nullsafe_method_call_from_receiver(ctx, object, method, args, expr)
}

/// Guards a prepared receiver, including missing elements whose declared type excludes null.
pub(super) fn lower_nullsafe_method_call_from_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let object_ty = ctx.builder.value_php_type(object.value);
    if value_is_definitely_null(ctx, object.value) {
        release_owning_receiver_temporary(ctx, object, expr.span);
        return lower_boxed_null(ctx, expr);
    }
    // `$f?->__invoke()` over a `?\Closure` is the same null guard around the same closure call,
    // and a callable is not an object class, so `singular_object_class` cannot see it. Symfony's
    // `TraceableAdapter` writes `$this->disabled?->__invoke()` on every operation it traces.
    let invokes_a_nullable_callable = php_symbol_key(method) == "__invoke"
        && callable_receiver_nullability(&object_ty) == Some(true);
    let nullable = invokes_a_nullable_callable
        || singular_object_class(&object_ty).is_some_and(|(_, nullable)| nullable);
    let container_miss = value_may_carry_container_miss(ctx, object.value);
    if !nullable && !container_miss {
        return lower_method_call_with_receiver(
            ctx,
            object,
            method,
            args,
            Op::NullsafeMethodCall,
            expr,
        );
    }
    let result_type = if invokes_a_nullable_callable {
        // The closure's own return type is not recoverable from the receiver's `callable`, and
        // the guard adds null on the other arm, so the merge temp holds a boxed value.
        PhpType::Mixed
    } else {
        method_call_result_type(ctx, object.value, method, Op::NullsafeMethodCall, expr)
    };
    let result_type = if container_miss && !nullable && result_type.codegen_repr() != PhpType::Mixed {
        nullable_result_type(result_type)
    } else { result_type };
    lower_guarded_nullsafe_receiver(ctx, object, result_type, expr, |ctx, object| {
        if invokes_a_nullable_callable {
            lower_expr_call_from_value(ctx, object, args, expr)
        } else if let Some(object_ty) =
            super::override_arity::nullable_receiver_ladder_type(ctx, object, method, args)
        {
            // The null arm is already split off, so this is the same call a plain `->` makes on a
            // proven-non-null nullable receiver: the class-id ladder has to be in charge of it, or an
            // implementation whose parameters are stored differently from the interface's (`mixed
            // $key` against `string $key`) is entered through the interface table with the wrong ABI.
            lower_call_on_unboxed_receiver(ctx, object, object_ty, method, args, expr)
        } else {
            lower_method_call_with_receiver(ctx, object, method, args, Op::NullsafeMethodCall, expr)
        }
    })
}

/// Runs an already prepared call only for a non-null receiver, merging into nullable storage.
pub(super) fn lower_guarded_nullsafe_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    result_type: PhpType,
    expr: &Expr,
    emit_call: impl FnOnce(&mut LoweringContext<'_, '_>, LoweredValue) -> LoweredValue,
) -> LoweredValue {
    let temp_name = ctx.declare_hidden_temp(result_type.clone());
    let null_block = ctx.builder.create_named_block("nullsafe.method.null", Vec::new());
    let call_block = ctx.builder.create_named_block("nullsafe.method.call", Vec::new());
    let merge = ctx.builder.create_named_block("nullsafe.method.merge", Vec::new());
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![object.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(expr.span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: null_block,
        then_args: Vec::new(),
        else_target: call_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(null_block);
    release_owning_receiver_temporary(ctx, object, expr.span);
    let null_value = lower_null(ctx, expr);
    let null_value = if result_type.codegen_repr() == PhpType::Mixed {
        ctx.box_value_as_mixed(null_value, result_type.clone(), Some(expr.span))
    } else {
        null_value
    };
    store_value_into_temp(ctx, &temp_name, result_type.clone(), null_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(call_block);
    let call = emit_call(ctx, object);
    store_value_into_temp(ctx, &temp_name, result_type.clone(), call, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    ctx.load_local(&temp_name, Some(expr.span))
}

/// Fills in omitted trailing parameters when one of them is BY REFERENCE.
///
/// PHP allows `function f($a, array &$out = null)` and a caller that passes only `$a`: the callee
/// writes through `&$out` and the write goes nowhere the caller can see. elephc needs somewhere
/// for it to go — a by-reference parameter is a cell address, and the default-argument path
/// materializes a VALUE, which left the call one operand short of the callee's ABI.
///
/// Each missing position gets a synthetic local seeded with that parameter's default, passed
/// positionally, so the ordinary by-reference machinery (`prepare_ref_place_args` and the
/// write-back after it) applies with no special case. `None` when nothing needs padding, which is
/// every call that supplies all its by-reference arguments.
///
/// Symfony's `ResolveEnvPlaceholdersPass::processValue` calls
/// `ContainerBuilder::resolveEnvPlaceholders($value, true)` past `array &$usedEnvs = null`.
pub(super) fn pad_omitted_by_ref_args(
    ctx: &mut LoweringContext<'_, '_>,
    method: &str,
    sig: &FunctionSig,
    args: &[Expr],
) -> Option<Vec<Expr>> {
    if crate::types::call_args::has_named_args(args) || args.iter().any(is_spread_arg) {
        return None;
    }
    // A by-reference VARIADIC (`int &...$tail`) is never padded: omitting it passes zero
    // arguments, not a null cell. Padding it handed the tail one `null` element, and a typed tail
    // then failed its own guard (`Argument #2 must be of type int, null given`).
    let regular = crate::types::call_args::regular_param_count(sig);
    let fill_through = sig
        .ref_params
        .iter()
        .enumerate()
        .filter(|(index, by_ref)| **by_ref && *index >= args.len() && *index < regular)
        .map(|(index, _)| index)
        .next_back()?;
    // Only a parameter this call could legally omit is filled: anything else is an arity error the
    // checker already reports, and inventing a place for it would hide that.
    let mut padded = args.to_vec();
    for index in args.len()..=fill_through {
        let (_, param_ty) = sig.params.get(index)?;
        let default = sig.defaults.get(index)?.clone();
        if !sig.ref_params.get(index).copied().unwrap_or(false) {
            padded.push(default?);
            continue;
        }
        let span = args.last().map(|arg| arg.span).unwrap_or_else(Span::dummy);
        // A callee whose closure keeps this parameter (`use (&$param)`) needs a cell that
        // outlives the call, and a synthetic local is this FRAME's slot. Passing the default
        // itself leaves the cell to the backend, which gives such a callee heap storage
        // (`RefArgCellLifetime::for_callee`).
        if crate::ir_lower::context::ref_param_may_escape(method) {
            padded.push(default.unwrap_or_else(|| Expr::new(ExprKind::Null, span)));
            continue;
        }
        let value_type = normalize_value_php_type(param_ty.codegen_repr());
        let temp = ctx.declare_synthetic_php_local(value_type.clone());
        let seed = match default {
            Some(default) => lower_expr(ctx, &default),
            None => lower_null(ctx, &Expr::new(ExprKind::Null, span)),
        };
        ctx.store_local(&temp, seed, value_type, Some(span));
        padded.push(Expr::new(ExprKind::Variable(temp), span));
    }
    Some(padded)
}

/// Returns whether a receiver is a callable, and whether null is among its values.
///
/// `$f->__invoke(...)` and `$f?->__invoke(...)` are the closure call written the long way, and a
/// callable is not a class — `singular_object_class` necessarily says `None` here, which left
/// every such call without a lowering at all. `?\Closure` is the shape Symfony's
/// `TraceableAdapter` uses on each traced operation.
pub(super) fn callable_receiver_nullability(php_type: &PhpType) -> Option<bool> {
    match php_type {
        PhpType::Callable => Some(false),
        PhpType::Union(members) => {
            let mut callable = false;
            let mut nullable = false;
            for member in members {
                match member {
                    PhpType::Callable => callable = true,
                    PhpType::Void => nullable = true,
                    _ => return None,
                }
            }
            callable.then_some(nullable)
        }
        _ => None,
    }
}

/// Lowers a method call using an already evaluated receiver value.
pub(super) fn lower_method_call_with_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    method: &str,
    args: &[Expr],
    op: Op,
    expr: &Expr,
) -> LoweredValue {
    // The ordinary call path checks the recorded private/protected access before
    // dispatching; the nullsafe call reaches this helper directly after its own null
    // check, so it must reject the inaccessible call here instead of running the method.
    if let Some(message) = recorded_method_access_message(ctx, expr, op) {
        release_owning_receiver_temporary(ctx, object, expr.span);
        return crate::ir_lower::stmt::lower_throw_access_error_expr(ctx, &message, expr.span);
    }
    // The ordinary call path applies this gate before dispatching; the nullsafe call reaches
    // this helper directly after its own null check, so it needs the same static-only reroute.
    if op == Op::NullsafeMethodCall {
        if let Some(result) = try_static_only_method_through_receiver(
            ctx, object, method, args, expr,
        ) {
            return result;
        }
    }
    let object = refine_member_receiver(ctx, object, expr);
    lower_method_call_with_refined_receiver(ctx, object, method, args, op, expr)
}

/// [`lower_method_call_with_receiver`] for a receiver whose static class is already settled.
///
/// `refine_member_receiver` re-types the value to the class the checker recorded for the member
/// expression. A receiver the override ladder narrowed to a descendant (`Op::Move` to the wider
/// override's class) must keep that class, or the call is lowered against the ancestor's
/// signature again and the override reads arguments the site never materialized.
pub(super) fn lower_method_call_with_refined_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    method: &str,
    args: &[Expr],
    op: Op,
    expr: &Expr,
) -> LoweredValue {
    if let Some(call) = enum_interface_calls::lower_enum_interface_call(ctx, object, method, args, op, expr) {
        return call;
    }
    if let Some(call) = selected_native_methods::lower(ctx, object, method, args, op, expr) {
        return call;
    }
    lower_method_call_with_receiver_direct(ctx, object, method, args, op, expr)
}

/// Redirects `$receiver->method()` to the static-method path when only a static method exists.
///
/// The receiver has already been evaluated and, for a nullsafe chain, has already passed its
/// null guard. Returns `None` when the class declares an instance method of the same name, so
/// the caller keeps ordinary instance dispatch.
pub(super) fn try_static_only_method_through_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    // A rerouted static call must still honor the access check the checker recorded for
    // the receiver form; otherwise `(new Box)->privateStatic()` would run the method.
    if let Some(message) = recorded_method_access_message(ctx, expr, Op::MethodCall) {
        release_owning_receiver_temporary(ctx, object, expr.span);
        return Some(crate::ir_lower::stmt::lower_throw_access_error_expr(
            ctx, &message, expr.span,
        ));
    }
    let class_name = static_only_method_class(ctx, object.value, method)?;
    let receiver = static_receiver_for_class_name(&class_name);
    let pins = pin_in_flight_owners(ctx, &[object.value], expr.span);
    let result = lower_object_static_method_call(ctx, object.value, &receiver, method, args, expr);
    unpin_in_flight_owners(ctx, pins, expr.span);
    release_owning_receiver_temporary(ctx, object, expr.span);
    Some(result)
}

/// Lowers an already selected concrete or fallback receiver without redispatching its interface.
pub(super) fn lower_method_call_with_receiver_direct(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, method: &str,
    args: &[Expr], op: Op, expr: &Expr,
) -> LoweredValue {
    lower_method_call_with_binding(ctx, object, method, args, op, expr, false)
}

/// Checks the selected enum signature without changing direct reference-return adoption.
pub(super) fn lower_selected_enum_method_call(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, method: &str,
    args: &[Expr], op: Op, expr: &Expr,
) -> LoweredValue {
    lower_method_call_with_binding(ctx, object, method, args, op, expr, true)
}

/// Keeps selected parameter validation inside the published argument and result-owner scope.
fn lower_method_call_with_binding(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, method: &str,
    args: &[Expr], op: Op, expr: &Expr, selected_enum: bool,
) -> LoweredValue {
    if let Some(error) = super::method_calls::lower_method_call_on_scalar(ctx, object, method, op, expr) {
        return error;
    }
    if let Some(call) = magic_method_calls::lower(ctx, object, method, args, op, expr) {
        return call;
    }
    if op == Op::MethodCall
        && is_reflection_class_new_instance_call(ctx, object.value, method)
        && reflection_class_new_instance_forwards_statically(ctx, None, args)
    {
        return lower_reflection_class_new_instance(ctx, None, object, args, expr);
    }
    if op == Op::MethodCall
        && is_reflection_class_new_instance_args_call(ctx, object.value, method)
        && reflection_class_new_instance_args_forward_statically(ctx, None, args)
    {
        return lower_reflection_class_new_instance_args(ctx, None, object, args, expr);
    }
    if op == Op::MethodCall
        && is_reflection_class_new_instance_without_constructor_call(ctx, object.value, method)
    {
        return lower_reflection_class_new_instance_without_constructor(ctx, object, args, expr);
    }
    if op == Op::MethodCall {
        if let Some(call) = lower_boxed_closure_method_call(ctx, object, method, args, expr) {
            return call;
        }
        if let Some(call) = lower_boxed_receiver_variadic_method_call(ctx, object, method, args, expr) {
            return call;
        }
    }
    // A nullable (`?C`) or `Object|false` receiver of a static-only method: the receiver type
    // `try_static_only_method_through_receiver` answers for is a bare object.
    if let Some(receiver) = object_static_method_receiver(ctx, object.value, method) {
        let call = lower_static_method_call(ctx, &receiver, method, args, expr);
        release_owning_receiver_temporary(ctx, object, expr.span);
        return call;
    }
    let dispatch_method = method;
    // Taken before any argument is lowered, so a nested call never inherits it.
    let direct_class = ctx.direct_method_call_class.take();
    let result_type = method_call_result_type(ctx, object.value, dispatch_method, op, expr);
    let sig = method_signature_for_call(ctx, object.value, dispatch_method, args.len());
    // A spread supplies an unknown number of arguments, so `args.len()` says nothing about which
    // callee accepts them: `$o->deprecate($k, ...$rest)` counts two against a three-parameter
    // method, drops its signature, and used to hand the spread array to the boxed dispatch as ONE
    // plain operand. The regular-receiver path routes such a call through the descriptor invoker,
    // which applies php's positional/named binding at run time; this boxed-receiver path must too.
    //
    // Not for an enum branch an interface call selected: `enum_method_args` binds that call's
    // runtime unpack itself, converting each argument and releasing its source BEFORE the method
    // body runs, as php's RECV does. Through the descriptor invoker the conversion happened at
    // first use instead, so `$target->v(...$args)` printed the body's output before `__toString`
    // and the source's destructor.
    if !(selected_enum && sig.is_some()) {
        if let Some(call) = lower_runtime_spread_method_call(
        ctx,
        object,
        dispatch_method,
        args,
        sig.as_ref(),
        result_type.clone(),
        expr,
    ) {
        return call;
    }
    }
    // An omitted BY-REFERENCE parameter still needs a cell to be written through.
    let padded_args;
    let args = match sig
        .as_ref()
        .and_then(|signature| pad_omitted_by_ref_args(ctx, dispatch_method, signature, args))
    {
        Some(padded) => {
            padded_args = padded;
            padded_args.as_slice()
        }
        None => args,
    };
    // php refuses a `__call` that takes its arguments by reference: nothing to promote or bind.
    if !method.eq_ignore_ascii_case("__call") {
        promote_eval_bridge_method_argument_locals(ctx, object.value, sig.as_ref(), args);
    }
    promote_pdo_binding_ref_argument(ctx, object.value, dispatch_method, args);
    // A by-reference-returning method transfers a lease that must outlive this caller's
    // argument and receiver cleanup, so its staging is published before the arguments.
    // A receiver whose class is only known at run time may be any class declaring the method, so the
    // signature found for it is one candidate's: the callee is not known to transfer a reference
    // cell, and the dispatch hands the by-value caller a snapshot when it finds one that does.
    let erased_receiver = match ctx.builder.value_php_type(object.value) {
        PhpType::Mixed => true,
        PhpType::Union(members) => members.iter().any(|member| matches!(member, PhpType::Mixed)),
        _ => false,
    };
    let reference_sig = if erased_receiver { None } else { sig.as_ref() };
    let reference_staging = begin_reference_return_call(ctx, reference_sig, expr.span);
    // An ordinary owned result gets the same protection as the reference lease: argument roots,
    // evaluation intermediates and an owning receiver all retire after the call and run PHP
    // destructors that can throw. Receiver and method alone decide the result type and the alias
    // summary, so both are available before any argument expression runs.
    let return_alias = method_return_arg_alias(ctx, object.value, dispatch_method);
    let result_staging = prepublish_user_call_result(
        ctx, sig.as_ref(), &return_alias, &result_type, expr.span,
    );
    let receiver_class = match ctx.builder.value_php_type(object.value).codegen_repr() {
        PhpType::Object(class_name) => Some(class_name.trim_start_matches('\\').to_string()),
        _ => None,
    };
    // A concrete local load may later unbox physical Mixed storage after eval widens
    // its slot. Acquiring a root is safe for both borrowed and independently owned loads.
    let (object, receiver_owner) = root_method_receiver(ctx, object, expr.span);
    // The rooted receiver keeps an owning temporary alive over the whole call window, so a
    // throwing argument or callee does not strand it.
    emit_native_factory_method_lookup(ctx, object, method, expr.span);
    let mut operands = vec![object.value];
    let prepared = sig
        .as_ref()
        .and_then(|signature| ref_place_args::prepare_ref_place_args(ctx, signature, args));
    let call_args = prepared
        .as_ref()
        .map(|(call_args, _)| call_args.as_slice())
        .unwrap_or(args);
    begin_call_argument_evaluation(ctx);
    let enum_args = singular_object_class(&ctx.builder.value_php_type(object.value))
        .and_then(|(class, _)| ctx.enums.contains_key(class).then(|| class.to_string()))
        .and_then(|class| sig.as_ref().and_then(|signature|
            enum_method_args::lower_arguments(ctx, signature, call_args, &class, dispatch_method, expr.span)
                .or_else(|| selected_enum.then(|| enum_static_args::lower_arguments(
                    ctx, signature, call_args, &class, dispatch_method, expr.span,
                )).flatten())));
    let call_description = receiver_class
        .map(|class_name| format!("{class_name}::{dispatch_method}"))
        .unwrap_or_else(|| dispatch_method.to_string());
    let previous_description = ctx.call_description.replace(call_description);
    let ref_place_rewrite = if enum_args.is_none() {
        sig.as_ref().and_then(|signature| {
            super::ref_place_args::rewrite_typed_property_ref_args(ctx, signature, call_args)
        })
    } else {
        None
    };
    let call_args = ref_place_rewrite
        .as_ref()
        .map_or(call_args, |rewrite| rewrite.args.as_slice());
    let arg_values =
        enum_args.unwrap_or_else(|| lower_user_args_with_signature(ctx, sig.as_ref(), call_args));
    ctx.call_description = previous_description;
    let mut arg_values = arg_values;
    let evaluation_intermediates = finish_call_argument_evaluation(ctx, &mut arg_values);
    let roots = root_user_call_operands(
        ctx,
        &mut arg_values,
        sig.as_ref(),
        &return_alias,
        &result_type,
        expr.span,
    );
    // PHP ignores surplus positionals on user methods. Their values still participate in
    // argument evaluation and cleanup, but must not reach the fixed native parameter ABI.
    let operand_limit = sig.as_ref().filter(|sig| sig.variadic.is_none())
        .map_or(arg_values.len(), |sig| sig.params.len());
    operands.extend(arg_values.iter().take(operand_limit).copied());
    super::object_construction::publish_constructor_call_strictness(
        ctx,
        dispatch_method,
        expr.span,
    );
    // `Class::method` names the implementation to run without virtual dispatch; the backend
    // splits it again (`lower_scoped_direct_method_call`).
    let data = match &direct_class {
        Some(class_name) => ctx.intern_string(&format!("{class_name}::{dispatch_method}")),
        None => ctx.intern_string(dispatch_method),
    };
    let call = ctx.emit_value(
        op,
        operands,
        Some(Immediate::Data(data)),
        result_type,
        op.default_effects(),
        Some(expr.span),
    );
    let call = finish_reference_return_call(
        ctx, call, reference_sig, reference_staging.as_ref(), expr.span,
    );
    stage_call_result(ctx, result_staging.as_ref(), call, expr.span);
    release_owned_call_arg_temporaries_with_roots(
        ctx,
        &arg_values,
        Some(call.value),
        &return_alias,
        sig.as_ref(),
        &roots,
        expr.span,
    );
    retire_call_argument_intermediates(ctx, &evaluation_intermediates);
    if let Some(rewrite) = ref_place_rewrite {
        super::ref_place_args::finish_ref_place_rewrite(ctx, rewrite, expr.span);
    }
    if let Some((_, plans)) = prepared {
        ref_place_args::write_back_ref_place_args(ctx, plans);
    }
    if let Some(owner) = receiver_owner {
        retire_owned_call_operand(ctx, owner, expr.span);
    } else {
        release_owning_receiver_temporary(ctx, object, expr.span);
    }
    let call = take_prepublished_call_result(ctx, result_staging, call, expr.span);
    finish_reference_return_value(ctx, call, reference_staging, expr.span)
}

/// Lowers a nullsafe dynamic instance method call after the receiver was evaluated and guarded.
///
/// The non-null receiver is stored in a hidden temp so the existing
/// `call_user_func([$obj, $method], ...)` lowering can be reused without
/// evaluating the original receiver expression again.
pub(in crate::ir_lower) fn lower_dynamic_method_call_with_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    method: &Expr,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    // `$object->{$method}()` is lowered through a callable array. Its runtime receiver can be an
    // eval-owned class, so retain a lazy context slot for the callable bridge to preserve the
    // current method's PHP visibility scope instead of falling back to global scope. A program
    // that cannot load PHP at run time has no eval-owned class, and the native invoker takes the
    // calling scope from the site itself.
    if ctx.program_loads_code_at_run_time() || ctx.has_eval_barrier() {
        ctx.declare_eval_context_local();
    }
    let receiver_type = strip_void_from_union(ctx.builder.value_php_type(object.value));
    let object = crate::ir_lower::gradual_coercions::coerce_gradual_value_to_boundary(
        ctx,
        object,
        &receiver_type,
        Some(expr.span),
    );
    let receiver_name = ctx.declare_hidden_temp(receiver_type.clone());
    // The hidden slot is an owner, not a borrow of the caller's variable. Keep
    // the source representation so boxed nullable receivers are not moved as raw objects.
    let retained = crate::ir_lower::ownership::acquire_if_refcounted(ctx, object, Some(expr.span));
    ctx.store_local(&receiver_name, retained, receiver_type, Some(expr.span));
    if ctx.value_is_owning_temporary(object) {
        crate::ir_lower::ownership::release_if_owned(ctx, object, Some(expr.span));
    }
    let receiver_slot = ctx.local_slots[&receiver_name];
    register_owned_call_operand(ctx, receiver_slot, expr.span);
    let (method, method_slot) = if ctx.classes.contains_key("SimpleXMLElement") {
        let method_value = lower_expr(ctx, method);
        let method_type = ctx.builder.value_php_type(method_value.value);
        let method_name = ctx.declare_hidden_temp(method_type.clone());
        store_value_into_temp(ctx, &method_name, method_type, method_value, expr.span);
        let method_slot = ctx.local_slots[&method_name];
        register_owned_call_operand(ctx, method_slot, expr.span);
        let receiver = ctx.load_local(&receiver_name, Some(expr.span));
        let method = ctx.load_local(&method_name, Some(expr.span));
        ctx.emit_void(Op::MethodLookup, vec![receiver.value, method.value], None,
            Op::MethodLookup.default_effects(), Some(expr.span));
        (Expr::new(ExprKind::Variable(method_name), expr.span), Some(method_slot))
    } else { (method.clone(), None) };
    let receiver = Expr::new(ExprKind::Variable(receiver_name), expr.span);
    let callback = Expr::new(
        ExprKind::ArrayLiteral(vec![receiver, method]),
        Span::dummy(),
    );
    let mut call_args = Vec::with_capacity(args.len() + 1);
    call_args.push(callback);
    call_args.extend(args.iter().cloned());
    let call = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("call_user_func"),
            args: call_args,
        },
        expr.span,
    );
    let result = lower_expr(ctx, &call);
    if let Some(slot) = method_slot { retire_owned_call_operand(ctx, slot, expr.span); }
    retire_owned_call_operand(ctx, receiver_slot, expr.span);
    result
}

/// Gives a hidden temporary its own reference to a value it would otherwise only borrow.
///
/// A hidden temp is released with the frame, so storing a BORROWED value in one hands the
/// epilogue a release it does not own. `$class->name::getSubscribedServices()` desugars to a
/// dynamic call whose receiver is the property's own string; the temp holding it freed the
/// `ReflectionClass`'s name at return, and the next reflection call on it read freed memory
/// (`ReflectionClassResource::loadFiles()` during every `cache:clear`).
pub(super) fn acquire_borrowed_for_hidden_temp(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) -> LoweredValue {
    // A PHP local's load counts as an owning temporary only PROVISIONALLY (builder finalization
    // prunes the releases emitted on it while the slot stays concrete); a store into a hidden
    // temp is not such a release, so the local and the temp would both own one reference. The
    // dynamic method call `$h->$m()` stored `$h` that way and released it with the temp; the
    // callable array it built leaked a reference that happened to cancel it, until that array
    // was released too and `$h` died after its first call.
    let is_named_local_load = matches!(
        ctx.builder.value_defining_op(value.value),
        Some(Op::LoadLocal | Op::LoadStaticLocal)
    ) && !ctx.value_is_owned_temp_load(value.value);
    if ctx.value_is_owning_temporary(value) && !is_named_local_load {
        return value;
    }
    crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span))
}

/// Releases normalized call arguments that cannot be returned by this call.
pub(super) fn release_owned_call_arg_temporaries(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[crate::ir::ValueId],
    result: Option<crate::ir::ValueId>,
    return_alias: &ReturnArgAlias,
    span: Span,
) {
    release_owned_call_arg_temporaries_with_signature(
        ctx,
        args,
        result,
        return_alias,
        None,
        span,
    );
}

/// Releases call arguments while accounting for fresh Mixed boxes created by the ABI.
pub(super) fn release_owned_call_arg_temporaries_with_signature(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[crate::ir::ValueId],
    result: Option<crate::ir::ValueId>,
    return_alias: &ReturnArgAlias,
    signature: Option<&FunctionSig>,
    span: Span,
) {
    release_owned_call_arg_temporaries_with_roots(
        ctx, args, result, return_alias, signature, &[], span,
    );
}

/// Retires scoped roots and ordinary temporaries in parameter order without renumbering alias facts.
pub(super) fn release_owned_call_arg_temporaries_with_roots(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[crate::ir::ValueId],
    result: Option<crate::ir::ValueId>,
    return_alias: &ReturnArgAlias,
    signature: Option<&FunctionSig>,
    roots: &[(usize, crate::ir::LocalSlotId)],
    span: Span,
) {
    let independently_owned_result = result.is_some_and(|result| {
        let representation = ctx.builder.value_php_type(result).codegen_repr();
        let operation = ctx.builder.value_defining_op(result);
        (matches!(representation, PhpType::Object(_))
            && matches!(operation, Some(Op::Call | Op::MethodCall | Op::NullsafeMethodCall | Op::StaticMethodCall))
            // Method return normalization persists a borrowed string before publishing it.
            // Even a passthrough therefore returns an owner separate from the argument.
            || matches!(representation, PhpType::Callable | PhpType::Str)
                && matches!(operation, Some(Op::MethodCall | Op::NullsafeMethodCall | Op::StaticMethodCall)))

            && signature.is_some_and(|signature| !signature.by_ref_return)
    });
    let guarded_result = result.filter(|_| independently_owned_result && args.iter().any(|value| {
        ctx.value_is_owning_temporary(LoweredValue {
            value: *value,
            ir_type: ctx.builder.value_type(*value),
        })
    }));
    if let Some(value) = guarded_result {
        guard_descriptor_container(ctx, LoweredValue {
            value,
            ir_type: ctx.builder.value_type(value),
        }, span);
    }
    for (parameter_index, value) in args.iter().enumerate() {
        ctx.unguard_call_argument(*value, span);
        if let Some((_, slot)) = roots.iter().find(|(index, _)| *index == parameter_index) {
            let pin_only = ctx.builder.value_defining_op(*value) == Some(Op::EnsureOwned)
                || ctx.builder.local_kind(*slot) == LocalKind::OwnedTemp;
            if !pin_only {
                retire_owned_call_operand(ctx, *slot, span);
                continue;
            }
            // OwnedTemp records only pin an existing operand owner. Detach that record without
            // releasing, then apply the same result-alias decision to the original producer.
            unregister_owned_call_operand(ctx, *slot, span);
            ctx.emit_void(
                Op::UnsetLocal,
                Vec::new(),
                Some(Immediate::LocalSlot(*slot)),
                Op::UnsetLocal.default_effects(),
                Some(span),
            );
        }
        let native_reference_value_view = ctx.builder.value_defining_op(*value) == Some(Op::LoadRefCellValue);
        let reference_place_view_already_released = (native_reference_value_view || signature.is_some_and(|signature| {
            signature
                .ref_params
                .get(parameter_index)
                .copied()
                .unwrap_or(false)
        }) && matches!(
            ctx.builder.value_defining_op(*value),
            Some(Op::LoadLocal | Op::LoadRefCell)
        )) && ctx.builder.function().instructions.iter().any(|inst| {
            inst.op == Op::Release && inst.operands == [*value]
        });
        if reference_place_view_already_released {
            // Source-order evaluation already transferred this incidental value view into an
            // unwind-visible intermediate. The call operand remains the original load solely so
            // ABI materialization can recover its local slot; releasing that view again after a
            // successful call would free the same detached Mixed-to-string result twice.
            continue;
        }
        let php_type = ctx.builder.value_php_type(*value);
        let lowered = LoweredValue {
            value: *value,
            ir_type: value_ir_type(&php_type),
        };
        if ctx.value_needs_release_after_use(lowered) {
            // PHP callees acquire by-value array/hash/Mixed parameters into owning shadow slots.
            // By-value returns from reference parameters also acquire or clone a separate owner.
            // Their result therefore cannot be an unretained alias of the caller's argument.
            let callee_owns = signature
                .is_some_and(|signature| signature.by_ref_return
                    || signature.returned_parameter_has_independent_owner(parameter_index));
            let independently_boxed = signature.is_some_and(|signature| {
                call_arg_gets_independent_mixed_box(signature, parameter_index, &php_type)
            });
            // The call result reuses this argument's payload — so the argument release
            // must be suppressed and the ownership left to flow through the result —
            // in either of two cases:
            //  - a summary that *may* return this parameter, under the conservative
            //    alias check (which excludes fresh checked-arithmetic boxes so an
            //    unproven callee still releases them, issue #486); or
            //  - a callee *proven* to return this parameter, where even a fresh
            //    boxed `$i + 1` argument is handed straight back and must not be
            //    released twice (issue #604).
            // `ReturnArgAlias::Parameters` is a MAY summary (a union over branches), so
            // `proven_aliases_parameter` also holds for a callee that returns the
            // parameter only conditionally (`if ($c) return $x; return 7;`). Suppressing
            // the argument release on every path then leaks the owned box on the runtime
            // paths that do not return it — the same deliberate leak-over-crash trade-off
            // the `may_alias` suppression already makes for array/hash arguments. A
            // follow-up issue tracks runtime alias disambiguation.
            // Whether the runtime can settle the question itself: two boxed payloads compare
            // as single pointers, so `ReleaseUnlessAliases` can decide per call. Where that
            // holds, the leak-versus-double-free trade-off below does not have to be made at
            // COMPILE time at all, which is what lets the fresh-box exclusion be lifted for a
            // MAY summary — see the third disjunct.
            let conditionally_releasable = result.is_some_and(|result| {
                let arg_repr = ctx.builder.value_php_type(*value).codegen_repr();
                let result_repr = ctx.builder.value_php_type(result).codegen_repr();
                matches!(arg_repr, PhpType::Mixed | PhpType::Union(_))
                    && matches!(result_repr, PhpType::Mixed | PhpType::Union(_))
            });
            let result_reuses_arg = result.is_some_and(|result| {
                (return_alias.may_alias_parameter(parameter_index)
                    && ctx.call_result_may_alias_arg(*value, result))
                    || (return_alias.proven_aliases_parameter(parameter_index)
                        && ctx.arg_and_result_types_can_alias(*value, result))
                    // A MAY summary whose argument is a fresh checked-arithmetic box.
                    // `call_result_may_alias_arg` excludes that shape on purpose: with no proof
                    // the callee hands it back, an unconditional SKIP leaks it once per call
                    // (issue #486). But the choice it was avoiding is only forced when the
                    // release has to be decided statically. `fib($n - 1)` is exactly this: the
                    // summary is `Unknown` because the body calls itself, the argument is a
                    // fresh box, and the base case `return $n;` hands it straight back — so the
                    // caller released the argument AND the result, the same cell twice, and
                    // `fib()` answered its own predecessor.
                    || (return_alias.may_alias_parameter(parameter_index)
                        && conditionally_releasable
                        && ctx.arg_and_result_types_can_alias(*value, result))
            });
            // A by-value scalar parameter receives a converted word, never the caller's
            // owning box. Even an unknown return summary cannot make that box an alias.
            let scalar_parameter_copy = signature.is_some_and(|signature| {
                !signature.ref_params.get(parameter_index).copied().unwrap_or(false)
                    && signature.params.get(parameter_index).is_some_and(|(_, ty)|
                        matches!(ty.codegen_repr(), PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void))
            });
            if !independently_owned_result && !callee_owns && !independently_boxed
                && !scalar_parameter_copy && result_reuses_arg {
                // Both suppression reasons above are MAY facts, so an unconditional skip is
                // right only on the calls that actually hand the payload back. Emitting a
                // conditional release instead lets each call decide at runtime: the codegen
                // compares the returned payload against this argument and releases it only
                // when they differ (issue #619). On a callee that genuinely always returns
                // the parameter the comparison always matches, so the #604 behaviour is
                // unchanged.
                // Only when the two payloads are directly comparable as single pointers. A
                // boxed `Mixed` result that *wraps* the argument's container holds a different
                // pointer than the container itself, so comparing them would read "not aliased"
                // for a value the result does own, and release it twice. Same restriction the
                // ABI-side cleanup slots already apply via `call_result_can_alias_mixed_temp`.
                // A string argument the caller owns outright (claimed with `EnsureOwned`, an
                // acquired copy, or a `StrPersist` of a normalized string cast) is decided the
                // same way when a USER function produced the result. A `Str` result compares
                // by payload pointer, the string's whole
                // identity: a PHP function persists every builtin result it returns, so an equal
                // pointer means the callee handed the block back and a different one means the
                // caller still owns it. Any other result that can alias it is a boxed `Mixed`
                // (`: mixed`, `?string`, `string|int`), whose return coercion persists the string
                // into a cell of its own, so the argument is never the result's storage and is
                // released outright. A builtin result is excluded: `ltrim`, `substr` and friends
                // answer with a view INTO the argument, a different pointer that still borrows
                // the argument's bytes.
                if let Some(result) = result {
                    let arg_repr = ctx.builder.value_php_type(lowered.value).codegen_repr();
                    let result_repr = ctx.builder.value_php_type(result).codegen_repr();
                    // The enclosing gate includes provisional local reads and owned
                    // user returns; final backend ownership still skips borrowed storage.
                    let owned_string_arg = arg_repr == PhpType::Str;
                    let user_call_result = matches!(
                        ctx.builder.value_defining_op(result),
                        Some(
                            Op::Call
                                | Op::FunctionVariantCall
                                | Op::MethodCall
                                | Op::NullsafeMethodCall
                                | Op::StaticMethodCall
                                | Op::ClosureCall
                                | Op::ExprCall
                                | Op::CallableDescriptorInvoke
                        )
                    );
                    // A boxed user return owns its persisted string independently of the raw
                    // argument, including temporary calls and string casts from Mixed storage.
                    // Provisional local releases remain subject to backend ownership filtering.
                    if arg_repr == PhpType::Str && result_repr == PhpType::Mixed && user_call_result {
                        crate::ir_lower::ownership::release_if_owned(ctx, lowered, Some(span));
                        continue;
                    }
                    if owned_string_arg && user_call_result {
                        if result_repr == PhpType::Str {
                            // A runtime view has no releasing owner of its own. Retire the
                            // normalized cast it borrows unless that owner became the result.
                            release_marked_cast_copies_unless_returned(ctx, lowered, result, span);
                            ctx.emit_void(
                                Op::ReleaseUnlessAliases,
                                vec![lowered.value, result],
                                None,
                                Op::ReleaseUnlessAliases.default_effects(),
                                Some(span),
                            );
                        } else {
                            crate::ir_lower::ownership::release_if_owned(ctx, lowered, Some(span));
                        }
                        continue;
                    }
                    let comparable = (matches!(arg_repr, PhpType::Mixed | PhpType::Union(_))
                        && matches!(result_repr, PhpType::Mixed | PhpType::Union(_)))
                        // Callable descriptors also have a single pointer identity. A new
                        // binding owns its captures independently of the source descriptor.
                        || arg_repr == PhpType::Callable && result_repr == PhpType::Callable;
                    if comparable {
                        ctx.emit_void(
                            Op::ReleaseUnlessAliases,
                            vec![lowered.value, result],
                            None,
                            Op::ReleaseUnlessAliases.default_effects(),
                            Some(span),
                        );
                    }
                    // `trim((string)$s)` is not itself the owned copy. The copy is the
                    // cast underneath it. A user call that may return its argument skips
                    // releasing `trim`'s result, so the copy has to be released here
                    // unless that call handed the same block back.
                    release_marked_cast_copies_unless_returned(ctx, lowered, result, span);
                }
                continue;
            }
            crate::ir_lower::ownership::release_if_owned(ctx, lowered, Some(span));
        } else if let Some(boxed) = ctx.owning_box_behind_borrowed_nominal_guard(lowered) {
            // The argument is the nullable nominal guard's FORWARDED cell, so it is borrowed and
            // the branch above releases nothing. The caller still owns the box the argument
            // expression minted for the Mixed ABI (`f(new C())` against `?C $c`), and this call
            // is its last use — release it, or the object never reaches zero and `__destruct`
            // never runs.
            //
            // A callee that hands its parameter back returns that same cell, so decide at
            // runtime whenever the result is a comparable single pointer, the way the
            // `result_reuses_arg` suppression above does.
            match result {
                Some(result)
                    if matches!(
                        ctx.builder.value_php_type(result).codegen_repr(),
                        PhpType::Mixed | PhpType::Union(_)
                    ) =>
                {
                    ctx.emit_void(
                        Op::ReleaseUnlessAliases,
                        vec![boxed.value, result],
                        None,
                        Op::ReleaseUnlessAliases.default_effects(),
                        Some(span),
                    );
                }
                _ => crate::ir_lower::ownership::release_if_owned(ctx, boxed, Some(span)),
            }
        }
    }
    if let Some(value) = guarded_result {
        ctx.unguard_call_argument(value, span);
    }
}

/// Releases a normalized string-cast copy nested under a call argument.
///
/// `id(trim((string)$s))` skips releasing `trim`'s result because the callee may
/// return that argument. The owned copy is the cast underneath `trim`. Free that
/// copy when the callee returned a different block, and leave it alone when the
/// callee handed the same block back.
fn release_marked_cast_copies_unless_returned(
    ctx: &mut LoweringContext<'_, '_>,
    argument: LoweredValue,
    result: ValueId,
    span: Span,
) {
    let Some(inst) = ctx.builder.value_defining_instruction(argument.value) else {
        return;
    };
    if inst.op != Op::RuntimeCall {
        return;
    }
    let copies = inst
        .operands
        .iter()
        .copied()
        .filter(|operand| {
            ctx.builder
                .value_defining_instruction(*operand)
                .is_some_and(|producer| {
                    producer.op == Op::StrPersist
                        && producer.immediate == Some(Immediate::Bool(true))
                })
        })
        .collect::<Vec<_>>();
    for copy in copies {
        ctx.emit_void(
            Op::ReleaseUnlessAliases,
            vec![copy, result],
            None,
            Op::ReleaseUnlessAliases.default_effects(),
            Some(span),
        );
    }
}

/// Returns true when ABI materialization wraps a concrete argument in fresh Mixed storage.
pub(super) fn call_arg_gets_independent_mixed_box(
    signature: &FunctionSig,
    parameter_index: usize,
    source_type: &PhpType,
) -> bool {
    if signature
        .ref_params
        .get(parameter_index)
        .copied()
        .unwrap_or(false)
    {
        return false;
    }
    signature
        .params
        .get(parameter_index)
        .is_some_and(|(_, parameter_type)| {
            parameter_type.codegen_repr() == PhpType::Mixed
                && !matches!(
                    source_type.codegen_repr(),
                    PhpType::Mixed | PhpType::Union(_)
                )
        })
}

/// Makes a borrowed read result independent from an owning receiver before releasing it.
///
/// Property and indexed reads can return strings, arrays, objects, or callables
/// borrowed from the receiver. When that receiver is an owned temporary — notably
/// an object retained while unboxing a Mixed local — releasing it first could
/// destroy the result payload. Reads that already materialize an independent owned
/// value must not be acquired a second time.
pub(super) fn stabilize_borrowed_result_and_release_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: LoweredValue,
    result: LoweredValue,
    span: Span,
) -> LoweredValue {
    let boxed_local_read = ctx.builder.value_php_type(receiver.value).codegen_repr() == PhpType::Mixed
        && ctx.value_needs_release_after_use(receiver);
    if !ctx.value_is_owning_temporary(receiver) && !boxed_local_read {
        return result;
    }
    let result = if ctx.value_is_owning_temporary(result) {
        result
    } else {
        crate::ir_lower::ownership::acquire_if_refcounted(ctx, result, Some(span))
    };
    crate::ir_lower::ownership::release_if_owned(ctx, receiver, Some(span));
    result
}

/// Releases the receiver of a method call when it was an owning temporary.
///
/// A method borrows its receiver, so a receiver that is itself a temporary — the
/// result of a prior chained call (`$o->a()->b()`) or an inline `new X()->m()` —
/// has no owner once the call returns and would otherwise never reach refcount
/// zero (a leak; its destructor never runs). A plain local or `$this` receiver is
/// not an owning temporary and is left to normal scope cleanup. This must run
/// after the call is emitted (and after `return $this` has acquired its own
/// reference) so the released reference is the receiver's, not the result's.
pub(super) fn release_owning_receiver_temporary(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: LoweredValue,
    span: Span,
) {
    if ctx.value_is_owning_temporary(receiver) {
        crate::ir_lower::ownership::release_if_owned(ctx, receiver, Some(span));
    }
}
