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
/// Reports whether an operand is a pure property/dim WALK, with no call anywhere inside it.
///
/// PHP fixes the quiet fetch at COMPILE time: the mode propagates down property and dim fetch
/// nodes and is never applied to a call, so `f($o) ?? 'D'` and `$o->getT() ?? 'D'` raise where
/// `$o->t ?? 'D'` answers. Deciding it on the shape of the operand is therefore not an
/// approximation of PHP's rule, it IS the rule -- and it means the compiled side needs no
/// barrier at call sites, because an operand containing a call is simply never wrapped.
///
/// Deliberately conservative: an index expression is walked too, so `isset($o->a[f()])` is left
/// alone rather than run with the mode held across `f()`. Refusing to go quiet is always safe;
/// going quiet where PHP would not is what silently swallows a real error.
pub(super) fn expr_is_quiet_fetch_chain(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(_) | ExprKind::This => true,
        ExprKind::PropertyAccess { object, .. } => expr_is_quiet_fetch_chain(object),
        // `$o->$name` / `$o->{'n'}`: the name is walked like an index.
        ExprKind::DynamicPropertyAccess { object, property } => {
            expr_is_quiet_fetch_chain(object) && expr_is_quiet_fetch_chain(property)
        }
        ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::ArrayAccess { array, index } => {
            expr_is_quiet_fetch_chain(array) && expr_is_quiet_fetch_chain(index)
        }
        ExprKind::IntLiteral(_) | ExprKind::StringLiteral(_) | ExprKind::BoolLiteral(_) => true,
        _ => false,
    }
}

/// Reports whether a quiet-fetch chain reads an instance or static property anywhere in it.
fn expr_chain_fetches_a_property(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::PropertyAccess { .. }
        | ExprKind::DynamicPropertyAccess { .. }
        | ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::ArrayAccess { array, index } => {
            expr_chain_fetches_a_property(array) || expr_chain_fetches_a_property(index)
        }
        _ => false,
    }
}

/// Emits one side of the compiled quiet-fetch scope.
pub(in crate::ir_lower) fn emit_quiet_property_fetch(ctx: &mut LoweringContext<'_, '_>, enter: bool, span: Span) {
    ctx.emit_void(
        Op::RuntimeCall,
        Vec::new(),
        Some(Immediate::RuntimeCall(
            crate::ir::RuntimeCallTarget::EvalQuietPropertyFetch { enter },
        )),
        effects_lookup::runtime_effects(),
        Some(span),
    );
}

/// Lowers one `isset`/`empty`/`??` operand inside PHP's quiet property fetch.
///
/// Compiled code reaching an EVAL-OWNED object's property goes through the bridge, which raises
/// for an uninitialized typed property unless the mode is on; the interpreter sets the mode
/// around its own operands and this is the same door for the compiled side, sharing one
/// thread-local depth so a chain that crosses between them sees one mode. Without it,
/// `empty($this->p[$k])` compiled over an eval-declared class raised where the identical source
/// interpreted answered -- the Symfony stop, `CheckCircularReferencesPass::$checkedLazyNodes`.
///
/// Only a pure property/dim walk is wrapped, which is what keeps "the mode does not cross a
/// call" true without a barrier at every compiled call site.
pub(super) fn lower_operand_in_quiet_property_fetch<F>(
    ctx: &mut LoweringContext<'_, '_>,
    operand: &Expr,
    lower_operand: F,
) -> LoweredValue
where
    F: FnOnce(&mut LoweringContext<'_, '_>) -> LoweredValue,
{
    // The mode only changes what a PROPERTY fetch does; a chain of variables and dims
    // (`isset($x)`, `$a['k'] ?? null`) reads nothing the bridge could refuse, and wrapping it
    // cost two bridge crossings per evaluation -- about 2,000 per Symfony request.
    if !expr_is_quiet_fetch_chain(operand) || !expr_chain_fetches_a_property(operand) {
        return lower_operand(ctx);
    }
    emit_quiet_property_fetch(ctx, true, operand.span);
    let value = lower_operand(ctx);
    emit_quiet_property_fetch(ctx, false, operand.span);
    value
}

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
        let checked = lower_operand_in_quiet_property_fetch(ctx, arg, |ctx| {
        lower_lazy_isset_operand(ctx, arg).unwrap_or_else(|| {
            // `isset()` never emits undefined-offset warnings, so eager array
            // operands must be lowered with the silent read variants.
            let value = if let ExprKind::ArrayAccess { array, index } = &arg.kind {
                lower_array_access_with_missing_warning(ctx, array, index, arg, false)
            } else {
                lower_expr(ctx, arg)
            };
            emit_builtin_call_value(ctx, name, vec![value.value], PhpType::Int, arg.span, None)
        })
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
        // No syntactic gate on the receiver. A pre-lowering guess types every user call `int`
        // (`infer_expr_type_syntactic`), so `isset(f()['k'])` used to be refused here and fall
        // through to the read-then-test path in `lower_lazy_isset`. That path RELEASES the
        // call's temporary array as soon as the element is read, and the backend's `isset`
        // lowering then re-probes the read's producer (`source_instruction`), calling
        // `__rt_hash_get` on storage already freed — a present key answered `false`. Symfony's
        // `isset(static::getProvidedTypes()[$prefix])` is exactly that shape, and it took down
        // every env-var placeholder in the prod container.
        //
        // `lower_native_isset_offset_probe` dispatches on the receiver's REAL lowered IR type
        // instead of on the shape of its expression, and its final arm is the same
        // read-then-test for receivers no native probe fits — with the receiver evaluated once
        // and kept, so nothing re-reads it after the release.
        ExprKind::ArrayAccess { array, index } => {
            if array_access_expr_satisfies_array_access(ctx, array) {
                return Some(lower_array_access_offset_exists(ctx, array, index, arg));
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
            let probed = lower_property_isset_probe_from_value(ctx, object, property, arg);
            Some(probe_value_is_set(ctx, probed, arg))
        }
        ExprKind::DynamicPropertyAccess { object, property } => {
            // `isset($o->{$k})` must not raise for a name php refuses, and must not warn for one
            // it has never created, so the read carries php's probe fetch mode.
            let probed =
                lower_dynamic_property_fetch(ctx, object, property, PropertyFetchMode::Isset, arg);
            Some(probe_value_is_set(ctx, probed, arg))
        }
        // `$o?->{$k}` is a nullsafe CHAIN, so its short-circuit belongs to the chain lowering.
        // Passing the silent-probe flag is what carries the fetch mode down to the segment.
        ExprKind::NullsafeDynamicPropertyAccess { .. } => {
            let probed = nullsafe_chain::lower_with_missing_warning(ctx, arg, false)?;
            Some(probe_value_is_set(ctx, probed, arg))
        }
        // No class `static::` can be bound to declares the name: php's constant `false`.
        ExprKind::StaticPropertyAccess { receiver, property }
            if late_bound_static_property_is_absent(ctx, receiver, property) =>
        {
            Some(lower_bool_literal(ctx, false, arg))
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

/// Lowers `isset($arrayAccess[$key])`, returning false without a call for a null receiver.
fn lower_array_access_offset_exists(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
    index: &Expr,
    arg: &Expr,
) -> LoweredValue {
    // `isset()` reads the container quietly too: an uninitialized typed property is not set.
    let receiver = lower_subscript_receiver_silently(ctx, array);
    let args = vec![index.clone()];
    if !value_is_nullable(ctx, receiver.value) {
        return lower_method_call_with_receiver(
            ctx,
            receiver,
            "offsetExists",
            &args,
            Op::MethodCall,
            arg,
        );
    }

    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![receiver.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(arg.span),
    );
    let temp_name = ctx.declare_hidden_temp(PhpType::Bool);
    let null_block = ctx
        .builder
        .create_named_block("isset.array_access.null", Vec::new());
    let call_block = ctx
        .builder
        .create_named_block("isset.array_access.call", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("isset.array_access.merge", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: null_block,
        then_args: Vec::new(),
        else_target: call_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(null_block);
    let false_value = emit_bool_literal(ctx, false, Some(arg.span));
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, false_value, arg.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(call_block);
    let exists = lower_method_call_with_receiver(
        ctx,
        receiver,
        "offsetExists",
        &args,
        Op::MethodCall,
        arg,
    );
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, exists, arg.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp_name, arg.span)
}

/// Whether a property chain's receiver reads through an array element (`$list[$k]->a->b`).
pub(super) fn object_chain_reads_an_element(object: &Expr) -> bool {
    match &object.kind {
        ExprKind::ArrayAccess { .. } => true,
        ExprKind::PropertyAccess { object, .. } => object_chain_reads_an_element(object),
        _ => false,
    }
}

/// Reduces a probed property value to php's `isset()` answer: set means "not null".
pub(super) fn probe_value_is_set(
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
        // The receiver is read ONCE and judged by the type it lowers to. `$this`, a property
        // and a call or `new` result only get that type by being lowered (#1448), and even a
        // variable is not named again: `offsetExists` may reassign it, and `offsetGet` must
        // still ask the object that answered (#1449). An `ArrayAccess` object is kept in a
        // hidden temp for the two calls and released as soon as `empty()` has decided, so it
        // lives no longer than PHP holds the container: a later `unset()` or reassignment
        // still runs the destructor at that point. Any other value continues as the plain read.
        if array_access_expr_satisfies_array_access(ctx, array)
            || receiver_is_computed_value(array)
            || receiver_is_typed_once_lowered(array)
        {
            // The receiver read is part of the quiet fetch: `empty($o->lazy[$k])` over an
            // eval-owned `$o` reads `lazy` through the bridge, which raises for an uninitialized
            // typed property unless the mode is on. Only the receiver read is wrapped; the
            // `offsetExists`/`offsetGet` calls below run outside it, as php runs them.
            let receiver = lower_operand_in_quiet_property_fetch(ctx, &args[0], |ctx| {
                lower_subscript_receiver_silently(ctx, array)
            });
            let receiver_type = ctx.builder.value_php_type(receiver.value);
            if type_satisfies_array_access_for_ir(ctx, &receiver_type) {
                let nullable = value_is_nullable(ctx, receiver.value);
                let temp_name = ctx.declare_hidden_temp(receiver_type.clone());
                store_value_into_temp(ctx, &temp_name, receiver_type, receiver, args[0].span);
                let object = Expr::new(ExprKind::Variable(temp_name.clone()), args[0].span);
                let result = lower_array_access_object_empty(
                    ctx, &object, nullable, index, name, expr, &args[0],
                );
                release_evaluated_once(ctx, Some(temp_name), args[0].span);
                return Some(result);
            }
            // Keep a computed native receiver in the same hidden-local form used by the
            // ArrayAccess route, through both the silent read and the empty test.
            // Retire the local immediately after the decision instead of at function exit.
            let temp_name = ctx.declare_hidden_temp(receiver_type.clone());
            store_value_into_temp(ctx, &temp_name, receiver_type, receiver, args[0].span);
            let staged = Expr::new(ExprKind::Variable(temp_name.clone()), args[0].span);
            let value = lower_array_access_with_missing_warning(
                ctx, &staged, index, &args[0], false,
            );
            let result = lower_empty_of_probed_value(ctx, value, name, expr);
            release_evaluated_once(ctx, Some(temp_name), args[0].span);
            return Some(result);
        }
        // `empty($this->p[$k])` is the Symfony stop's exact shape, and its property read is one
        // level BELOW the operand root, so the mode has to wrap the whole walk rather than the
        // outermost node.
        let value = lower_operand_in_quiet_property_fetch(ctx, &args[0], |ctx| {
            lower_array_access_with_missing_warning(ctx, array, index, &args[0], false)
        });
        return Some(lower_empty_of_probed_value(ctx, value, name, expr));
    }
    // A typed static property that is still uninitialized is EMPTY in PHP, and reading it the
    // ordinary way to find that out is fatal — the same reason `isset()` and `??` need the
    // slot probe. Uninitialized answers true without the read.
    if let ExprKind::StaticPropertyAccess { receiver, property } = &args[0].kind {
        // A name no bindable class declares is php's EMPTY, without the read that raises.
        if late_bound_static_property_is_absent(ctx, receiver, property) {
            return Some(lower_bool_literal(ctx, true, &args[0]));
        }
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
        // The element may not exist: `empty($mappings[$name]->type)` is quiet at every link, and
        // the probe answers for a null receiver where a slot read would dereference it.
        if object_chain_reads_an_element(object) {
            let probed = lower_operand_in_quiet_property_fetch(ctx, &args[0], |ctx| {
                lower_quiet_element_property(ctx, object, property, &args[0])
            });
            return Some(lower_empty_of_probed_value(ctx, probed, name, expr));
        }
        // A property this scope cannot reach on a class without `__isset`: `isset()` answers
        // false, so `empty()` answers true -- without the read, which would touch a slot the
        // scope has no right to (`empty($o->private)` from outside crashed once the checker
        // stopped refusing it).
        if matches!(
            property_isset_action(ctx, object, property, &args[0]),
            Some(IssetPropertyAction::AlwaysFalse)
        ) {
            lower_member_receiver(ctx, object, &args[0]);
            return Some(emit_bool_literal(ctx, true, Some(args[0].span)));
        }
        if matches!(
            property_isset_action(ctx, object, property, &args[0]),
            Some(IssetPropertyAction::Initialized)
        ) {
            // `empty()` is a quiet fetch in php: an uninitialized typed property is empty, never
            // an error. A receiver owned by an eval context is read through the bridge, which
            // only stays quiet inside the mode.
            return Some(lower_operand_in_quiet_property_fetch(ctx, &args[0], |ctx| {
                let object = lower_member_receiver(ctx, object, &args[0]);
                lower_initialized_property_empty(ctx, object, property, name, &args[0])
            }));
        }
        // Same reason as `isset()`: php's `empty()` is a silent probe, so an inaccessible or
        // never-created name answers `true` instead of raising. Unlike `isset()` it needs the
        // VALUE: php asks `__isset` and then `__get`, which is the `??` fetch (`Coalesce`).
        if property_probe_needs_runtime_name_form(ctx, object) {
            let object = lower_expr(ctx, object);
            let probed = lower_property_empty_probe_from_value(ctx, object, property, &args[0]);
            return Some(lower_empty_of_probed_value(ctx, probed, name, expr));
        }
        // A class with `__get` but no `__isset`: php finds no property and never calls
        // `__get`, so the probe below answers from the dynamic hash or null.
        if property_existence_magic_class(ctx, object, property, "__get").is_some()
            && property_existence_magic_class(ctx, object, property, "__isset").is_none()
        {
            let object = lower_expr(ctx, object);
            let probed = lower_property_empty_probe_from_value(ctx, object, property, &args[0]);
            return Some(lower_empty_of_probed_value(ctx, probed, name, expr));
        }
    }
    if let ExprKind::DynamicPropertyAccess { object, property } = &args[0].kind {
        let probed = lower_operand_in_quiet_property_fetch(ctx, &args[0], |ctx| {
            lower_dynamic_property_fetch(ctx, object, property, PropertyFetchMode::Empty, &args[0])
        });
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
    let ordinary_value = match &args[0].kind {
        ExprKind::Variable(name) => ctx.has_local_slot(name),
        ExprKind::PropertyAccess { .. } | ExprKind::DynamicPropertyAccess { .. }
        | ExprKind::StaticPropertyAccess { .. } | ExprKind::NullsafePropertyAccess { .. }
        | ExprKind::NullsafeDynamicPropertyAccess { .. } => false,
        _ => true,
    };
    let ty = materialized_expr_type_for_merge(ctx, &args[0]);
    let may_have_handler = crate::types::object_operators::operator_class(&ty).is_some()
        || (matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_) | PhpType::Object(_))
            && ctx.program_has_operator_classes());
    if ordinary_value && may_have_handler {
        let value = lower_expr(ctx, &args[0]);
        return Some(lower_empty_of_probed_value(ctx, value, name, expr));
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
/// already evaluated into a hidden temp, so both calls reach the same object; the offset is
/// evaluated here, once, before either call names it.
///
/// A `nullable` receiver that holds null is empty without either call, as a null container
/// is in PHP; its offset is still evaluated first, as PHP evaluates it. A computed offset is
/// released once the decision is made, on every path.
fn lower_array_access_object_empty(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    nullable: bool,
    index: &Expr,
    name: &str,
    expr: &Expr,
    arg: &Expr,
) -> LoweredValue {
    let (key, key_temp) = evaluate_once(ctx, index, arg);
    let exists_call = synthetic_method_call(object, "offsetExists", &key, arg.span);
    let get_call = synthetic_method_call(object, "offsetGet", &key, arg.span);
    let result = if nullable {
        lower_nullable_receiver_empty(ctx, object, &exists_call, &get_call, name, expr, arg)
    } else {
        lower_empty_through_exists_then_get(ctx, &exists_call, &get_call, name, expr)
    };
    release_evaluated_once(ctx, key_temp, arg.span);
    result
}

/// Lowers the gated `empty()` for a receiver that may hold null: null is empty without either
/// `ArrayAccess` call, an object takes the `offsetExists` then `offsetGet` route.
fn lower_nullable_receiver_empty(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    exists_call: &Expr,
    get_call: &Expr,
    name: &str,
    expr: &Expr,
    arg: &Expr,
) -> LoweredValue {
    let temp_name = ctx.declare_hidden_temp(PhpType::Bool);
    let null_block = ctx.builder.create_named_block("empty.null_receiver", Vec::new());
    let object_block = ctx.builder.create_named_block("empty.object_receiver", Vec::new());
    let merge = ctx.builder.create_named_block("empty.receiver_merge", Vec::new());
    let receiver = lower_expr(ctx, object);
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![receiver.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(arg.span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: null_block,
        then_args: Vec::new(),
        else_target: object_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(null_block);
    let true_value = lower_bool_literal(ctx, true, expr);
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, true_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(object_block);
    let empty_value = lower_empty_through_exists_then_get(ctx, exists_call, get_call, name, expr);
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, empty_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    ctx.load_local(&temp_name, Some(expr.span))
}

/// Returns an expression that can be named twice with the effects of evaluating `expr` once,
/// plus the hidden temp holding the value when one was needed.
///
/// A variable or a literal already is one; anything else is lowered once into a hidden temp,
/// which the caller hands to `release_evaluated_once` as soon as both reads are done. A hidden
/// temp, not a one-shot owned temp: its loads borrow from the slot, so the `offsetExists` and
/// `offsetGet` calls can both name it without consuming its reference.
pub(super) fn evaluate_once(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
    arg: &Expr,
) -> (Expr, Option<String>) {
    match expr.kind {
        ExprKind::Variable(_)
        | ExprKind::This
        | ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null => (expr.clone(), None),
        _ => {
            let value = lower_expr(ctx, expr);
            let temp_type = ctx.builder.value_php_type(value.value);
            let temp_name = ctx.declare_hidden_temp(temp_type.clone());
            store_value_into_temp(ctx, &temp_name, temp_type, value, arg.span);
            (Expr::new(ExprKind::Variable(temp_name.clone()), arg.span), Some(temp_name))
        }
    }
}

/// Releases the hidden temp that held an `empty()` receiver or offset for its two
/// `ArrayAccess` calls, once neither call needs it.
///
/// The value's lifetime then ends with the `empty()` expression, as in PHP, instead of at
/// function exit, where the frame cleanup would otherwise release the slot: until then the
/// extra reference kept an object alive past its last `unset()` and delayed `__destruct()`.
/// `ReleaseLocalSlot` clears the slot before releasing its occupant, so the epilogue finds it
/// empty and a throwing destructor cannot revisit it.
pub(super) fn release_evaluated_once(ctx: &mut LoweringContext<'_, '_>, temp: Option<String>, span: Span) {
    let Some(temp_name) = temp else {
        return;
    };
    let slot = ctx.declare_local(&temp_name, ctx.local_type(&temp_name));
    ctx.emit_void(
        Op::ReleaseLocalSlot,
        Vec::new(),
        Some(Immediate::LocalSlot(slot)),
        Op::ReleaseLocalSlot.default_effects(),
        Some(span),
    );
}

/// Returns true for `$this` and property receivers, whose type is known only once they are
/// lowered: nothing types them syntactically.
fn receiver_is_typed_once_lowered(receiver: &Expr) -> bool {
    matches!(
        receiver.kind,
        ExprKind::This | ExprKind::PropertyAccess { .. } | ExprKind::StaticPropertyAccess { .. }
    )
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
    // The probe is declared `bool`, but a call through a property receiver can come back as a
    // boxed `Mixed` (`empty($this->inner['k'])` dispatches `offsetExists()` that way), and a
    // branch condition must be a canonical bool.
    let exists = lower_expr(ctx, exists_call);
    let exists = super::unary_logic::lower_truthy_bool(ctx, exists, Some(expr.span));
    ctx.builder.terminate(Terminator::CondBr {
        cond: exists.value,
        then_target: present_block,
        then_args: Vec::new(),
        else_target: absent_block,
        else_args: Vec::new(),
    });

    // Set: apply the fetched value's own boolean contract.
    ctx.builder.position_at_end(present_block);
    // The read value is a fresh call result nothing else owns, so the builtin path, which
    // releases owned operands once the test has consumed them, is the one to take.
    let get_value = lower_expr(ctx, get_call);
    let empty_value = lower_empty_of_probed_value(ctx, get_value, name, expr);
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
pub(super) fn lower_empty_of_probed_value(
    ctx: &mut LoweringContext<'_, '_>,
    probed: LoweredValue,
    name: &str,
    expr: &Expr,
) -> LoweredValue {
    if ctx.builder.value_php_type(probed.value).codegen_repr() == PhpType::Str {
        // A string offset can be the byte "0", which PHP considers empty despite its length.
        let truthy = ctx.truthy_consuming(probed, Some(expr.span));
        let zero = lower_int_literal(ctx, 0, expr);
        return ctx.emit_value(Op::ICmp, vec![truthy.value, zero.value],
            Some(Immediate::CmpPredicate(CmpPredicate::Eq)), PhpType::Bool,
            Op::ICmp.default_effects(), Some(expr.span));
    }
    if let Some(truthy) = object_operators::lower_object_truthy(ctx, probed, Some(expr.span)) {
        let zero = lower_int_literal(ctx, 0, expr);
        let result = ctx.emit_value(Op::ICmp, vec![truthy.value, zero.value],
            Some(Immediate::CmpPredicate(CmpPredicate::Eq)), PhpType::Bool,
            Op::ICmp.default_effects(), Some(expr.span));
        release_binary_operand_temporary(ctx, probed, expr.span);
        return result;
    }
    emit_builtin_call_value(ctx, name, vec![probed.value], PhpType::Bool, expr.span, None)
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
