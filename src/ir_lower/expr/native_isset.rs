//! Purpose:
//! Native array and property isset probes.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers native array/hash `isset($array[$key])` without reading the element value.
pub(super) fn lower_native_isset_offset_probe(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
    index: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let array_value = lower_subscript_receiver_silently(ctx, array);
    if value_is_nullable(ctx, array_value.value) {
        return lower_nullable_native_isset_offset_probe(ctx, array_value, index, expr);
    }
    lower_native_isset_offset_probe_from_value(ctx, array_value, index, expr)
}

/// Lowers nullable native array/hash `isset`, evaluating the offset only for its effects on a
/// null receiver.
pub(super) fn lower_nullable_native_isset_offset_probe(
    ctx: &mut LoweringContext<'_, '_>,
    array_value: LoweredValue,
    index: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![array_value.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(expr.span),
    );
    let temp_name = ctx.declare_hidden_temp(PhpType::Bool);
    let null_block = ctx
        .builder
        .create_named_block("isset.native.null", Vec::new());
    let probe_block = ctx
        .builder
        .create_named_block("isset.native.probe", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("isset.native.merge", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: null_block,
        then_args: Vec::new(),
        else_target: probe_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(null_block);
    super::array_access::lower_skipped_index_for_effects(ctx, index);
    // A concrete container projected from a widened Mixed local owns the lease created by that
    // projection. The probe arm retires it after `ArrayIsset` / `HashIsset`; the null arm skips
    // the probe, so it must retire the same receiver here instead of leaking one lease.
    release_owning_receiver_temporary(ctx, array_value, expr.span);
    let false_value = emit_bool_literal(ctx, false, Some(expr.span));
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, false_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(probe_block);
    let checked = lower_native_isset_offset_probe_from_value(ctx, array_value, index, expr);
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, checked, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp_name, expr.span)
}

/// Lowers native array/hash `isset` once the receiver has already been evaluated.
///
/// The probe only reads its receiver, so a receiver it owns is released afterwards, as an array
/// read does. A boxed (`mixed`) local is loaded with its own reference: kept, that reference left
/// the container shared, and the next `$cache[$k] = $v` after `isset($cache[$k])` copied the whole
/// table and leaked the copy it replaced.
pub(super) fn lower_native_isset_offset_probe_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    array_value: LoweredValue,
    index: &Expr,
    expr: &Expr,
) -> LoweredValue {
    if !matches!(
        array_value.ir_type,
        IrType::Heap(IrHeapKind::Array) | IrType::Heap(IrHeapKind::Hash)
    ) {
        return lower_non_native_isset_offset_probe(ctx, array_value, index, expr);
    }
    let checked = lower_native_container_isset_offset_probe(ctx, array_value, index, expr);
    if ctx.value_is_owning_temporary(array_value) {
        crate::ir_lower::ownership::release_if_owned(ctx, array_value, Some(expr.span));
    }
    checked
}

/// Probes an indexed array or hash receiver without releasing it.
fn lower_native_container_isset_offset_probe(
    ctx: &mut LoweringContext<'_, '_>,
    array_value: LoweredValue,
    index: &Expr,
    expr: &Expr,
) -> LoweredValue {
    match array_value.ir_type {
        IrType::Heap(IrHeapKind::Array) => {
            let mut index_value = lower_expr(ctx, index);
            let index_ty = isset_index_expr_key_type(ctx, index, index_value.value);
            let result = if index_ty == PhpType::Int {
                index_value = coerce_array_key_to_int_at_span(ctx, index_value, Some(index.span), false);
                ctx.emit_value(
                    Op::ArrayIsset,
                    vec![array_value.value, index_value.value],
                    None,
                    PhpType::Bool,
                    Op::ArrayIsset.default_effects(),
                    Some(expr.span),
                )
            } else {
                // `isset()` is a silent probe even when the key is absent.
                let read_value = ctx.emit_value(
                    Op::ArrayGetMixedKeySilent,
                    vec![array_value.value, index_value.value],
                    None,
                    PhpType::Mixed,
                    Op::ArrayGetMixedKeySilent.default_effects(),
                    Some(expr.span),
                );
                let is_null = ctx.emit_value(
                    Op::IsNull,
                    vec![read_value.value],
                    None,
                    PhpType::Bool,
                    Op::IsNull.default_effects(),
                    Some(expr.span),
                );
                crate::ir_lower::ownership::release_if_owned(
                    ctx,
                    read_value,
                    Some(expr.span),
                );
                release_owned_probe_key(ctx, index_value, expr);
                let zero = ctx.emit_value(
                    Op::ConstI64,
                    Vec::new(),
                    Some(Immediate::I64(0)),
                    PhpType::Int,
                    Op::ConstI64.default_effects(),
                    Some(expr.span),
                );
                ctx.emit_value(
                    Op::ICmp,
                    vec![is_null.value, zero.value],
                    Some(Immediate::CmpPredicate(crate::ir::CmpPredicate::Eq)),
                    PhpType::Bool,
                    Op::ICmp.default_effects(),
                    Some(expr.span),
                )
            };
            // Ordinary array reads stabilize their result and retire an owning receiver. Native
            // `isset` returns only a bool, so no stabilization is needed, but the receiver still
            // has to be released when it was projected from widened Mixed frame storage.
            release_owning_receiver_temporary(ctx, array_value, expr.span);
            result
        }
        _ => {
            let index_value = lower_expr(ctx, index);
            let checked = ctx.emit_value(
                Op::HashIsset,
                vec![array_value.value, index_value.value],
                None,
                PhpType::Bool,
                Op::HashIsset.default_effects(),
                Some(expr.span),
            );
            release_owned_probe_key(ctx, index_value, expr);
            release_owning_receiver_temporary(ctx, array_value, expr.span);
            checked
        }
    }
}

/// Releases a probe key the key expression allocated (`(string) $mixed`, a call result): the
/// probe only reads it. A key cast from a `foreach` key leaked one string per `isset()`, which
/// is what every DOM `loadHTML()` with an attribute did through its valueless-attribute check.
fn release_owned_probe_key(ctx: &mut LoweringContext<'_, '_>, key: LoweredValue, expr: &Expr) {
    if ctx.value_is_owning_temporary(key) {
        crate::ir_lower::ownership::release_if_owned(ctx, key, Some(expr.span));
    }
}

/// Probes a receiver that is neither an indexed array nor a hash; the paths it takes release
/// their own receiver.
fn lower_non_native_isset_offset_probe(
    ctx: &mut LoweringContext<'_, '_>,
    array_value: LoweredValue,
    index: &Expr,
    expr: &Expr,
) -> LoweredValue {
    // An `ArrayAccess` receiver whose EXPRESSION could not name its class — a call is
    // typed `int` before lowering — reaches this arm rather than the syntactic
    // `offsetExists` path. Decide it here on the value that was actually produced:
    // `isset($obj[$k])` is `offsetExists` in PHP, never `offsetGet`, and an
    // implementation is free to make the two observably different.
    let receiver_ty = ctx.builder.value_php_type(array_value.value);
    if type_satisfies_array_access_for_ir(ctx, &receiver_ty) {
        return lower_method_call_with_receiver(
            ctx,
            array_value,
            "offsetExists",
            &[index.clone()],
            Op::MethodCall,
            expr,
        );
    }
    if matches!(receiver_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        && program_may_hold_array_access_objects(ctx)
    {
        return lower_mixed_isset_dim_probe(ctx, array_value, index, expr);
    }
    let read_value = lower_array_access_from_value(ctx, array_value, index, expr, false);
    emit_builtin_call_value(
        ctx,
        "isset",
        vec![read_value.value],
        PhpType::Int,
        expr.span,
        None,
    )
}

/// Returns whether a `mixed` value in this program can be an `ArrayAccess` object whose
/// `offsetExists()` and `offsetGet()` may disagree: a class the PROGRAM declares implements the
/// interface, or PHP loaded at run time can declare one.
///
/// The `offsetExists()` arm is a method call on an unknown receiver, which links the eval bridge.
/// Emitting it unconditionally put the interpreter into EVERY program, because the always-present
/// session prelude probes `isset($options[$key])` on a mixed array: a two-line script grew from
/// 5.8 MB to 28 MB of assembly, and its runtime errors went through the bridge's throw path. The
/// built-in SPL containers (and the compiler's own `__Elephc*` subclasses of them) are always
/// declared and answer the quiet read the way `offsetExists()` does, so they do not count.
pub(super) fn program_may_hold_array_access_objects(ctx: &LoweringContext<'_, '_>) -> bool {
    ctx.has_eval_barrier()
        || ctx.eval_executed()
        || ctx.can_resolve_runtime_dynamic_functions()
        || ctx.program_loads_code_at_run_time()
        || ctx.classes.keys().any(|class| {
            elephc_builtin_contract::lookup_class(class).is_none()
                && !class.trim_start_matches('\\').to_ascii_lowercase().starts_with("__elephc")
                && class_implements_interface_for_ir(ctx, class, "ArrayAccess")
        })
}

/// Lowers `isset($mixed[$k])` for a receiver whose kind is known only at run time.
///
/// php answers `isset()` on an `ArrayAccess` object with `offsetExists()` alone. The quiet read
/// this used to take calls `offsetGet()` instead, and a container's `offsetGet` RESOLVES the
/// key: Laravel's `isset(static::$app[$name])` answered true for a binding that does not exist,
/// so `Date` skipped installing its default factory and the next read failed with "Target class
/// [date] does not exist". Every other receiver keeps the quiet read.
fn lower_mixed_isset_dim_probe(
    ctx: &mut LoweringContext<'_, '_>,
    array_value: LoweredValue,
    index: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let data = ctx.intern_class_name("ArrayAccess");
    let is_array_access = ctx.emit_value(
        Op::InstanceOf,
        vec![array_value.value],
        Some(Immediate::Data(data)),
        PhpType::Bool,
        Op::InstanceOf.default_effects(),
        Some(expr.span),
    );
    let temp_name = ctx.declare_hidden_temp(PhpType::Bool);
    let access_block = ctx
        .builder
        .create_named_block("isset.array_access", Vec::new());
    let read_block = ctx.builder.create_named_block("isset.mixed_read", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("isset.array_access.merge", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_array_access.value,
        then_target: access_block,
        then_args: Vec::new(),
        else_target: read_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(access_block);
    let exists = lower_method_call_with_receiver(
        ctx,
        array_value,
        "offsetExists",
        &[index.clone()],
        Op::MethodCall,
        expr,
    );
    let exists = ctx.truthy_consuming(exists, Some(expr.span));
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, exists, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(read_block);
    let read_value = lower_array_access_from_value(ctx, array_value, index, expr, false);
    let isset = emit_builtin_call_value(
        ctx,
        "isset",
        vec![read_value.value],
        PhpType::Int,
        expr.span,
        None,
    );
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, isset, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp_name, expr.span)
}

/// Lowers `isset($object->property)` without performing a normal property read first.
pub(super) fn lower_lazy_property_isset_operand(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    arg: &Expr,
) -> Option<LoweredValue> {
    match property_isset_action(ctx, object, property, arg)? {
        IssetPropertyAction::Fallback => None,
        IssetPropertyAction::Magic => {
            let object = lower_member_receiver(ctx, object, arg);
            Some(lower_magic_property_isset(ctx, object, property, arg))
        }
        IssetPropertyAction::AlwaysFalse => {
            lower_member_receiver(ctx, object, arg);
            Some(emit_bool_literal(ctx, false, Some(arg.span)))
        }
        IssetPropertyAction::Initialized => {
            let object = lower_subscript_receiver_silently(ctx, object);
            let object = refine_member_receiver(ctx, object, arg);
            Some(lower_initialized_property_isset(ctx, object, property, arg))
        }
    }
}

/// Describes how `isset($object->property)` should be lowered for a known receiver class.
pub(super) enum IssetPropertyAction {
    Fallback,
    Magic,
    AlwaysFalse,
    /// A declared (typed) property slot, which can be uninitialized: probe the slot
    /// before reading it so `isset()` never raises the uninitialized-read error.
    Initialized,
}

/// Selects the PHP-visible `isset()` behavior for a statically known object property operand.
pub(super) fn property_isset_action(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    member: &Expr,
) -> Option<IssetPropertyAction> {
    let Some((class_name, _)) = checked_object_expr_class(ctx, object, member) else {
        // No single receiver class — an untyped parameter, a `mixed` local, a union of two
        // classes. The probe resolves the class at run time and answers "initialized" for every
        // shape it cannot settle there, which sends this back to the ordinary read. Without it
        // the read happened unconditionally and `isset($untyped->typedProp)` raised the
        // uninitialized-typed-property fatal where PHP simply answers false.
        return Some(IssetPropertyAction::Initialized);
    };
    if is_builtin_stdclass_name(&class_name) {
        return Some(IssetPropertyAction::Fallback);
    }
    let class_info = ctx.classes.get(class_name.as_str())?;
    if class_info.allow_dynamic_properties {
        return Some(IssetPropertyAction::Fallback);
    }
    if property_is_accessible_for_ir(ctx, &class_name, class_info, property) {
        if let Some((index, (_, property_ty))) = class_info.visible_property(property) {
            if class_info.property_slot_is_declared(index, property)
                || (!class_info.property_slot_is_reference(index, property)
                    && property_ty.codegen_repr() == PhpType::Mixed)
            {
                return Some(IssetPropertyAction::Initialized);
            }
        }
        return Some(IssetPropertyAction::Fallback);
    }
    if class_method_signature(ctx, &class_name, &php_symbol_key("__isset")).is_some() {
        Some(IssetPropertyAction::Magic)
    } else {
        Some(IssetPropertyAction::AlwaysFalse)
    }
}

/// Returns the single receiver class and whether that receiver may be null.
pub(super) fn isset_object_expr_class(ctx: &LoweringContext<'_, '_>, object: &Expr) -> Option<(String, bool)> {
    isset_object_expr_type(ctx, object)
        .and_then(|ty| {
            let (class_name, nullable) = singular_object_class(&ty)?;
            normalized_class_name(class_name).map(|name| (name, nullable))
        })
        // Receivers the syntactic probe cannot type (subscripts, method-call chains) resolve
        // through the shared instance-call receiver rules.
        .or_else(|| instance_callable_object_class_and_nullability(ctx, object))
}

/// Resolves a silent mutation/probe's receiver class before selecting its storage behavior.
pub(super) fn checked_object_expr_class(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    member: &Expr,
) -> Option<(String, bool)> {
    let checked = isset_object_expr_type(ctx, object).and_then(|ty| {
        let ty = checked_member_receiver_type(ctx, &ty, member).unwrap_or(ty);
        let (class_name, nullable) = singular_object_class(&ty)?;
        normalized_class_name(class_name).map(|name| (name, nullable))
    });
    checked.or_else(|| isset_object_expr_class(ctx, object))
}

/// Resolves the receiver type without evaluating a property probe's source expression.
fn isset_object_expr_type(ctx: &LoweringContext<'_, '_>, object: &Expr) -> Option<PhpType> {
    Some(match &object.kind {
        ExprKind::Variable(name) => ctx.local_type(name),
        ExprKind::This => PhpType::Object(ctx.current_class.clone()?),
        ExprKind::NewObject { class_name, .. } => PhpType::Object(class_name.to_string()),
        ExprKind::NewDynamicObject { fallback_class, .. } => {
            PhpType::Object(fallback_class.to_string())
        }
        ExprKind::PropertyAccess { object, property } => {
            property_access_expr_type_for_ir(ctx, object, property)?
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            static_property_result_type(ctx, receiver, property, object)
        }
        ExprKind::FunctionCall { name, .. } => ctx
            .functions
            .get(name.as_str())
            .map(|sig| sig.return_type.clone())
            .unwrap_or_else(|| infer_expr_type_syntactic(object)),
        _ => infer_expr_type_syntactic(object),
    })
}

/// Returns whether a named property can use normal `isset()` value probing.
pub(super) fn property_is_accessible_for_ir(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    class_info: &crate::types::ClassInfo,
    property: &str,
) -> bool {
    if class_info.visible_property(property).is_none() {
        return false;
    }
    class_info
        .property_visibilities
        .get(property)
        .is_none_or(|visibility| {
            let declaring_class = class_info
                .property_declaring_classes
                .get(property)
                .map(String::as_str)
                .unwrap_or(class_name);
            ir_can_access_member(ctx, declaring_class, visibility)
        })
}

/// Checks PHP member visibility from the current lowering class scope.
pub(super) fn ir_can_access_member(
    ctx: &LoweringContext<'_, '_>,
    declaring_class: &str,
    visibility: &Visibility,
) -> bool {
    match visibility {
        Visibility::Public => true,
        Visibility::Private => ctx
            .current_class
            .as_deref()
            .is_some_and(|current| same_php_class_name(current, declaring_class)),
        Visibility::Protected => ctx.current_class.as_deref().is_some_and(|current| {
            same_php_class_name(current, declaring_class)
                || class_extends_class(ctx, current, declaring_class)
                || class_extends_class(ctx, declaring_class, current)
        }),
    }
}

/// Returns true when two class metadata names match PHP's case-insensitive class lookup.
pub(super) fn same_php_class_name(left: &str, right: &str) -> bool {
    php_symbol_key(left.trim_start_matches('\\')) == php_symbol_key(right.trim_start_matches('\\'))
}

/// Lowers a magic `__isset($name)` call and coerces the result to PHP boolean semantics.
pub(super) fn lower_magic_property_isset(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    arg: &Expr,
) -> LoweredValue {
    if value_is_nullable(ctx, object.value) {
        return lower_nullable_magic_property_isset(ctx, object, property, arg);
    }
    let args = vec![Expr::new(
        ExprKind::StringLiteral(property.to_string()),
        arg.span,
    )];
    let result =
        lower_method_call_with_receiver(ctx, object, "__isset", &args, Op::MethodCall, arg);
    ctx.truthy_consuming(result, Some(arg.span))
}

/// Lowers `__isset` for nullable receivers, returning false instead of calling on null.
pub(super) fn lower_nullable_magic_property_isset(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    arg: &Expr,
) -> LoweredValue {
    let temp_name = ctx.declare_hidden_temp(PhpType::Bool);
    let null_block = ctx
        .builder
        .create_named_block("isset.property.null", Vec::new());
    let call_block = ctx
        .builder
        .create_named_block("isset.property.call", Vec::new());
    let merge = ctx
        .builder
        .create_named_block("isset.property.merge", Vec::new());
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![object.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(arg.span),
    );
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
    let args = vec![Expr::new(
        ExprKind::StringLiteral(property.to_string()),
        arg.span,
    )];
    let result =
        lower_method_call_with_receiver(ctx, object, "__isset", &args, Op::MethodCall, arg);
    let result = ctx.truthy_consuming(result, Some(arg.span));
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, result, arg.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    ctx.load_local(&temp_name, Some(arg.span))
}
