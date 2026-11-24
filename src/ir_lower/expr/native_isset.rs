//! Purpose:
//! Native array and property isset probes.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Probe keys are borrowed for lookup and owned expression leases retire afterwards.
//! - Provisional key releases are pruned when final local storage stays borrowed.
//! - Global keys and other borrowed operands retain their existing owner.

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

/// Lowers nullable native array/hash `isset` while evaluating the offset even on null receivers.
pub(super) fn lower_nullable_native_isset_offset_probe(
    ctx: &mut LoweringContext<'_, '_>,
    array_value: LoweredValue,
    index: &Expr,
    expr: &Expr,
) -> LoweredValue {
    let (array_value, receiver_owner) = root_owned_call_operand(ctx, array_value, expr.span);
    let receiver_type = ctx.builder.value_php_type(array_value.value);
    let borrowed = if Ownership::php_type_needs_lifetime_tracking(&receiver_type) {
        let borrowed = ctx.emit_value(Op::Borrow, vec![array_value.value], None, receiver_type,
            Op::Borrow.default_effects(), Some(expr.span));
        ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
        borrowed
    } else { array_value };
    let (index, index_temp) = evaluate_once(ctx, index, expr);
    let key_owner = index_temp.as_ref().filter(|name|
        Ownership::php_type_needs_lifetime_tracking(&ctx.local_type(name)))
        .map(|name| ctx.local_slots[name]);
    if let Some(slot) = key_owner { register_owned_call_operand(ctx, slot, expr.span); }
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
    let false_value = emit_bool_literal(ctx, false, Some(expr.span));
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, false_value, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(probe_block);
    let checked = lower_native_isset_offset_probe_from_value(ctx, borrowed, &index, expr);
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, checked, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    if let Some(slot) = key_owner { unregister_owned_call_operand(ctx, slot, expr.span); }
    release_evaluated_once(ctx, index_temp, expr.span);
    if let Some(slot) = receiver_owner { retire_owned_call_operand(ctx, slot, expr.span); }
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
                // `isset()` is a silent probe even when the key is absent. A key born of checked
                // integer arithmetic can still be a float that php reports ("not representable
                // as an int"), and that warning needs this statement's line.
                let mut probe_effects = Op::ArrayGetMixedKeySilent.default_effects();
                if ctx.builder.value_defining_instruction(index_value.value).is_some_and(|producer| {
                    matches!(producer.op, Op::ICheckedAdd | Op::ICheckedSub | Op::ICheckedMul)
                }) {
                    probe_effects |= Effects::MAY_WARN;
                }
                let read_value = ctx.emit_value(
                    Op::ArrayGetMixedKeySilent,
                    vec![array_value.value, index_value.value],
                    None,
                    PhpType::Mixed,
                    probe_effects,
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
            // A later foreach can widen a string key's slot after this probe was lowered.
            // Its load then owns a detached copy; finalization prunes concrete borrowed reads.
            if ctx.value_needs_release_after_use(index_value) {
                crate::ir_lower::ownership::release_if_owned(ctx, index_value, Some(index.span));
            }
            // The receiver is retired by the caller (`lower_native_isset_offset_probe_from_value`);
            // releasing it here as well over-released a widened local's table.
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
            if ctx.value_needs_release_after_use(index_value) {
                crate::ir_lower::ownership::release_if_owned(ctx, index_value, Some(index.span));
            }
            checked
        }
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
/// built-in SPL containers (and the compiler's own `__Elephc*` subclasses of them) answer the
/// quiet read the way `offsetExists()` does, so they do not count. WeakMap does count: its
/// `offsetGet()` throws for a missing key, while the existence probe must remain silent.
pub(super) fn program_may_hold_array_access_objects(ctx: &LoweringContext<'_, '_>) -> bool {
    ctx.has_eval_barrier()
        || ctx.eval_executed()
        || ctx.can_resolve_runtime_dynamic_functions()
        || ctx.program_loads_code_at_run_time()
        || ctx.program_names_weak_map()
        || super::array_access_types::program_declares_array_access(ctx)
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

/// Returns whether a syntactic array receiver can use a non-materializing native `isset` probe.
pub(super) fn array_access_expr_supports_native_isset_probe(
    ctx: &LoweringContext<'_, '_>,
    array: &Expr,
) -> bool {
    let ty = match &array.kind {
        ExprKind::Variable(name) => ctx
            .local_types
            .get(name)
            .cloned()
            .unwrap_or_else(|| infer_expr_type_syntactic(array)),
        ExprKind::PropertyAccess { object, property } => {
            property_access_expr_type_for_ir(ctx, object, property)
                .unwrap_or_else(|| {
                    // A receiver with no single class (`SimpleXMLElement|false`, `mixed`) may be
                    // an XML object whose `->child` is a selection: the syntactic `int` guess
                    // would send `isset($x->child[1])` down the array probe and answer false.
                    if ctx.classes.contains_key("SimpleXMLElement")
                        && matches!(
                            array_cast_source::array_cast_source_type_for_ir(ctx, object).codegen_repr(),
                            PhpType::Mixed | PhpType::Union(_)
                        )
                    {
                        PhpType::Mixed
                    } else {
                        infer_expr_type_syntactic(array)
                    }
                })
        }
        ExprKind::ArrayLiteral(items) => array_literal_type_for_ir(ctx, items, array),
        ExprKind::ArrayLiteralAssoc(pairs) => assoc_array_literal_type_for_ir(ctx, pairs, array),
        _ => infer_expr_type_syntactic(array),
    }
    .codegen_repr();
    matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Int
        | PhpType::Float | PhpType::Bool | PhpType::Void | PhpType::Never | PhpType::TaggedScalar)
}

/// Lowers `isset($object->property)` without performing a normal property read first.
pub(super) fn lower_lazy_property_isset_operand(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    arg: &Expr,
) -> Option<LoweredValue> {
    if needs_native_xml_property_dispatch(ctx, object, property) {
        let object = lower_member_receiver(ctx, object, arg);
        return Some(lower_boxed_xml_property_isset(ctx, object, property, arg));
    }
    match property_isset_action(ctx, object, property, arg)? {
        // The ordinary read would warn `Undefined property` for a name nothing created; `isset()`
        // is a silent probe, so a plain (non-nullsafe) access asks the quiet fetch instead.
        IssetPropertyAction::Fallback if matches!(arg.kind, ExprKind::PropertyAccess { .. }) => {
            let object = lower_subscript_receiver_silently(ctx, object);
            let probed = super::property_access::lower_property_isset_probe_from_value(
                ctx, object, property, arg);
            Some(super::lazy_isset::probe_value_is_set(ctx, probed, arg))
        }
        IssetPropertyAction::Fallback => {
            let probed = super::nullsafe_chain::lower_with_missing_warning(ctx, arg, false)?;
            Some(super::lazy_isset::probe_value_is_set(ctx, probed, arg))
        }
        IssetPropertyAction::Magic => {
            let object = lower_member_receiver(ctx, object, arg);
            Some(lower_magic_property_isset(ctx, object, property, arg))
        }
        IssetPropertyAction::AlwaysFalse => {
            lower_member_receiver(ctx, object, arg);
            Some(emit_bool_literal(ctx, false, Some(arg.span)))
        }
        IssetPropertyAction::Initialized => {
            if matches!(object.kind, ExprKind::ArrayAccess { .. }) {
                let probed = super::array_access::lower_quiet_element_property(ctx, object, property, arg);
                return Some(super::lazy_isset::probe_value_is_set(ctx, probed, arg));
            }
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
    if crate::types::builtin_classes::native_property_method(&class_name, property,
        elephc_builtin_contract::ObjectProperty::Isset, ctx.classes).is_some()
    { return Some(IssetPropertyAction::Magic); }
    if class_info.allow_dynamic_properties
        || (class_info.dynamic_property_hash_is_name_addressable()
            && class_info.visible_property(property).is_none())
    {
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
        let declared_storage = matches!(
            object.kind,
            ExprKind::PropertyAccess { .. } | ExprKind::StaticPropertyAccess { .. }
        );
        let ty = checked_member_receiver_type(ctx, &ty, member, declared_storage).unwrap_or(ty);
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
pub(crate) fn property_is_accessible_for_ir(
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
pub(crate) fn ir_can_access_member(
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
    let method = native_property_method_for_value(ctx, object.value, property,
        elephc_builtin_contract::ObjectProperty::Isset).unwrap_or("__isset");
    let result = lower_method_call_with_receiver(ctx, object, method, &args, Op::MethodCall, arg);
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
    let method = native_property_method_for_value(ctx, object.value, property,
        elephc_builtin_contract::ObjectProperty::Isset).unwrap_or("__isset");
    let result = lower_method_call_with_receiver(ctx, object, method, &args, Op::MethodCall, arg);
    let result = ctx.truthy_consuming(result, Some(arg.span));
    store_value_into_temp(ctx, &temp_name, PhpType::Bool, result, arg.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    ctx.load_local(&temp_name, Some(arg.span))
}
