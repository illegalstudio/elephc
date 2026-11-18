//! Purpose:
//! Instance property array mutations and retaining-store cleanup.
//!
//! Called from:
//! - `crate::ir_lower::stmt`.
//!
//! Key details:
//! - Preserves statement ordering, CFG shape, EIR effects, and ownership contracts.

use super::*;
use crate::parser::ast::BinOp;

mod native_xml;
mod native_property_children;
mod magic_append;
mod magic_index;
mod compound;
mod incdec;
mod runtime_receivers;
pub(super) use incdec::native as lower_native_incdec;
pub(crate) use incdec::step as with_dimension_incdec_value;
use compound::{compound_source, with_compound_value, write_ordinary_compound};

/// Selects declaration initialization rules for a property used as a dimension container.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum PropertyDimensionFetchMode { Write, ReadWrite }

/// Shares the declared-writer candidate rule with runtime-name update planning.
pub(super) fn has_declared_readwrite_candidate(
    ctx: &LoweringContext<'_, '_>, name: &str, info: &crate::types::ClassInfo, property: &str,
) -> bool {
    runtime_receivers::has_declared_readwrite_candidate(ctx, name, info, property)
}

/// Guards a stabilized expression increment receiver before its prelude reads the element.
pub(super) fn guard_property_dimension_readwrite(
    ctx: &mut LoweringContext<'_, '_>, object: &Expr, property: &str, span: Span,
) {
    if !runtime_receivers::may_need_guard(ctx, property, PropertyDimensionFetchMode::ReadWrite)
        || !matches!(object.kind, ExprKind::Variable(_) | ExprKind::This) { return; }
    let object = lower_expr(ctx, object);
    let receiver = property_write_receiver::PropertyWriteReceiver::new(ctx, object, span);
    runtime_receivers::guard_readwrite(ctx, receiver.value, property, span);
    receiver.finish(ctx, span);
}

/// Keeps the owning receiver visible across either write-only or read-modify-write validation.
pub(crate) fn guard_property_dimension_fetch(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str, span: Span,
    mode: PropertyDimensionFetchMode,
) {
    if !runtime_receivers::may_need_guard(ctx, property, mode) { return; }
    let pins = crate::ir_lower::expr::pin_in_flight_owners(ctx, &[object.value], span);
    match mode {
        PropertyDimensionFetchMode::Write => runtime_receivers::guard_write(ctx, object, property, span),
        PropertyDimensionFetchMode::ReadWrite => runtime_receivers::guard_readwrite(ctx, object, property, span),
    }
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, pins, span);
}

/// Defers storage-only receiver chains while preserving early evaluation of computed receivers.
pub(super) fn deferred_property_receiver(ctx: &LoweringContext<'_, '_>, object: &Expr) -> bool {
    match &object.kind {
        ExprKind::Variable(_) | ExprKind::This | ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::PropertyAccess { object, .. } => deferred_property_receiver(ctx, object),
        ExprKind::DynamicPropertyAccess { .. } | ExprKind::ArrayAccess { .. } =>
            crate::ir_lower::expr::defer_native_property_write_receiver(ctx, object),
        _ => false,
    }
}

/// Returns the one owner of a property container the write below mutates and stores back.
///
/// A Mixed property read is already an owning temporary (codegen retains the box it returns),
/// and acquiring it again gave the write TWO owners while the cleanup after the store releases
/// one: `$this->data[$k] = $v` on an untyped property left the box one reference high per write,
/// so the whole array outlived its object. A borrowed read (a typed container slot) is acquired.
fn own_property_container(
    ctx: &mut LoweringContext<'_, '_>,
    property_value: LoweredValue,
    span: Span,
) -> LoweredValue {
    if ctx.value_is_owning_temporary(property_value) {
        return property_value;
    }
    crate::ir_lower::ownership::acquire_if_refcounted(ctx, property_value, Some(span))
}

/// Lowers `$object->prop[] = value`.
pub(super) fn lower_property_array_push(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    value: &Expr,
    span: Span,
) {
    let receiver = Expr::new(ExprKind::PropertyAccess {
        object: Box::new(object.clone()), property: property.into(),
    }, span);
    if crate::ir_lower::expr::lower_weak_map_denied_append(ctx, &receiver, value, span) { return; }
    let mut roots = Vec::new();
    let (mut receiver, value) = if deferred_property_receiver(ctx, object) {
        let value = lower_rooted_property_write_operand(ctx, value, &mut roots);
        let receiver = crate::ir_lower::expr::lower_static_property_write_chain(ctx, object, value, span);
        (receiver, value)
    } else {
        let object = crate::ir_lower::expr::lower_property_write_receiver(ctx, object);
        let mut receiver = property_write_receiver::PropertyWriteReceiver::new(ctx, object, span);
        let value = lower_rooted_property_write_operand(ctx, value, &mut roots);
        receiver.place_above_operand_roots(ctx, &roots, span);
        (receiver, value)
    };
    receiver.narrow_for_array(ctx, property, &[value.value], span);
    if let Some(child) = native_xml::fetch(ctx, &receiver, object, property, span)
        .or_else(|| lower_native_dimension_property(ctx, receiver.value, object, property, span,
            PropertyDimensionFetchMode::Write).map(native_xml::Fetch::known)) {
        native_xml::write(ctx, receiver, child, property, None, None, value, None, span, false);
        retire_property_write_roots(ctx, roots, span);
        return;
    }
    lower_property_array_push_value(ctx, receiver.value, property, value, span);
    receiver.finish(ctx, span);
    retire_property_write_roots(ctx, roots, span);
}

/// Mutates the array property while the caller holds its receiver lease.
///
/// The value is already evaluated (and rooted) by the caller, so the container is always fetched
/// after the right-hand side, which is PHP's delayed dim fetch.
fn lower_property_array_push_value(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str, value: LoweredValue, span: Span,
) {
    if runtime_receivers::lower_append(ctx, object, property, value, span) { return; }
    lower_property_array_push_value_direct(ctx, object, property, value, span);
}

/// Uses the already selected class without repeating runtime dispatch on its fallback.
fn lower_property_array_push_value_direct(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str, value: LoweredValue, span: Span,
) {
    if runtime_receivers::has_dynamic_slot(ctx, object.value, property) {
        native_xml::attached_append(ctx, object, property, value, span);
        return;
    }
    if object_property_type(ctx, object.value, property)
        .is_some_and(|ty| ty.codegen_repr() == PhpType::Iterable)
    {
        lower_iterable_property_write(ctx, object, property, None, value, span, false);
        return;
    }
    if object_property_type(ctx, object.value, property)
        .is_some_and(|ty| ty.is_php_array() || ty == PhpType::Mixed)
    {
        lower_php_array_property_write(ctx, object, property, None, value, span, false);
        return;
    }
    let initialize_uninitialized = is_concrete_object_receiver(ctx, object.value);
    if let Some(property_ty) = generic_object_array_property_type(ctx, object.value, property) {
        initialize_uninitialized_array_property_for_write(
            ctx,
            object.value,
            property,
            &property_ty,
            initialize_uninitialized,
            span,
        );
    }
    let generic_receiver = is_generic_object_receiver(ctx, object.value, property);
    if let Some(property_ty) =
        generic_object_array_property_type(ctx, object.value, property).filter(is_indexed_array_type)
    {
        let value = ctx.retype_value_for_container_element(&property_ty, value, Some(span));
        lower_property_element_write(ctx, object, property, &property_ty, value, span, |ctx, container| {
            ctx.emit_void(
                Op::ArrayPush,
                vec![container.value, value.value],
                None,
                Op::ArrayPush.default_effects(),
                Some(span),
            );
        });
        return;
    }
    if let Some(property_ty) =
        object_property_type(ctx, object.value, property).filter(is_assoc_array_type)
    {
        // An untyped slot a skipping integer key promoted to hash storage appends at the hash's
        // own next integer key, with the same retain discipline as the keyed `HashSet` write.
        let value = ctx.retype_value_for_container_element(&property_ty, value, Some(span));
        lower_property_element_write(ctx, object, property, &property_ty, value, span, |ctx, container| {
            ctx.emit_void(
                Op::HashAppend,
                vec![container.value, value.value],
                None,
                Op::HashAppend.default_effects(),
                Some(span),
            );
        });
        return;
    }
    if let Some(property_ty) = generic_object_array_property_type(ctx, object.value, property)
        .filter(|ty| {
            // A nullable ArrayAccess object (`?ArrayObject`, `?WeakMap`) boxes like a Mixed
            // property, but appending to it is `offsetSet(null, $v)` on the OBJECT; the Mixed
            // array append wrote into a detached copy and the push vanished.
            property_type_uses_mixed_array_storage(ty) && !type_satisfies_array_access_for_ir(ctx, ty)
        })
    {
        let data = ctx.intern_string(property);
        // The CONTAINER of an indexed write or unset is fetched quietly, the same as the
        // container of an `isset`/`empty` operand. PHP does not raise for an uninitialized
        // typed property here either: an indexed write auto-initializes it and an `unset` of
        // one of its elements is a no-op that leaves it uninitialized. Over an EVAL-OWNED
        // receiver this fetch leaves compiled code through the bridge, which raises unless the
        // mode is on -- `unset($this->p[$id])` and `$this->p[$id] = true` on the same property
        // as the `empty()` above are what still stopped the Symfony request once the operand
        // form was fixed.
        crate::ir_lower::expr::emit_quiet_property_fetch(ctx, true, span);
        let property_value = ctx.emit_value(
            Op::PropGet,
            vec![object.value],
            Some(Immediate::Data(data)),
            property_ty.clone(),
            Op::PropGet.default_effects(),
            Some(span),
        );
        crate::ir_lower::expr::emit_quiet_property_fetch(ctx, false, span);
        ctx.emit_void(
            Op::MixedArrayAppend,
            vec![property_value.value, value.value],
            None,
            Op::MixedArrayAppend.default_effects(),
            Some(span),
        );
        if generic_receiver {
            ctx.emit_void(
                Op::PropSet,
                vec![object.value, property_value.value],
                Some(Immediate::Data(data)),
                Op::PropSet.default_effects(),
                Some(span),
            );
            release_rewritten_property_value_after_retaining_store(
                ctx,
                &property_ty,
                property_value,
                span,
            );
        } else if ctx.value_is_owning_temporary(property_value) {
            // The append mutated the property's own box through this read, which owns one
            // reference to it (a Mixed property read retains); without the release every
            // `$this->items[] = $v` left the array one reference high and it outlived its object.
            crate::ir_lower::ownership::release_if_owned(ctx, property_value, Some(span));
        }
        return;
    }

    if let Some(property_ty) = generic_object_array_property_type(ctx, object.value, property)
        .filter(|ty| type_satisfies_array_access_for_ir(ctx, ty))
    {
        let data = ctx.intern_string(property);
        let property_value = ctx.emit_value(
            Op::PropGet,
            vec![object.value],
            Some(Immediate::Data(data)),
            property_ty,
            Op::PropGet.default_effects(),
            Some(span),
        );
        // A nullable receiver is a boxed `?Class`; the push dispatch keys on the class itself.
        let receiver_ty = ctx.builder.value_php_type(property_value.value);
        let operands = if matches!(receiver_ty, PhpType::Union(_) | PhpType::Mixed) {
            // The class is only known at run time, so the native `append()` shortcut the typed
            // form takes is unavailable: dispatch the generic `offsetSet(null, $v)`.
            let null_key = ctx
                .builder
                .emit_with_effects(
                    Op::ConstNull,
                    Vec::new(),
                    None,
                    IrType::I64,
                    PhpType::Void,
                    Ownership::NonHeap,
                    Op::ConstNull.default_effects(),
                    Some(span),
                )
                .expect("const_null produces a value");
            vec![property_value.value, null_key, value.value]
        } else {
            super::array_write_storage::object_push_operands(
                ctx,
                &receiver_ty,
                property_value,
                value,
                span,
            )
        };
        ctx.emit_void(
            Op::RuntimeCall,
            operands,
            None,
            effects_lookup::runtime_effects(),
            Some(span),
        );
        // `offsetSet()`/`append()` borrow the receiver and the value, as in the indexed write.
        if ctx.value_is_owning_temporary(property_value) {
            crate::ir_lower::ownership::release_if_owned(ctx, property_value, Some(span));
        }
        super::array_write_core::release_object_offset_set_operand(ctx, value, span);
        return;
    }


    native_xml::ordinary_append(ctx, object, property, value, span);
}

/// Lowers `$object->prop[index] = value`.
///
/// A desugared `??=` becomes a probe plus a conditional insert, and a desugared compound update
/// writes with the key its own read already converted (see `ElementUpdate`).
pub(super) fn lower_property_array_assign(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
) {
    let update = desugared_element_update(value, span, |read| {
        reads_property_array_element(read, object, property, index)
    });
    if matches!(&update, Some(ElementUpdate::Compound | ElementUpdate::IncDec { .. }))
        && crate::ir_lower::expr::lower_weak_map_incdec_stmt(ctx, value, span)
    {
        return;
    }
    if let Some(ElementUpdate::NullCoalesce { read, default }) = update {
        crate::ir_lower::expr::lower_null_coalesce_update_stmt(ctx, read, default, span);
        return;
    }
    if lower_string_property_offset_write(ctx, object, property, index, value, span) {
        return;
    }
    let element_update = match &update {
        Some(ElementUpdate::IncDec { increment }) => PropertyElementUpdate::IncDec(*increment),
        _ => PropertyElementUpdate::Written,
    };
    lower_property_array_assign_with_update(
        ctx,
        object,
        property,
        index,
        value,
        span,
        update.is_some(),
        element_update,
    );
}

/// Lowers `$this->types[$i] = 'i'` on a `string` property as php's byte-offset write: the string
/// is read into a hidden local, written through the local string-offset path, and stored back.
///
/// Only a receiver that is evaluated without effects (`$this`, a variable) is replayed; returns
/// `false`, leaving the generic lowering, for any other shape or property type.
fn lower_string_property_offset_write(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
) -> bool {
    if !matches!(object.kind, ExprKind::This | ExprKind::Variable(_)) {
        return false;
    }
    let property_ty =
        crate::ir_lower::expr::property_access_expr_type_for_ir(ctx, object, property);
    if property_ty.map(|ty| ty.codegen_repr()) != Some(PhpType::Str) {
        return false;
    }
    let temp = ctx.declare_synthetic_php_local(PhpType::Str);
    let property_read = Expr::new(
        ExprKind::PropertyAccess { object: Box::new(object.clone()), property: property.to_string() },
        span,
    );
    let statements = [
        StmtKind::Assign { name: temp.clone(), value: property_read },
        StmtKind::ArrayAssign { array: temp.clone(), index: index.clone(), value: value.clone() },
        StmtKind::PropertyAssign {
            object: Box::new(object.clone()),
            property: property.to_string(),
            value: Expr::new(ExprKind::Variable(temp), span),
        },
    ];
    for kind in statements {
        crate::ir_lower::stmt::lower_stmt(ctx, &crate::parser::ast::Stmt::new(kind, span));
        if ctx.builder.insertion_block_is_terminated() {
            break;
        }
    }
    true
}

/// Keeps scalar buffer updates on their existing typed read/step/store path.
fn known_buffer_property(ctx: &LoweringContext<'_, '_>, object: &Expr, property: &str) -> bool {
    let name = match &object.kind { ExprKind::This => "this", ExprKind::Variable(name) => name, _ => return false };
    let Some(PhpType::Object(class)) = ctx.local_load_php_type(name) else { return false; };
    ctx.classes.get(&class).and_then(|info| info.visible_property(property))
        .is_some_and(|(_, (_, ty))| matches!(ty.codegen_repr(), PhpType::Buffer(_)))
}

/// How a property-array element write may store a typed scalar buffer update.
#[derive(Clone, Copy)]
pub(super) enum PropertyElementUpdate {
    /// The value is lowered as written.
    Written,
    /// A statement `++`/`--` (`true` for `++`), typed once the receiver shows a scalar buffer.
    IncDec(bool),
    /// An expression `++`/`--` whose element read and step already ran, as the parser's prelude
    /// runs them before the write: the write stores this value instead of lowering `value`.
    Stored(LoweredValue),
}

/// Returns whether `read` is the element `$object->property[index]` a statement writes.
fn reads_property_array_element(read: &Expr, object: &Expr, property: &str, index: &Expr) -> bool {
    matches!(
        &read.kind,
        ExprKind::ArrayAccess { array, index: read_index }
            if read_index.as_ref() == index
                && matches!(
                    &array.kind,
                    ExprKind::PropertyAccess { object: read_object, property: read_property }
                        if read_object.as_ref() == object && read_property == property
                )
    )
}

/// Rejects a null receiver before a prelude statement reads the element `target` names, the
/// first step of an expression `++`/`--` or compound update of `$receiver->p[k]`.
///
/// PHP fetches the property for read-write there and throws before any read. The parser binds
/// a computed receiver to a variable and replays a side-effect-free one, so the receiver is a
/// variable or an element of array storage (`$nodes[5]`); both are probed.
pub(crate) fn reject_null_receiver_before_element_read(
    ctx: &mut LoweringContext<'_, '_>, target: &Expr, prelude: &Stmt, span: Span,
) {
    let ExprKind::ArrayAccess { array, index } = &target.kind else { return; };
    let ExprKind::PropertyAccess { object, property } = &array.kind else { return; };
    let StmtKind::Assign { value: read, .. } = &prelude.kind else { return; };
    // An increment captures the element itself; a compound binds `element OP rhs`.
    let read = match &read.kind {
        ExprKind::BinaryOp { left, .. } => left.as_ref(),
        _ => read,
    };
    if !reads_property_array_element(read, object, property, index) { return; }
    if matches!(object.kind, ExprKind::Variable(_)) {
        reject_null_variable_receiver(ctx, object, property, span);
    } else if reads_class_element_of_array_storage(ctx, object) {
        // The probe reports PHP's one `Undefined array key` warning for a missed element.
        let probe = crate::ir_lower::expr::lower_expr(ctx, object);
        runtime_receivers::reject_null_receiver_probe(ctx, probe, property, span);
    }
}

/// Returns whether `receiver` reads an element of array storage whose declared element is a
/// concrete class, the read that answers null for a missing key.
///
/// The chain is typed from declarations alone, through any depth of elements and declared
/// properties (`$nodes[5]`, `$grid[0][1]`, `$nodes[0]->peers[3]`). An `ArrayAccess` object, a
/// boxed container, a hooked property and an interface element are excluded: reading them
/// again could run user code, or their null is not this check's to make.
fn reads_class_element_of_array_storage(ctx: &LoweringContext<'_, '_>, receiver: &Expr) -> bool {
    matches!(receiver.kind, ExprKind::ArrayAccess { .. })
        && static_receiver_storage(ctx, receiver)
            .is_some_and(|ty| concrete_class(ctx, &ty).is_some())
}

/// Returns the declared storage type a side-effect-free receiver chain reads, when it is known.
fn static_receiver_storage(ctx: &LoweringContext<'_, '_>, receiver: &Expr) -> Option<PhpType> {
    match &receiver.kind {
        ExprKind::This => ctx.local_load_php_type("this"),
        ExprKind::Variable(name) => ctx.local_load_php_type(name),
        ExprKind::PropertyAccess { object, property } => {
            let class = concrete_class(ctx, &static_receiver_storage(ctx, object)?)?;
            let info = ctx.classes.get(&class)?;
            if info.property_hooks.get(property.as_str()).is_some_and(|hooks| hooks.any()) {
                return None;
            }
            info.visible_property(property).map(|(_, (_, ty))| ty.clone())
        }
        ExprKind::ArrayAccess { array, .. } => {
            match static_receiver_storage(ctx, array)?.codegen_repr() {
                PhpType::Array(element) | PhpType::AssocArray { value: element, .. } => Some(*element),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Returns the class of a value typed as one declared, non-interface class.
fn concrete_class(ctx: &LoweringContext<'_, '_>, ty: &PhpType) -> Option<String> {
    let PhpType::Object(class) = ty.codegen_repr() else { return None; };
    let class = class.trim_start_matches('\\').to_string();
    (!class.is_empty() && !ctx.interfaces.contains_key(&class) && ctx.classes.contains_key(&class))
        .then_some(class)
}

/// Probes a variable receiver without consuming it and rejects the null sentinel.
///
/// Of the storage-only receivers a write defers past its operands, only a variable can hold
/// the null sentinel of a missed element read.
fn reject_null_variable_receiver(
    ctx: &mut LoweringContext<'_, '_>, object: &Expr, property: &str, span: Span,
) {
    let ExprKind::Variable(name) = &object.kind else { return; };
    if !ctx.local_slots.contains_key(name.as_str()) { return; }
    let probe = ctx.load_local(name, Some(span));
    ctx.builder.set_value_ownership(probe.value, Ownership::Borrowed);
    runtime_receivers::reject_null_receiver(ctx, probe, property, span);
}

/// Lowers `$object->prop[index] = value`, optionally reusing a float-key diagnosis.
///
/// `key_already_diagnosed` marks the write half of a compound update: its read of the same
/// element already reported the float key's conversion, as PHP does once per update. An
/// `ArrayAccess` property receives the key unconverted, and a runtime-typed receiver keeps its
/// own conversion, so neither consults the flag.
pub(crate) fn lower_property_array_assign_with_diagnosed_key(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
    key_already_diagnosed: bool,
) {
    lower_property_array_assign_with_update(
        ctx,
        object,
        property,
        index,
        value,
        span,
        key_already_diagnosed,
        PropertyElementUpdate::Written,
    );
}

/// Lowers `$object->prop[index] = value`, storing a typed buffer update when `update` names one.
///
/// A statement `++`/`--` is decided after the receiver is lowered, from the same property type
/// that selects the `BufferSet` write below, and its element read then runs where `value` would.
/// `value` is not lowered when the write stores a typed update.
pub(super) fn lower_property_array_assign_with_update(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
    key_already_diagnosed: bool,
    update: PropertyElementUpdate,
) {
    let compound = compound_source(value, object, property, index, span);
    let incdec_results = value_incdec_locals(value);
    let index_expression = index;
    let delayed_incdec = match update {
        PropertyElementUpdate::IncDec(increment) if (ctx.classes.contains_key("SimpleXMLElement")
            || runtime_receivers::may_need_guard(ctx, property, PropertyDimensionFetchMode::ReadWrite))
            && !known_buffer_property(ctx, object, property) => Some(increment),
        _ => None,
    };
    if ctx.classes.contains_key("SimpleXMLElement")
        && (compound.is_some() || delayed_incdec.is_some())
    {
        crate::ir_lower::scoped_writes::consume_update_receiver(ctx, object);
    }
    let placeholder = Expr::new(ExprKind::Null, span);
    let late_index = (ctx.classes.contains_key("SimpleXMLElement")
        && !known_buffer_property(ctx, object, property)
        && matches!(index.kind, ExprKind::Variable(_))).then_some(index);
    let staged_index = if late_index.is_some() { &placeholder } else { index };
    let source_value = if delayed_incdec.is_some() { &placeholder } else { compound.map_or(value, |(_, rhs)| rhs) };
    let mut roots = Vec::new();
    // A `++`/`--` lowered from its desugared value reads the element while that value is staged;
    // PHP rejects a null receiver before the read. The parser hoisted the key, so no operand
    // effect is skipped.
    let early_null_check = matches!(update, PropertyElementUpdate::IncDec(_)) && delayed_incdec.is_none();
    let (mut receiver, index, value) = if deferred_property_receiver(ctx, object) && !known_buffer_property(ctx, object, property) {
        if early_null_check { reject_null_variable_receiver(ctx, object, property, span); }
        let source = match update {
            PropertyElementUpdate::Stored(stored) => PropertyWriteValue::Stored(stored),
            PropertyElementUpdate::Written | PropertyElementUpdate::IncDec(_) => {
                PropertyWriteValue::Expr(source_value)
            }
        };
        let (index, value) = lower_property_write_operands(ctx, staged_index, source, &mut roots, span);
        let receiver = crate::ir_lower::expr::lower_static_property_write_chain_with_operands(
            ctx, object, &[index.value, value.value], span);
        (receiver, index, value)
    } else {
        // A receiver reached through a nullable step is lowered boxed; its source decides.
        let element_receiver = reads_class_element_of_array_storage(ctx, object);
        let object = crate::ir_lower::expr::lower_property_write_receiver(ctx, object);
        let mut receiver = property_write_receiver::PropertyWriteReceiver::new(ctx, object, span);
        if early_null_check && element_receiver {
            runtime_receivers::reject_null_receiver_value(ctx, receiver.value, property, span);
        } else if early_null_check {
            runtime_receivers::reject_null_receiver(ctx, receiver.value, property, span);
        }
        let source = match update {
            PropertyElementUpdate::Written => PropertyWriteValue::Expr(source_value),
            PropertyElementUpdate::IncDec(_) if delayed_incdec.is_some() => PropertyWriteValue::Expr(&placeholder),
            PropertyElementUpdate::IncDec(increment) => {
                super::buffer_element_updates::property_statement_update(
                    ctx, receiver.value, property, value, increment,
                )
                .map_or(PropertyWriteValue::Expr(value), PropertyWriteValue::Buffer)
            }
            PropertyElementUpdate::Stored(stored) => PropertyWriteValue::Stored(stored),
        };
        let (index, value) = lower_property_write_operands(ctx, staged_index, source, &mut roots, span);
        receiver.place_above_operand_roots(ctx, &roots, span);
        // A compound or delayed `++`/`--` reads the element after its operands; the receiver
        // is rejected here, once they have run, as PHP's read-write fetch does.
        if element_receiver && !early_null_check && (compound.is_some() || delayed_incdec.is_some()) {
            runtime_receivers::reject_null_receiver_value(ctx, receiver.value, property, span);
        }
        (receiver, index, value)
    };
    receiver.narrow_for_array(ctx, property, &[index.value, value.value], span);
    let fetch_mode = if delayed_incdec.is_some() || compound.is_some() {
        PropertyDimensionFetchMode::ReadWrite
    } else { PropertyDimensionFetchMode::Write };
    if let Some(child) = native_xml::fetch(ctx, &receiver, object, property, span)
        .or_else(|| lower_native_dimension_property(ctx, receiver.value, object, property, span,
            fetch_mode).map(native_xml::Fetch::known)) {
        if let Some(increment) = delayed_incdec {
            native_xml::dispatch(ctx, receiver, child, span,
                |ctx, child| with_late_property_key(ctx, late_index, index, span,
                    |ctx, index| incdec::native(ctx, child, index, increment, incdec_results, span)),
                |ctx, object| with_late_property_key(ctx, late_index, index, span,
                    |ctx, index| incdec::ordinary(ctx, object, property, index, index_expression, increment, incdec_results, span)));
        } else {
            native_xml::write(ctx, receiver, child, property, Some(index), late_index, value,
                compound.map(|(op, _)| (op, index_expression)), span, key_already_diagnosed);
        }
        retire_property_write_roots(ctx, roots, span);
        return;
    }
    with_late_property_key(ctx, late_index, index, span, |ctx, index| {
        if let Some(increment) = delayed_incdec {
            incdec::ordinary(ctx, receiver.value, property, index, index_expression, increment, incdec_results, span);
        } else {
            write_ordinary_compound(ctx, receiver.value, property, index, value,
                compound.map(|(op, _)| (op, index_expression)), span, key_already_diagnosed);
        }
    });
    receiver.finish(ctx, span);
    retire_property_write_roots(ctx, roots, span);
}

/// Loads a bare variable key at mutation time, after a native parent destructor can change it.
fn with_late_property_key(
    ctx: &mut LoweringContext<'_, '_>, expression: Option<&Expr>, staged: LoweredValue, span: Span,
    write: impl FnOnce(&mut LoweringContext<'_, '_>, LoweredValue),
) {
    let Some(expression) = expression else { write(ctx, staged); return; };
    let mut roots = Vec::new();
    let key = lower_rooted_property_write_operand(ctx, expression, &mut roots);
    write(ctx, key);
    retire_property_write_roots(ctx, roots, span);
}

/// Reads a native XML child through its write-fetch while the parent lease protects the receiver.
fn lower_native_dimension_property(
    ctx: &mut LoweringContext<'_, '_>, receiver: LoweredValue, object: &Expr, property: &str, span: Span,
    mode: PropertyDimensionFetchMode,
) -> Option<LoweredValue> {
    let ty = ctx.builder.value_php_type(receiver.value);
    let (class, _) = crate::ir_lower::expr::singular_object_class(&ty)?;
    let info = ctx.classes.get(class.trim_start_matches('\\'))?;
    let native_property = crate::types::builtin_classes::native_property_method(class, property,
        elephc_builtin_contract::ObjectProperty::Get, ctx.classes).is_some();
    let native = if native_property {
        true
    } else if let Some((_, (_, ty))) = info.visible_property(property) {
        crate::ir_lower::expr::singular_object_class(ty).is_some_and(|(name, _)|
            crate::types::builtin_classes::has_native_dimensions(name, ctx.classes))
    } else { crate::types::builtin_classes::has_native_dimensions(class, ctx.classes) };
    if !native { return None; }
    let native_hook_owner = info.property_hooks.get(property).filter(|hooks| hooks.requires_get())
        .map(|_| info.property_declaring_classes.get(property).cloned().unwrap_or_else(|| class.to_string()));
    let borrowed = ctx.emit_value(Op::Borrow, vec![receiver.value], None, ty,
        Op::Borrow.default_effects(), Some(span));
    ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
    let expression = Expr::new(ExprKind::PropertyAccess {
        object: Box::new(object.clone()), property: property.into(),
    }, span);
    if native_property {
        return Some(crate::ir_lower::expr::lower_native_xml_property_write_fetch(ctx, borrowed, property, &expression));
    }
    guard_property_dimension_fetch(ctx, receiver, property, span, mode);
    let child = crate::ir_lower::expr::lower_property_get_from_value(
        ctx, borrowed, property, Op::PropGet, &expression,
    );
    if let Some(owner) = native_hook_owner {
        let null = ctx.emit_value(Op::IsNull, vec![child.value], None, PhpType::Bool,
            Op::IsNull.default_effects(), Some(span));
        let invalid = ctx.builder.create_named_block("xml.hook.null", vec![]);
        let selected = ctx.builder.create_named_block("xml.hook.object", vec![]);
        let initial = ctx.initialized_slots_snapshot();
        ctx.builder.terminate(Terminator::CondBr { cond: null.value,
            then_target: invalid, then_args: vec![], else_target: selected, else_args: vec![] });
        ctx.builder.position_at_end(invalid);
        crate::ir_lower::ownership::release_if_owned(ctx, child, Some(span));
        lower_throw_access_exception(ctx, "Error",
            &format!("Indirect modification of {}::${property} is not allowed", owner.trim_start_matches('\\')), span);
        ctx.builder.position_at_end(selected);
        ctx.restore_initialized_slots(initial);
    }
    Some(child)
}

/// The value a property-array write stores, resolved once the receiver is known.
enum PropertyWriteValue<'a> {
    /// The written expression, lowered and rooted as an ordinary operand.
    Expr(&'a Expr),
    /// A statement `++`/`--` of a scalar buffer element, read and stepped where `Expr` would run.
    Buffer(super::buffer_element_updates::BufferIncDec<'a>),
    /// A typed update the caller already read and stepped.
    Stored(LoweredValue),
}

/// Lowers a property-array write's key and stored value in PHP's variable-key order.
fn lower_property_write_operands(
    ctx: &mut LoweringContext<'_, '_>,
    index: &Expr,
    source: PropertyWriteValue<'_>,
    roots: &mut Vec<LocalSlotId>,
    span: Span,
) -> (LoweredValue, LoweredValue) {
    match source {
        PropertyWriteValue::Expr(value) => {
            lower_rooted_property_write_key_and_value(ctx, index, value, roots)
        }
        // The typed element values are scalars, so they need no unwind root.
        PropertyWriteValue::Buffer(buffer) => lower_rooted_property_write_key_then(
            ctx, index, roots, |ctx, _| buffer.lower_values(ctx, span).stored,
        ),
        PropertyWriteValue::Stored(stored) => {
            lower_rooted_property_write_key_then(ctx, index, roots, |_, _| stored)
        }
    }
}

/// Lowers one operand into an unwind-visible owner and lends a borrowed view to the writer.
fn lower_rooted_property_write_operand(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
    roots: &mut Vec<LocalSlotId>,
) -> LoweredValue {
    // `$this->items[$k] = &$x` stores a reference marker, which only the array-reference lowering
    // builds; any other operand falls through to `lower_expr` (see `nested_plain_writes`).
    let value = lower_array_reference_or_value(ctx, expr);
    root_property_write_value(ctx, value, expr.span, roots)
}

/// Publishes an already evaluated write operand and lends a non-consuming view to nested calls.
fn root_property_write_value(
    ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span, roots: &mut Vec<LocalSlotId>,
) -> LoweredValue {
    let (value, root) = crate::ir_lower::expr::root_owned_call_operand(ctx, value, span);
    let Some(root) = root else {
        return value;
    };
    roots.push(root);
    let ty = ctx.builder.value_php_type(value.value);
    let borrowed = ctx.emit_value(
        Op::Borrow,
        vec![value.value],
        None,
        ty,
        Op::Borrow.default_effects(),
        Some(span),
    );
    ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
    borrowed
}

/// Preserves PHP's variable-key timing while rooting every evaluated write operand immediately.
fn lower_rooted_property_write_key_and_value(
    ctx: &mut LoweringContext<'_, '_>,
    index: &Expr,
    value: &Expr,
    roots: &mut Vec<LocalSlotId>,
) -> (LoweredValue, LoweredValue) {
    lower_rooted_property_write_key_then(ctx, index, roots, |ctx, roots| {
        lower_rooted_property_write_operand(ctx, value, roots)
    })
}

/// Roots the key of a property-array write and runs `lower_value` in PHP's variable-key order.
///
/// A bare variable key is read after the value and a computed key before it, exactly as for
/// `lower_rooted_property_write_key_and_value`; `lower_value` receives the same root list.
fn lower_rooted_property_write_key_then<'m, 'f, T>(
    ctx: &mut LoweringContext<'m, 'f>,
    index: &Expr,
    roots: &mut Vec<LocalSlotId>,
    lower_value: impl FnOnce(&mut LoweringContext<'m, 'f>, &mut Vec<LocalSlotId>) -> T,
) -> (LoweredValue, T) {
    if matches!(index.kind, ExprKind::Variable(_)) {
        let value = lower_value(ctx, roots);
        return (lower_rooted_property_write_operand(ctx, index, roots), value);
    }
    let index = lower_rooted_property_write_operand(ctx, index, roots);
    let value = lower_value(ctx, roots);
    (index, value)
}

/// Releases rooted operands after the receiver lease and write have both completed.
fn retire_property_write_roots(
    ctx: &mut LoweringContext<'_, '_>, roots: Vec<LocalSlotId>, span: Span,
) {
    for root in roots.into_iter().rev() {
        crate::ir_lower::expr::retire_owned_call_operand(ctx, root, span);
    }
}

/// Mutates the indexed property while the caller holds its receiver lease.
///
/// The key and value are already evaluated (and rooted) by the caller, so the container is always
/// fetched after the right-hand side, which is PHP's delayed dim fetch.
fn lower_property_array_assign_value(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str, index: LoweredValue,
    value: LoweredValue, span: Span, key_already_diagnosed: bool,
) {
    if magic_index::lower(ctx, object, property, index, value, None, span, key_already_diagnosed,
        |ctx, object| lower_property_array_assign_value_without_magic(
            ctx, object, property, index, value, span, key_already_diagnosed)) { return; }
    lower_property_array_assign_value_without_magic(ctx, object, property, index, value, span, key_already_diagnosed);
}

/// Retains declared-slot COW after special runtime getter and visibility cases are excluded.
fn lower_property_array_assign_value_without_magic(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str, index: LoweredValue,
    value: LoweredValue, span: Span, diagnosed: bool,
) {
    if runtime_receivers::lower(ctx, object, property, index, value, span, diagnosed) { return; }
    lower_property_array_assign_value_direct(ctx, object, property, index, value, span, diagnosed);
}

/// Reuses typed storage after class selection without recursively dispatching an erased fallback.
fn lower_property_array_assign_value_direct(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str, index: LoweredValue,
    value: LoweredValue, span: Span, key_already_diagnosed: bool,
) {
    if object_property_type(ctx, object.value, property)
        .is_some_and(|ty| ty.codegen_repr() == PhpType::Iterable)
    {
        lower_iterable_property_write(
            ctx,
            object,
            property,
            Some(index),
            value,
            span,
            key_already_diagnosed,
        );
        return;
    }
    if object_property_type(ctx, object.value, property)
        .is_some_and(|ty| ty.is_php_array() || ty == PhpType::Mixed)
    {
        lower_php_array_property_write(
            ctx,
            object,
            property,
            Some(index),
            value,
            span,
            key_already_diagnosed,
        );
        return;
    }
    if let Some(property_ty) = object_property_type(ctx, object.value, property)
        .filter(|ty| matches!(ty.codegen_repr(), PhpType::Buffer(_)))
    {
        let data = ctx.intern_string(property);
        let property_value = ctx.emit_value(
            Op::PropGet,
            vec![object.value],
            Some(Immediate::Data(data)),
            property_ty.clone(),
            Op::PropGet.default_effects(),
            Some(span),
        );
        let property_value =
            crate::ir_lower::ownership::acquire_if_refcounted(ctx, property_value, Some(span));
        let index = coerce_to_int_at_span(ctx, index, Some(span));
        let value = coerce_buffer_set_value(ctx, &property_ty, value, Some(span));
        ctx.emit_void(
            Op::BufferSet,
            vec![property_value.value, index.value, value.value],
            None,
            Op::BufferSet.default_effects(),
            Some(span),
        );
        crate::ir_lower::ownership::release_if_owned(ctx, property_value, Some(span));
        return;
    }
    let initialize_uninitialized = is_concrete_object_receiver(ctx, object.value);
    if let Some(property_ty) = generic_object_array_property_type(ctx, object.value, property) {
        initialize_uninitialized_array_property_for_write(
            ctx,
            object.value,
            property,
            &property_ty,
            initialize_uninitialized,
            span,
        );
    }
    if let Some(property_ty) =
        generic_object_array_property_type(ctx, object.value, property).filter(is_indexed_array_type)
    {
        let value = ctx.retype_value_for_container_element(&property_ty, value, Some(span));
        // PHP reads a plain-variable index at STORE time, after the right-hand side, so
        // `$o->a[$i] = ($i = 1)` writes index 1. The bare-local write already used this
        // rule; sharing the helper is what keeps the two from answering differently for
        // the same source line.
        let index =
            coerce_array_key_to_int_at_span(ctx, index, Some(span), key_already_diagnosed);
        let value = coerce_indexed_array_set_value(ctx, &property_ty, value, Some(span));
        if index.ir_type == IrType::Str
            || (property_ty.codegen_repr() == PhpType::Array(Box::new(PhpType::Mixed))
                && index_is_boxed_mixed_key(index.ir_type))
        {
            // A string key promotes the indexed container to a hash, and a boxed key may be either:
            // both rewrite the container, so they keep the retaining read / write / store-back.
            let data = ctx.intern_string(property);
            // The CONTAINER of an indexed write or unset is fetched quietly, the same as the
            // container of an `isset`/`empty` operand. PHP does not raise for an uninitialized
            // typed property here either: an indexed write auto-initializes it and an `unset` of
            // one of its elements is a no-op that leaves it uninitialized. Over an EVAL-OWNED
            // receiver this fetch leaves compiled code through the bridge, which raises unless the
            // mode is on -- `unset($this->p[$id])` and `$this->p[$id] = true` on the same property
            // as the `empty()` above are what still stopped the Symfony request once the operand
            // form was fixed.
            crate::ir_lower::expr::emit_quiet_property_fetch(ctx, true, span);
            let property_value = ctx.emit_value(
                Op::PropGet,
                vec![object.value],
                Some(Immediate::Data(data)),
                property_ty.clone(),
                Op::PropGet.default_effects(),
                Some(span),
            );
            crate::ir_lower::expr::emit_quiet_property_fetch(ctx, false, span);
            let property_value = own_property_container(ctx, property_value, span);
            if index.ir_type == IrType::Str {
                let assoc_ty = promoted_assoc_array_type(
                    property_ty.clone(),
                    ctx.builder.value_php_type(value.value),
                );
                let hash = ctx.emit_value(
                    Op::ArrayToHash,
                    vec![property_value.value],
                    None,
                    assoc_ty.clone(),
                    Op::ArrayToHash.default_effects(),
                    Some(span),
                );
                ctx.emit_void(
                    Op::HashSet,
                    vec![hash.value, index.value, value.value],
                    key_already_diagnosed.then_some(Immediate::Bool(true)),
                    Op::HashSet.default_effects(),
                    Some(span),
                );
                release_persisted_string_operand(ctx, index, span);
                release_persisted_string_operand(ctx, value, span);
                ctx.emit_void(
                    Op::PropSet,
                    vec![object.value, hash.value],
                    Some(Immediate::Data(data)),
                    Op::PropSet.default_effects(),
                    Some(span),
                );
                release_rewritten_property_value_after_retaining_store(
                    ctx, &assoc_ty, hash, span,
                );
                return;
            }
            let rewritten = ctx.emit_value(
                Op::ArraySetMixedKey,
                vec![property_value.value, index.value, value.value],
                None,
                property_ty.clone(),
                Op::ArraySetMixedKey.default_effects(),
                Some(span),
            );
            // The helper stores its own reference to the value, exactly as on the local path
            // (`array_write_core::lower_mixed_key_array_set`): a boxed temporary and a string
            // the box copied are still this write's to drop.
            if matches!(
                ctx.builder.value_php_type(value.value).codegen_repr(),
                PhpType::Mixed | PhpType::Union(_)
            ) && ctx.value_is_owning_temporary(value)
            {
                crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
            }
            release_string_copied_into_box(ctx, value, span);
            ctx.emit_void(
                Op::PropSet,
                vec![object.value, rewritten.value],
                Some(Immediate::Data(data)),
                Op::PropSet.default_effects(),
                Some(span),
            );
            release_rewritten_property_value_after_retaining_store(
                ctx,
                &property_ty,
                rewritten,
                span,
            );
            return;
        }
        // The key and value conversions can warn, and a warning handler can replace the
        // property, so the container is fetched last, with nothing between it and the write.
        lower_property_element_write(ctx, object, property, &property_ty, value, span, |ctx, container| {
            ctx.emit_void(
                Op::ArraySet,
                vec![container.value, index.value, value.value],
                None,
                Op::ArraySet.default_effects(),
                Some(span),
            );
        });
        return;
    }
    if let Some(property_ty) =
        generic_object_array_property_type(ctx, object.value, property).filter(is_assoc_array_type)
    {
        let value = ctx.retype_value_for_container_element(&property_ty, value, Some(span));
        // PHP reads a plain-variable index at STORE time, after the right-hand side, so
        // `$o->a[$i] = ($i = 1)` writes index 1. The bare-local write already used this
        // rule; sharing the helper is what keeps the two from answering differently for
        // the same source line.
        lower_property_element_write(ctx, object, property, &property_ty, value, span, |ctx, container| {
            ctx.emit_void(
                Op::HashSet,
                vec![container.value, index.value, value.value],
                key_already_diagnosed.then_some(Immediate::Bool(true)),
                Op::HashSet.default_effects(),
                Some(span),
            );
        });
        // `release_property_array_insert_value_after_retain` releases only a value stored into
        // CONCRETE element slots. A hash stores a string by copying it, so a fresh string key or
        // value is still this write's to drop: `$this->a[$k] = (string) $v` leaked the cast's
        // copy on every call.
        let helper_released_value = indexed_property_array_element_type(&property_ty)
            .is_some_and(|element| {
                !matches!(element.codegen_repr(), PhpType::Mixed | PhpType::Callable)
            });
        release_persisted_string_operand(ctx, index, span);
        if !helper_released_value {
            release_persisted_string_operand(ctx, value, span);
        }
        return;
    }

    if let Some(property_ty) = generic_object_array_property_type(ctx, object.value, property)
        .filter(|ty| type_satisfies_array_access_for_ir(ctx, ty))
    {
        let data = ctx.intern_string(property);
        let property_value = ctx.emit_value(
            Op::PropGet,
            vec![object.value],
            Some(Immediate::Data(data)),
            property_ty,
            Op::PropGet.default_effects(),
            Some(span),
        );
        // Object offsetSet borrows its operands just like a local boxed writer.
        // Reuse its evaluation order and unwind roots for fresh keys and values; the same
        // helper roots and retires the receiver read (an OWNED unbox for a nullable or
        // union-typed property such as `?WeakMap`).
        array_write_core::lower_boxed_array_set_values(
            ctx,
            property_value,
            index,
            value,
            span,
            false,
        );
        return;
    }

    if let Some(property_ty) = generic_object_array_property_type(ctx, object.value, property)
        .filter(|ty| property_type_uses_mixed_array_storage(ty))
    {
        let data = ctx.intern_string(property);
        // The CONTAINER of an indexed write or unset is fetched quietly, the same as the
        // container of an `isset`/`empty` operand. PHP does not raise for an uninitialized
        // typed property here either: an indexed write auto-initializes it and an `unset` of
        // one of its elements is a no-op that leaves it uninitialized. Over an EVAL-OWNED
        // receiver this fetch leaves compiled code through the bridge, which raises unless the
        // mode is on -- `unset($this->p[$id])` and `$this->p[$id] = true` on the same property
        // as the `empty()` above are what still stopped the Symfony request once the operand
        // form was fixed.
        crate::ir_lower::expr::emit_quiet_property_fetch(ctx, true, span);
        let property_value = ctx.emit_value(
            Op::PropGet,
            vec![object.value],
            Some(Immediate::Data(data)),
            property_ty.clone(),
            Op::PropGet.default_effects(),
            Some(span),
        );
        crate::ir_lower::expr::emit_quiet_property_fetch(ctx, false, span);
        let property_value = own_property_container(ctx, property_value, span);
        ctx.emit_void(
            Op::RuntimeCall,
            vec![property_value.value, index.value, value.value],
            key_already_diagnosed.then_some(Immediate::Bool(true)),
            effects_lookup::runtime_effects(),
            Some(span),
        );
        ctx.emit_void(
            Op::PropSet,
            vec![object.value, property_value.value],
            Some(Immediate::Data(data)),
            Op::PropSet.default_effects(),
            Some(span),
        );
        release_rewritten_property_value_after_retaining_store(
            ctx,
            &property_ty,
            property_value,
            span,
        );
        return;
    }

    // PHP reads a plain-variable index at STORE time, after the right-hand side, so
    // `$o->a[$i] = ($i = 1)` writes index 1. The bare-local write already used this
    // rule; sharing the helper is what keeps the two from answering differently for
    // the same source line.
    runtime_receivers::fallback(ctx, object, property, index, value, span);
}

/// Emits one element write into a typed array or hash property.
///
/// `write` receives the container and emits the set or append. The write is made in place on
/// the property's own container (`fetch_property_container_for_element_write`) whenever
/// nothing between the fetch and the store can run user code.
///
/// One case can: a boxed `mixed` value stored into concrete element storage is converted by the
/// backend inside the write itself, and that conversion can call `__toString` or raise a
/// warning. Either may replace the property, which frees the container the write borrowed.
/// Those writes keep the earlier shape instead: read the property, take a reference of its
/// own, write to that private copy and store it back. It copies the container, but the
/// conversion then runs while the write still owns what it writes to.
fn lower_property_element_write(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    property_ty: &PhpType,
    value: LoweredValue,
    span: Span,
    write: impl FnOnce(&mut LoweringContext<'_, '_>, LoweredValue),
) {
    if !element_store_conversion_may_run_user_code(ctx, property_ty, value) {
        let container =
            fetch_property_container_for_element_write(ctx, object, property, property_ty, span);
        write(ctx, container);
        release_property_array_insert_value_after_retain(ctx, property_ty, value, span);
        return;
    }
    let data = ctx.intern_string(property);
    crate::ir_lower::expr::emit_quiet_property_fetch(ctx, true, span);
    let property_value = ctx.emit_value(
        Op::PropGet,
        vec![object.value],
        Some(Immediate::Data(data)),
        property_ty.clone(),
        Op::PropGet.default_effects(),
        Some(span),
    );
    crate::ir_lower::expr::emit_quiet_property_fetch(ctx, false, span);
    let property_value = own_property_container(ctx, property_value, span);
    write(ctx, property_value);
    release_property_array_insert_value_after_retain(ctx, property_ty, value, span);
    ctx.emit_void(
        Op::PropSet,
        vec![object.value, property_value.value],
        Some(Immediate::Data(data)),
        Op::PropSet.default_effects(),
        Some(span),
    );
    if property_ty.codegen_repr().is_refcounted() {
        crate::ir_lower::ownership::release_if_owned(ctx, property_value, Some(span));
    }
}

/// Returns whether storing `value` as an element of `property_ty` converts it in the backend
/// in a way that can run user code: a boxed value into concrete element storage.
fn element_store_conversion_may_run_user_code(
    ctx: &LoweringContext<'_, '_>,
    property_ty: &PhpType,
    value: LoweredValue,
) -> bool {
    let Some(element_ty) = indexed_property_array_element_type(property_ty) else {
        return false;
    };
    element_ty.codegen_repr() != PhpType::Mixed
        && matches!(
            ctx.builder.value_php_type(value.value).codegen_repr(),
            PhpType::Mixed | PhpType::Union(_)
        )
}

/// Fetches a typed array or hash property as the receiver of one in-place element write.
///
/// `PropGetForWrite` separates the property's container under the `ensure_unique` convention,
/// publishes the unique container into the slot and lends it BORROWED, exactly as
/// `lower_property_array_unset` relies on. The element write then mutates the storage the
/// property owns, and the backend publishes the pointer again only when the write relocated
/// it (an indexed array that grew, or a table a warning handler shared in between).
///
/// The previous shape read the property with `PropGet`, took a reference of its own and
/// stored the result back with `PropSet`. That second reference made every write find a
/// shared container, so each element write copied the whole array or hash and then freed the
/// old one: filling a typed property was quadratic.
///
/// Nothing is stored back here, on purpose. An overwrite releases the replaced element last,
/// and that release can run a destructor which writes or replaces this same property; a
/// `PropSet` of the container fetched earlier would discard what the destructor wrote.
fn fetch_property_container_for_element_write(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    property_ty: &PhpType,
    span: Span,
) -> LoweredValue {
    let data = ctx.intern_string(property);
    // The CONTAINER of an indexed write is fetched quietly, the same as the container of an
    // `isset`/`empty` operand: PHP does not raise for an uninitialized typed property here
    // either. Over an EVAL-OWNED receiver this fetch leaves compiled code through the bridge,
    // which raises unless the mode is on.
    crate::ir_lower::expr::emit_quiet_property_fetch(ctx, true, span);
    let container = ctx.emit_value(
        Op::PropGetForWrite,
        vec![object.value],
        Some(Immediate::Data(data)),
        property_ty.clone(),
        Op::PropGetForWrite.default_effects(),
        Some(span),
    );
    crate::ir_lower::expr::emit_quiet_property_fetch(ctx, false, span);
    ctx.builder.set_value_ownership(container.value, Ownership::Borrowed);
    container
}

/// Lowers `unset($object->prop[$key])` once the receiver and the key have been evaluated.
///
/// PHP evaluates the receiver, then the key, and only then fetches the property for the write,
/// so a key expression that itself mutates the property is seen by the removal. The caller has
/// already lowered both operands in that order; the property is fetched here, last.
///
/// Both storage shapes that reach this function go through `PropGetForWrite`, which splits the
/// property's container under the `ensure_unique` convention, publishes the unique container
/// into the slot, and hands it back BORROWED from the property:
///
/// - A declared PHP `array` property keeps a boxed packed-or-hash cell; `OffsetUnset` promotes
///   the separated cell to hash storage before removing the key, exactly as the declared-array
///   local path does, so surviving keys keep their numbering.
/// - An associative property holds a raw hash; `HashUnset` removes the key from the separated
///   table, which is already unique, so the backend's own split is a no-op.
/// - A boxed `Mixed` property (`property_ty == Mixed`, an untyped slot widened by an earlier
///   `unset($o->prop)` or a `mixed`/`?array` declaration) goes through `OffsetUnset` like the
///   declared-array cell; the backend leaves a null cell alone and refuses a scalar with PHP's
///   `Error`. A removed untyped slot is recreated as null by the fetch, as PHP does.
///
/// No independent PHP array copy is in flight across removal. The backend can retain the same
/// hash in an unwind-visible scope when key conversion can invoke a warning handler.
/// That matters because `__rt_hash_unset`
/// releases the removed value last, and that release can run a destructor which throws or
/// writes the property again. An acquired copy of the property value, as the element WRITE
/// path uses, would leak on the throw (the unwinder cannot see it) and, stored back with
/// `PropSet`, would discard the destructor's write. A `$copy = $o->items` taken earlier still
/// keeps its own container, because the separation happens before the removal.
///
/// A packed indexed `array<T>` property never reaches this function: removing a key leaves a
/// hole, so the storage would have to become a hash, and a property slot's representation is
/// shared by every reader of the class. The caller refuses that shape instead.
///
/// Returns the runtime removal flag, which is set only when the removed value may own a graph
/// that can close a cycle (any value but int, float, bool, string or null).
pub(crate) fn lower_property_array_unset(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    property_ty: &PhpType,
    index: LoweredValue,
    span: Span,
) -> LoweredValue {
    let data = ctx.intern_string(property);
    let (container_ty, remove) = if property_ty.is_php_array() {
        (PhpType::php_array(), Op::OffsetUnset)
    } else if *property_ty == PhpType::Mixed {
        (PhpType::Mixed, Op::OffsetUnset)
    } else {
        (property_ty.clone(), Op::HashUnset)
    };
    let container = ctx.emit_value(
        Op::PropGetForWrite,
        vec![object.value],
        Some(Immediate::Data(data)),
        container_ty,
        Op::PropGetForWrite.default_effects(),
        Some(span),
    );
    ctx.builder.set_value_ownership(container.value, Ownership::Borrowed);
    // The runtime flag reports a removed value that may own a cycle-capable graph.
    ctx.emit_value(
        remove,
        vec![container.value, index.value],
        None,
        PhpType::Bool,
        remove.default_effects(),
        Some(span),
    )
}

/// Separates a declared PHP array property before mutating its packed-or-hash boxed payload.
/// `PropGetForWrite` publishes the detached cell and returns a borrow owned by the property.
/// `key_already_diagnosed` lets the boxed writer rebuild a float key its read already reported.
///
/// The caller evaluated the key and the value already. PHP does that BEFORE it fetches the
/// property for write (`zend_delayed_compile_dim` delays the container fetch past the right-hand
/// side): a right-hand side that writes the same property -- `$this->types[$n] =
/// $this->resolve($n)` where `resolve()` recursively fills `$this->types`, as Symfony's
/// `FormRegistry::getType()` does -- must see its writes kept.
fn lower_php_array_property_write(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    index: Option<LoweredValue>,
    value: LoweredValue,
    span: Span,
    key_already_diagnosed: bool,
) {
    // php auto-initializes an UNINITIALIZED typed `array` property on an element write or
    // append (`public array $a; $o->a['k'] = 1;`, `$this->items[$k] ??= ...`). Fetching it for
    // write first raised "must not be accessed before initialization" instead.
    if is_concrete_object_receiver(ctx, object.value) {
        if let Some(property_ty) = object_property_type(ctx, object.value, property) {
            initialize_uninitialized_array_property_for_write(
                ctx,
                object.value,
                property,
                &property_ty,
                true,
                span,
            );
        }
    }
    let data = ctx.intern_string(property);
    let array = ctx.emit_value(
        Op::PropGetForWrite,
        vec![object.value],
        Some(Immediate::Data(data)),
        PhpType::php_array(),
        Op::PropGetForWrite.default_effects(),
        Some(span),
    );
    ctx.builder.set_value_ownership(array.value, Ownership::Borrowed);
    if let Some(index) = index {
        ctx.emit_void(
            Op::RuntimeCall,
            vec![array.value, index.value, value.value],
            key_already_diagnosed.then_some(Immediate::Bool(true)),
            effects_lookup::runtime_effects(),
            Some(span),
        );
        release_persisted_string_operand(ctx, index, span);
    } else {
        ctx.emit_void(
            Op::MixedArrayAppend,
            vec![array.value, value.value],
            None,
            Op::MixedArrayAppend.default_effects(),
            Some(span),
        );
        // The append boxes a call's string by TAKING it over (`box_value_for_mixed_container`),
        // so releasing it here freed what the array now holds: `$this->paths[] =
        // rtrim($path, '/\\')` in Twig's `FilesystemLoader::addPath()` stored freed bytes, and
        // no template was ever found. Only a string the box copied is still this write's.
        if ctx.builder.value_php_type(value.value).codegen_repr() == PhpType::Str {
            release_string_copied_into_box(ctx, value, span);
            return;
        }
    }
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
}

/// Returns whether a declared array-like property is represented by one boxed Mixed cell.
fn property_type_uses_mixed_array_storage(property_ty: &PhpType) -> bool {
    match property_ty.codegen_repr() {
        PhpType::Mixed => true,
        PhpType::Union(members) => {
            let mut saw_array = false;
            for member in members {
                match member.codegen_repr() {
                    PhpType::Array(_) | PhpType::AssocArray { .. } => saw_array = true,
                    PhpType::Void | PhpType::Never => {}
                    _ => return false,
                }
            }
            saw_array
        }
        _ => false,
    }
}

/// Mutates an erased array-or-Traversable property and publishes any COW replacement.
fn lower_iterable_property_write(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    index: Option<LoweredValue>,
    value: LoweredValue,
    span: Span,
    key_already_diagnosed: bool,
) {
    let data = ctx.intern_string(property);
    let current = ctx.emit_owned_value(
        Op::PropGet,
        vec![object.value],
        Some(Immediate::Data(data)),
        PhpType::Iterable,
        Op::PropGet.default_effects(),
        Some(span),
    );
    let boxed = ctx.box_value_as_mixed(current, PhpType::Mixed, Some(span));
    let (boxed, owner) = crate::ir_lower::expr::root_owned_call_operand(ctx, boxed, span);
    let borrowed = ctx.emit_value(
        Op::Borrow,
        vec![boxed.value],
        None,
        PhpType::Mixed,
        Op::Borrow.default_effects(),
        Some(span),
    );
    ctx.builder
        .set_value_ownership(borrowed.value, Ownership::Borrowed);
    if let Some(index) = index {
        string_offset_writes::lower_boxed_set(
            ctx,
            borrowed,
            index,
            value,
            span,
            key_already_diagnosed,
        );
    } else if !native_dimension_append::lower(ctx, borrowed, value, span, |ctx, array, value| {
        ctx.emit_void(
            Op::MixedArrayAppend,
            vec![array.value, value.value],
            None,
            Op::MixedArrayAppend.default_effects(),
            Some(span),
        );
    }) {
        ctx.emit_void(
            Op::MixedArrayAppend,
            vec![borrowed.value, value.value],
            None,
            Op::MixedArrayAppend.default_effects(),
            Some(span),
        );
    }
    ctx.emit_void(
        Op::PropSet,
        vec![object.value, borrowed.value],
        Some(Immediate::Data(data)),
        Op::PropSet.default_effects(),
        Some(span),
    );
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
    if let Some(owner) = owner {
        crate::ir_lower::expr::retire_owned_call_operand(ctx, owner, span);
    }
}

/// Autovivifies an uninitialized declared array property before mutating one of its elements.
pub(super) fn initialize_uninitialized_array_property_for_write(
    ctx: &mut LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
    property_ty: &PhpType,
    initialize_uninitialized: bool,
    span: Span,
) {
    if !initialize_uninitialized {
        return;
    }
    // The slot's REPRESENTATION decides how the empty container is stored, and a nullable
    // array property does not represent as an array: `?array` is a union, and every union
    // reaches codegen as `Mixed`. Keying only on the representation therefore skipped
    // auto-initialization entirely for `?array $p`, and `$o->p['k'] = 1` raised where PHP
    // initializes — measured against `php -n` 8.5.6, which treats `array` and `?array`
    // identically here. The DECLARED type says which container to make; the representation
    // says whether it has to be boxed on the way into the slot.
    // The lowering type of `?array` is already `Mixed`; the declared union still names the
    // array member to create.
    let Some(container_ty) = auto_initialized_container_type(property_ty)
        .or_else(|| {
            declared_object_property_type(ctx, object, property)
                .and_then(|declared| auto_initialized_container_type(&declared))
        })
        .or_else(|| typed_boxed_property_container(ctx, object, property, property_ty))
    else {
        return;
    };
    let op = match container_ty.codegen_repr() {
        PhpType::Array(_) => Op::ArrayNew,
        PhpType::AssocArray { .. } => Op::HashNew,
        _ => return,
    };
    let slot_is_boxed = matches!(property_ty.codegen_repr(), PhpType::Mixed);
    let data = ctx.intern_string(property);
    let initialized = ctx.emit_value(
        Op::PropInitialized,
        vec![object],
        Some(Immediate::Data(data)),
        PhpType::Bool,
        Op::PropInitialized.default_effects(),
        Some(span),
    );
    let initialize = ctx
        .builder
        .create_named_block("property_array.initialize", Vec::new());
    let ready = ctx
        .builder
        .create_named_block("property_array.ready", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: initialized.value,
        then_target: ready,
        then_args: Vec::new(),
        else_target: initialize,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(initialize);
    let empty = ctx.emit_value(
        op,
        Vec::new(),
        Some(Immediate::Capacity(0)),
        container_ty.clone(),
        op.default_effects(),
        Some(span),
    );
    // A raw array word stored into a `Mixed` slot is the representation mismatch this codebase
    // keeps rediscovering, so a boxed slot gets a boxed value.
    let (empty, stored_ty) = if slot_is_boxed {
        (
            ctx.box_value_as_mixed(empty, PhpType::Mixed, Some(span)),
            PhpType::Mixed,
        )
    } else {
        (empty, container_ty.clone())
    };
    ctx.emit_void(
        Op::PropSet,
        vec![object, empty.value],
        Some(Immediate::Data(data)),
        Op::PropSet.default_effects(),
        Some(span),
    );
    release_property_assignment_source_after_retaining_store(ctx, &stored_ty, empty, span);
    branch_to(ctx, ready);

    ctx.builder.position_at_end(ready);
}

/// The container an element write creates in a typed property stored as a boxed Mixed.
///
/// The slot's type no longer names an array here: `mixed` never did, and a declared `array` the
/// checker widened to `Mixed` lost it. Either way a typed property can sit uninitialized, and php
/// creates the array on an element write, as `ClassMetadata::initializeReflection()` relies on with
/// `$this->table['name'] = ...`. An untyped property starts as null and needs none of this.
fn typed_boxed_property_container(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
    property_ty: &PhpType,
) -> Option<PhpType> {
    if property_ty.codegen_repr() != PhpType::Mixed {
        return None;
    }
    let PhpType::Object(class_name) = ctx.builder.value_php_type(object).codegen_repr() else {
        return None;
    };
    let info = ctx.classes.get(class_name.trim_start_matches('\\'))?;
    // `?ArrayObject` and `?int` are boxed too but name no array member: php refuses to
    // auto-initialize them, and the declared-slot guard throws that error.
    fn admits_array(ty: &PhpType) -> bool {
        match ty {
            PhpType::Mixed | PhpType::Iterable | PhpType::Array(_) | PhpType::AssocArray { .. } => true,
            PhpType::Union(members) => members.iter().any(admits_array),
            _ => false,
        }
    }
    if info.visible_property(property).is_some_and(|(_, (_, declared))| !admits_array(declared)) {
        return None;
    }
    info.declared_properties
        .contains(property)
        .then(|| PhpType::Array(Box::new(PhpType::Mixed)))
}

/// Returns the container type PHP auto-initializes an indexed property write into.
///
/// An `array`/`array<T>` slot makes that array. A UNION containing one — `?array`,
/// `array|null`, `array|string` — makes the array member, because PHP auto-initializes on the
/// array side of the union rather than refusing. A union with no array member, and every
/// non-array type, has no auto-initialization: PHP raises
/// `Cannot auto-initialize an array inside property C::$p of type int` there instead.
fn auto_initialized_container_type(property_ty: &PhpType) -> Option<PhpType> {
    match property_ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } => Some(property_ty.clone()),
        PhpType::Union(members) => members
            .iter()
            .find(|member| matches!(member, PhpType::Array(_) | PhpType::AssocArray { .. }))
            .cloned(),
        _ => match property_ty.codegen_repr() {
            PhpType::Array(_) | PhpType::AssocArray { .. } => Some(property_ty.codegen_repr()),
            _ => None,
        },
    }
}

/// Returns whether an EIR receiver needs the read/modify/write-back element write.
///
/// PHP's bare `object` pseudo-type is the original case, and a BOXED GRADUAL receiver — an
/// untyped parameter holding an object — needs exactly the same treatment: the compiler cannot
/// name the class, so it cannot address the property's slot and mutate the array in place.
///
/// Falling through instead reached the final `RuntimeCall` fallback, whose
/// `__rt_mixed_property_get` only understands stdClass and answers null for every other class.
/// `__rt_mixed_array_set` then wrote into that null and the element was SILENTLY DISCARDED, while
/// a scalar write to the same property through the same receiver worked — that one already went
/// through the declared-property class-id ladder. Symfony's generated container is written
/// entirely against this shape (`getXService($container, $lazyLoad = true)` storing into
/// `$container->services[$id]`), so every service it built was rebuilt on the next `get()`.
pub(in crate::ir_lower) fn is_generic_object_receiver(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
) -> bool {
    match ctx.builder.value_php_type(object) {
        PhpType::Object(class_name) if class_name.trim_start_matches('\\').is_empty() => {
            return true
        }
        PhpType::Mixed | PhpType::Union(_) => return true,
        PhpType::Object(class_name)
            if property_declared_only_by_descendants(
                ctx, class_name.trim_start_matches('\\'), property,
            ) =>
        {
            return true
        }
        _ => {}
    }
    receiver_stores_property_dynamically(ctx, object, property)
}

/// Whether `class_name` does not hold `property` itself (no slot, no `__set`, no dynamic hash):
/// only a descendant can, so the element write resolves the property on the run-time object, as
/// through an interface. Reachability may have pruned that descendant from `ctx.classes` after
/// the checker typed the access from it, so no live descendant is required: a Symfony Demo never
/// builds a `CurlClientState`, whose `$pauseExpiries` `CurlResponse::select()` still reaches.
pub(in crate::ir_lower) fn property_declared_only_by_descendants(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    property: &str,
) -> bool {
    ctx.classes.get(class_name).is_some_and(|info| {
        info.visible_property(property).is_none()
            && !info.methods.contains_key("__set")
            && !info.dynamic_property_hash_is_name_addressable()
    })
}

/// Returns whether this receiver keeps `property` in its dynamic side table rather than a slot.
///
/// A stdClass has no declared properties at all — `(object) ['vars' => []]` gives one whose keys
/// the compiler never sees — and a `#[\AllowDynamicProperties]` class keeps anything it did not
/// declare the same way. Both read and write that storage exactly as PHP's bare `object` type
/// does, so an element write through one takes the same Mixed read/modify/write the generic
/// receiver takes. Without this the lowering fell through to a `RuntimeCall` the backend has no
/// arm for, and `$state->vars[] = $m[1]` in Symfony's `CompiledUrlMatcherDumper` reached it.
fn receiver_stores_property_dynamically(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
) -> bool {
    let PhpType::Object(class_name) = ctx.builder.value_php_type(object) else {
        return false;
    };
    let class_name = class_name.trim_start_matches('\\');
    if crate::types::checker::builtin_stdclass::is_stdclass(class_name) {
        return true;
    }
    ctx.classes.get(class_name).is_some_and(|class_info| {
        // Any per-instance hash: `#[AllowDynamicProperties]`, or the one this program reserves
        // for a name it creates on an ordinary class (php 8.2+'s deprecated dynamic property).
        class_info.dynamic_property_hash_is_name_addressable()
            && class_info.visible_property(property).is_none()
    })
}

/// Returns whether the receiver names one concrete class with native property slots.
pub(super) fn is_concrete_object_receiver(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
) -> bool {
    matches!(
        ctx.builder.value_php_type(object).codegen_repr(),
        PhpType::Object(class_name) if !class_name.trim_start_matches('\\').is_empty()
    )
}

/// Releases a temporary assigned into an object property after `PropSet` retains or boxes it.
pub(in crate::ir_lower) fn release_property_assignment_source_after_retaining_store(
    ctx: &mut LoweringContext<'_, '_>,
    property_ty: &PhpType,
    value: LoweredValue,
    span: Span,
) {
    if !ctx.value_is_owning_temporary(value) {
        return;
    }
    if !property_store_keeps_independent_ref(property_ty, &ctx.builder.value_php_type(value.value))
    {
        return;
    }
    crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
}

/// Releases an element temporary after a property-array write retains it for storage.
pub(super) fn release_property_array_insert_value_after_retain(
    ctx: &mut LoweringContext<'_, '_>,
    property_ty: &PhpType,
    value: LoweredValue,
    span: Span,
) {
    if !property_array_insert_retains_value(ctx, property_ty, value) {
        return;
    }
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
}

/// Returns true when the backend insert helper takes its own reference to the element value.
///
/// Three disciplines meet here, and only the first needs a release:
///
/// - **Indexed storage** (`__rt_array_set_*`, `__rt_array_push_*`): the value is incref'd into
///   the slot unconditionally, so an owning temporary keeps a reference nobody consumes.
/// - **Mixed-element indexed storage from a concrete value**: codegen boxes the value first,
///   and the boxing path releases the box it made, so releasing here would double-free.
/// - **Assoc storage** (`__rt_hash_set`): the value is only incref'd when it cannot hand over
///   its own reference (`value_can_own_mixed_box_source`), so an owning temporary is moved in
///   and the store consumes it.
///
/// A Mixed-element *indexed* array fed an already-boxed Mixed value therefore falls in the
/// first group, not the second: nothing boxes, the incref still fires, and without the release
/// every `$o->items[0] += 1` / `$o->items[] = f()` leaks one Mixed cell (issue #1041).
fn property_array_insert_retains_value(
    ctx: &LoweringContext<'_, '_>,
    property_ty: &PhpType,
    value: LoweredValue,
) -> bool {
    let Some(elem_ty) = indexed_property_array_element_type(property_ty) else {
        return false;
    };
    if !matches!(elem_ty.codegen_repr(), PhpType::Mixed | PhpType::Callable) {
        return true;
    }
    if !matches!(property_ty.codegen_repr(), PhpType::Array(_)) {
        return false;
    }
    matches!(
        ctx.builder.value_php_type(value.value).codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    )
}

/// Releases the loaded property value after rewriting it through a retaining `PropSet`.
pub(in crate::ir_lower) fn release_rewritten_property_value_after_retaining_store(
    ctx: &mut LoweringContext<'_, '_>,
    property_ty: &PhpType,
    property_value: LoweredValue,
    span: Span,
) {
    if property_ty.codegen_repr().is_refcounted() {
        crate::ir_lower::ownership::release_if_owned(ctx, property_value, Some(span));
    }
}

/// Returns whether a property store creates a distinct retained/boxed owner for the value.
pub(super) fn property_store_keeps_independent_ref(property_ty: &PhpType, value_ty: &PhpType) -> bool {
    let property_ty = property_ty.codegen_repr();
    let value_ty = value_ty.codegen_repr();
    // Mixed slots retain when EIR owns source cleanup, including a release on a readonly
    // error sibling. Keep a normal-path release and an unwind pin instead of assuming adoption.
    if matches!(value_ty, PhpType::Mixed | PhpType::Union(_))
        && matches!(property_ty, PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::TaggedScalar)
    {
        return true;
    }
    // Callable slots retain through the descriptor ABI, outside is_refcounted().
    if matches!(property_ty, PhpType::Str | PhpType::Callable) {
        return true;
    }
    property_ty.is_refcounted()
}

/// Returns the element type for property arrays that use retaining indexed/hash helpers.
pub(super) fn indexed_property_array_element_type(property_ty: &PhpType) -> Option<PhpType> {
    match property_ty.codegen_repr() {
        PhpType::Array(elem_ty) => Some(elem_ty.codegen_repr()),
        PhpType::AssocArray { value, .. } => Some(value.codegen_repr()),
        _ => None,
    }
}
