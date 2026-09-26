//! Purpose:
//! Lazy isset and empty lowering for magic-property semantics.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers `isset()` as a lazy language construct instead of an eager builtin call.
pub(super) fn lower_lazy_isset(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "isset" {
        return None;
    }
    if crate::types::call_args::has_named_args(args) || args.iter().any(is_spread_arg) {
        return None;
    }
    if args.is_empty() {
        return Some(lower_bool_literal(ctx, false, expr));
    }

    let temp_name = ctx.declare_hidden_temp(PhpType::Bool);
    let false_block = ctx.builder.create_named_block("isset.lazy_false", Vec::new());
    let merge = ctx.builder.create_named_block("isset.lazy_merge", Vec::new());
    for (idx, arg) in args.iter().enumerate() {
        let checked = lower_lazy_isset_operand(ctx, arg).unwrap_or_else(|| {
            // `isset()` never emits undefined-offset warnings, so eager array
            // operands must be lowered with the silent read variants.
            let value = if let ExprKind::ArrayAccess { array, index } = &arg.kind {
                lower_array_access_with_missing_warning(ctx, array, index, arg, false)
            } else {
                lower_expr(ctx, arg)
            };
            emit_builtin_call_value(ctx, name, vec![value.value], PhpType::Int, arg.span, None)
        });
        let then_target = if idx + 1 == args.len() {
            ctx.builder.create_named_block("isset.lazy_true", Vec::new())
        } else {
            ctx.builder.create_named_block("isset.lazy_next", Vec::new())
        };
        ctx.builder.terminate(Terminator::CondBr {
            cond: checked.value,
            then_target,
            then_args: Vec::new(),
            else_target: false_block,
            else_args: Vec::new(),
        });
        ctx.builder.position_at_end(then_target);
    }

    let true_value = lower_bool_literal(ctx, true, expr);
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, true_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(false_block);
    let false_value = lower_bool_literal(ctx, false, expr);
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, false_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    Some(take_owned_temp(ctx, &temp_name, expr.span))
}

/// Lowers a single `isset()` operand that has special lazy PHP semantics.
pub(super) fn lower_lazy_isset_operand(
    ctx: &mut LoweringContext<'_, '_>,
    arg: &Expr,
) -> Option<LoweredValue> {
    match &arg.kind {
        ExprKind::ArrayAccess { array, index } => {
            if array_access_expr_satisfies_array_access(ctx, array) {
                let synthetic = Expr::new(
                    ExprKind::MethodCall {
                        object: array.clone(),
                        method: "offsetExists".to_string(),
                        args: vec![(**index).clone()],
                    },
                    arg.span,
                );
                return Some(lower_expr(ctx, &synthetic));
            }
            if !array_access_expr_supports_native_isset_probe(ctx, array) {
                return None;
            }
            Some(lower_native_isset_offset_probe(ctx, array, index, arg))
        }
        ExprKind::PropertyAccess { object, property }
        | ExprKind::NullsafePropertyAccess { object, property } => {
            if let Some(value) = lower_lazy_property_isset_operand(ctx, object, property, arg) {
                return Some(value);
            }
            // A receiver whose class is unknown until run time has no `property_isset_action`
            // answer, so the eager fallback would take the RAISING value-read arm of the Mixed
            // declared-slot ladder. php answers `false` for a property this scope may not reach.
            if !property_probe_needs_runtime_name_form(ctx, object) {
                return None;
            }
            let object = lower_expr(ctx, object);
            let probed = lower_property_probe_from_value(ctx, object, property, arg);
            Some(probe_value_is_set(ctx, probed, arg))
        }
        ExprKind::DynamicPropertyAccess { object, property } => {
            // `isset($o->{$k})` must not raise for a name php refuses, and must not warn for one
            // it has never created, so the read carries php's probe fetch mode.
            let probed =
                lower_dynamic_property_fetch(ctx, object, property, PropertyFetchMode::Probe, arg);
            Some(probe_value_is_set(ctx, probed, arg))
        }
        // `$o?->{$k}` is a nullsafe CHAIN, so its short-circuit belongs to the chain lowering.
        // Passing the silent-probe flag is what carries the fetch mode down to the segment.
        ExprKind::NullsafeDynamicPropertyAccess { .. } => {
            let probed = nullsafe_chain::lower_with_missing_warning(ctx, arg, false)?;
            Some(probe_value_is_set(ctx, probed, arg))
        }
        // A typed static property starts uninitialized and `isset()` must answer false there
        // rather than take the ordinary read, whose backend guard is fatal.
        ExprKind::StaticPropertyAccess { receiver, property }
            if static_property_can_be_uninitialized(ctx, receiver, property) =>
        {
            Some(lower_initialized_static_property_isset(
                ctx, receiver, property, arg,
            ))
        }
        // `isset($this)` inside a static closure always evaluates to `false`
        // because static closures have no `$this` binding. PHP allows this
        // probe and returns false; elephc must not try to load a missing slot.
        ExprKind::This if !ctx.local_slots.contains_key("this") => {
            Some(lower_bool_literal(ctx, false, arg))
        }
        _ => None,
    }
}

/// Reduces a probed property value to php's `isset()` answer: set means "not null".
fn probe_value_is_set(
    ctx: &mut LoweringContext<'_, '_>,
    probed: LoweredValue,
    arg: &Expr,
) -> LoweredValue {
    emit_builtin_call_value(ctx, "isset", vec![probed.value], PhpType::Int, arg.span, None)
}

/// Lowers `empty($obj->magicProp)` with PHP's overloaded-property semantics:
/// `empty` consults `__isset` first and only evaluates `__get` when `__isset`
/// is truthy, so an unset virtual property is empty without ever reading it.
/// Returns `None` for operands the eager `empty` builtin already handles (plain
/// variables, declared properties, array elements), letting that path run.
pub(super) fn lower_lazy_empty(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "empty" {
        return None;
    }
    if args.len() != 1
        || crate::types::call_args::has_named_args(args)
        || args.iter().any(is_spread_arg)
    {
        return None;
    }
    if let ExprKind::ArrayAccess { array, index } = &args[0].kind {
        if array_access_expr_satisfies_array_access(ctx, array) {
            let object = evaluate_once(ctx, array, &args[0]);
            return Some(lower_array_access_object_empty(
                ctx, &object, index, name, expr, &args[0],
            ));
        }
        // A call or `new` result has no syntactic type, so it is lowered once and judged by
        // the type it lowers to; that value is then the receiver on either path.
        if receiver_is_computed_value(array) {
            let object = super::pipe::lower_pipe_value_temp(ctx, array, &args[0]);
            if array_access_expr_satisfies_array_access(ctx, &object) {
                return Some(lower_array_access_object_empty(
                    ctx, &object, index, name, expr, &args[0],
                ));
            }
            let value =
                lower_array_access_with_missing_warning(ctx, &object, index, &args[0], false);
            return Some(emit_builtin_call_value(
                ctx,
                name,
                vec![value.value],
                PhpType::Bool,
                expr.span,
                None,
            ));
        }
        let value = lower_array_access_with_missing_warning(ctx, array, index, &args[0], false);
        return Some(emit_builtin_call_value(
            ctx,
            name,
            vec![value.value],
            PhpType::Bool,
            expr.span,
            None,
        ));
    }
    // A typed static property that is still uninitialized is EMPTY in PHP, and reading it the
    // ordinary way to find that out is fatal — the same reason `isset()` and `??` need the
    // slot probe. Uninitialized answers true without the read.
    if let ExprKind::StaticPropertyAccess { receiver, property } = &args[0].kind {
        if static_property_can_be_uninitialized(ctx, receiver, property) {
            return Some(lower_initialized_static_property_empty(
                ctx, receiver, property, name, &args[0],
            ));
        }
    }
    // The instance twin of the static arm above: an uninitialized typed slot is EMPTY, and the
    // ordinary read that would say so raises. `property_isset_action` already decides whether the
    // slot is a declared one worth probing — the same decision `isset()` makes, reused rather than
    // re-derived so the two constructs cannot drift on which properties they consider declared.
    if let ExprKind::PropertyAccess { object, property } = &args[0].kind {
        if matches!(
            property_isset_action(ctx, object, property),
            Some(IssetPropertyAction::Initialized)
        ) {
            let object = lower_expr(ctx, object);
            return Some(lower_initialized_property_empty(
                ctx, object, property, name, &args[0],
            ));
        }
        // Same reason as `isset()`: php's `empty()` is a silent probe, so an inaccessible or
        // never-created name answers `true` instead of raising.
        if property_probe_needs_runtime_name_form(ctx, object) {
            let object = lower_expr(ctx, object);
            let probed = lower_property_probe_from_value(ctx, object, property, &args[0]);
            return Some(lower_empty_of_probed_value(ctx, probed, name, expr));
        }
    }
    if let ExprKind::DynamicPropertyAccess { object, property } = &args[0].kind {
        let probed =
            lower_dynamic_property_fetch(ctx, object, property, PropertyFetchMode::Probe, &args[0]);
        return Some(lower_empty_of_probed_value(ctx, probed, name, expr));
    }
    // A `?->` operand is a nullsafe CHAIN. Its segments only learn they are in a silent probe
    // from the flag below, so `empty($o?->{$k})` raised for a name php refuses. The magic route
    // keeps precedence: php consults `__isset` before it evaluates anything.
    if lazy_empty_magic_property_calls(ctx, &args[0]).is_none() {
        if let Some(probed) = nullsafe_chain::lower_with_missing_warning(ctx, &args[0], false) {
            return Some(lower_empty_of_probed_value(ctx, probed, name, expr));
        }
    }
    let (exists_call, get_call) = lazy_empty_magic_property_calls(ctx, &args[0])?;
    Some(lower_empty_through_exists_then_get(
        ctx,
        &exists_call,
        &get_call,
        name,
        expr,
    ))
}

/// Lowers `empty($object[$index])` on an `ArrayAccess` receiver the way PHP does:
/// `offsetExists` decides first, and `offsetGet` is read only when it said yes. `object` is
/// already evaluated; the offset is evaluated here, once, before either call names it.
fn lower_array_access_object_empty(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    index: &Expr,
    name: &str,
    expr: &Expr,
    arg: &Expr,
) -> LoweredValue {
    let key = evaluate_once(ctx, index, arg);
    let exists_call = synthetic_method_call(object, "offsetExists", &key, arg.span);
    let get_call = synthetic_method_call(object, "offsetGet", &key, arg.span);
    lower_empty_through_exists_then_get(ctx, &exists_call, &get_call, name, expr)
}

/// Returns an expression that can be named twice with the effects of evaluating `expr` once.
/// A variable or a literal already is one; anything else is lowered into a hidden temp.
fn evaluate_once(ctx: &mut LoweringContext<'_, '_>, expr: &Expr, arg: &Expr) -> Expr {
    match expr.kind {
        ExprKind::Variable(_)
        | ExprKind::This
        | ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null => expr.clone(),
        _ => super::pipe::lower_pipe_value_temp(ctx, expr, arg),
    }
}

/// Returns true for a receiver that is a call or `new` result: evaluating it has effects, and
/// its type is only known once it is lowered.
fn receiver_is_computed_value(receiver: &Expr) -> bool {
    matches!(
        receiver.kind,
        ExprKind::FunctionCall { .. }
            | ExprKind::ClosureCall { .. }
            | ExprKind::ExprCall { .. }
            | ExprKind::MethodCall { .. }
            | ExprKind::StaticMethodCall { .. }
            | ExprKind::NewObject { .. }
            | ExprKind::NewDynamic { .. }
            | ExprKind::NewDynamicObject { .. }
            | ExprKind::NewScopedObject { .. }
    )
}

/// Builds `$object->method($key)` for a receiver and key that are already side-effect free.
fn synthetic_method_call(object: &Expr, method: &str, key: &Expr, span: Span) -> Expr {
    Expr::new(
        ExprKind::MethodCall {
            object: Box::new(object.clone()),
            method: method.to_string(),
            args: vec![key.clone()],
        },
        span,
    )
}

/// Lowers PHP's two-step `empty()` over an overloaded read: `exists_call` decides whether the
/// value is set at all, and only a truthy answer evaluates `get_call` to test its emptiness.
/// An unset value is empty without ever being read.
fn lower_empty_through_exists_then_get(
    ctx: &mut LoweringContext<'_, '_>,
    exists_call: &Expr,
    get_call: &Expr,
    name: &str,
    expr: &Expr,
) -> LoweredValue {
    let temp_name = ctx.declare_hidden_temp(PhpType::Bool);
    let present_block = ctx.builder.create_named_block("empty.present", Vec::new());
    let absent_block = ctx.builder.create_named_block("empty.absent", Vec::new());
    let merge = ctx.builder.create_named_block("empty.merge", Vec::new());

    // `__isset(prop)` decides whether the property is considered set at all.
    let exists = lower_expr(ctx, exists_call);
    ctx.builder.terminate(Terminator::CondBr {
        cond: exists.value,
        then_target: present_block,
        then_args: Vec::new(),
        else_target: absent_block,
        else_args: Vec::new(),
    });

    // Set: empty is the emptiness of the `__get` value (reuses the eager builtin).
    ctx.builder.position_at_end(present_block);
    // The read value is a fresh call result nothing else owns, so the builtin path, which
    // releases owned operands once the test has consumed them, is the one to take.
    let get_value = lower_expr(ctx, get_call);
    let empty_value =
        emit_builtin_call_value(ctx, name, vec![get_value.value], PhpType::Bool, expr.span, None);
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, empty_value, expr.span);
    branch_to(ctx, merge);

    // Not set: empty is true and `__get` is never called.
    ctx.builder.position_at_end(absent_block);
    let true_value = lower_bool_literal(ctx, true, expr);
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, true_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    ctx.load_local(&temp_name, Some(expr.span))
}

/// Applies php's `empty()` emptiness test to an already probed property value.
fn lower_empty_of_probed_value(
    ctx: &mut LoweringContext<'_, '_>,
    probed: LoweredValue,
    name: &str,
    expr: &Expr,
) -> LoweredValue {
    let empty_name = ctx.intern_function_name(name);
    ctx.emit_value(
        Op::LanguageConstructCall,
        vec![probed.value],
        Some(Immediate::Data(empty_name)),
        PhpType::Bool,
        effects_lookup::language_construct_effects(name),
        Some(expr.span),
    )
}

/// For an `empty()` operand that is an overloaded (magic) property access,
/// returns the `(__isset, __get)` synthetic call expressions PHP would evaluate.
/// The property name is a string literal, so reusing it for both calls is
/// side-effect free. Returns `None` for any other operand shape.
pub(super) fn lazy_empty_magic_property_calls(
    ctx: &LoweringContext<'_, '_>,
    arg: &Expr,
) -> Option<(Expr, Expr)> {
    match &arg.kind {
        ExprKind::PropertyAccess { object, property } => {
            property_existence_magic_class(ctx, object, property, "__isset")?;
            let key = Expr::new(ExprKind::StringLiteral(property.clone()), arg.span);
            let exists = Expr::new(
                ExprKind::MethodCall {
                    object: object.clone(),
                    method: "__isset".to_string(),
                    args: vec![key.clone()],
                },
                arg.span,
            );
            let get = Expr::new(
                ExprKind::MethodCall {
                    object: object.clone(),
                    method: "__get".to_string(),
                    args: vec![key],
                },
                arg.span,
            );
            Some((exists, get))
        }
        ExprKind::NullsafePropertyAccess { object, property } => {
            property_existence_magic_class(ctx, object, property, "__isset")?;
            let key = Expr::new(ExprKind::StringLiteral(property.clone()), arg.span);
            let exists = Expr::new(
                ExprKind::NullsafeMethodCall {
                    object: object.clone(),
                    method: "__isset".to_string(),
                    args: vec![key.clone()],
                },
                arg.span,
            );
            let get = Expr::new(
                ExprKind::NullsafeMethodCall {
                    object: object.clone(),
                    method: "__get".to_string(),
                    args: vec![key],
                },
                arg.span,
            );
            Some((exists, get))
        }
        _ => None,
    }
}

/// Returns the class whose `magic` method (`__isset`/`__unset`) should handle
/// property existence/removal: a property that cannot be accessed normally on an
/// object whose class declares the magic method.
pub(super) fn property_existence_magic_class(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    magic: &str,
) -> Option<String> {
    let class_name = instance_callable_object_class(ctx, object)?;
    let class_info = ctx.classes.get(&class_name)?;
    if property_is_accessible_for_ir(ctx, &class_name, class_info, property) {
        return None;
    }
    class_method_signature(ctx, &class_name, &php_symbol_key(magic)).map(|_| class_name)
}

