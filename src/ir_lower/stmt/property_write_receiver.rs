//! Purpose:
//! Keeps property-write receivers alive and visible to same-frame exception cleanup.
//!
//! Called from:
//! - Literal, runtime-name and array-element property write lowering.
//!
//! Key details:
//! - Static loads borrow either a concrete object or a boxed nullable value.
//! - Nullable receivers are guarded before extracting an owned object payload.
//! - Direct writes guard only after RHS evaluation, with its owner visible to unwind.

use super::*;

/// One receiver lease covering RHS evaluation, coercion and the eventual property mutation.
pub(crate) struct PropertyWriteReceiver {
    pub(crate) value: LoweredValue,
    nullable_class: Option<String>,
    pins: Vec<crate::ir_lower::expr::PinnedInFlightOwner>,
    owners: Vec<LoweredValue>,
}

impl PropertyWriteReceiver {
    /// Acquires borrowed static storage and parks any independently owned receiver for unwind.
    pub(crate) fn new(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span) -> Self {
        let nullable_class = nullable_write_class(ctx, value);
        ctx.with_independent_write_receiver(|ctx| {
            let value = if ctx.builder.value_defining_op(value.value) == Some(Op::LoadStaticProperty) {
                crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(ctx, value, Some(span))
            } else { value };
            let pins = crate::ir_lower::expr::pin_in_flight_owners(ctx, &[value.value], span);
            let owners = if ctx.value_is_owning_temporary(value) { vec![value] } else { Vec::new() };
            Self { value, nullable_class, pins, owners }
        })
    }

    /// Guards an array mutation after its keys and RHS have run, rooting their owners on Error.
    pub(super) fn narrow_for_array(
        &mut self, ctx: &mut LoweringContext<'_, '_>, property: &str, operands: &[crate::ir::ValueId], span: Span,
    ) {
        self.narrow_for_write(ctx, WritePropertyName::Literal(property), "modify", operands, span);
    }

    /// Guards a direct write after its RHS runs and retires that RHS if the null Error unwinds.
    pub(crate) fn narrow_for_assignment(
        &mut self, ctx: &mut LoweringContext<'_, '_>, property: &str, value: LoweredValue, span: Span,
    ) {
        self.narrow_for_write(ctx, WritePropertyName::Literal(property), "assign", &[value.value], span);
    }

    /// Narrows a runtime-name write after its name and RHS are evaluated exactly once.
    pub(crate) fn narrow_for_dynamic_assignment(
        &mut self, ctx: &mut LoweringContext<'_, '_>, property: LoweredValue, value: LoweredValue, span: Span,
    ) {
        self.narrow_for_write(ctx, WritePropertyName::Runtime(property), "assign", &[property.value, value.value], span);
    }

    /// Keeps write operands above the receiver on the LIFO unwind stack only over the null guard.
    fn narrow_for_write(
        &mut self, ctx: &mut LoweringContext<'_, '_>, property: WritePropertyName<'_>, verb: &str,
        operands: &[crate::ir::ValueId], span: Span,
    ) {
        let Some(class) = self.nullable_class.clone() else { return; };
        let pins = crate::ir_lower::expr::pin_in_flight_owners(ctx, operands, span);
        let object = ctx.with_independent_write_receiver(|ctx|
            narrow_nullable_write_receiver(ctx, self.value, &class, property, verb, span));
        crate::ir_lower::expr::unpin_in_flight_owners(ctx, pins, span);
        // The runtime unwind stack is LIFO. Detach the RHS record before publishing the
        // narrowed object's lease, which must outlive all subsequent coercion/store work.
        self.adopt_narrowed(ctx, object, span);
    }

    /// Retains the narrowed object view while preserving the boxed receiver's unwind lease.
    fn adopt_narrowed(
        &mut self, ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, span: Span,
    ) {
        if object.value != self.value.value {
            self.pins.extend(ctx.with_independent_write_receiver(|ctx|
                crate::ir_lower::expr::pin_in_flight_owners(ctx, &[object.value], span)));
            self.owners.push(object);
            self.value = object;
        }
    }

    /// Retires the unwind record and releases precisely the receiver's independent lease.
    pub(crate) fn finish(self, ctx: &mut LoweringContext<'_, '_>, span: Span) {
        crate::ir_lower::expr::unpin_in_flight_owners(ctx, self.pins, span);
        for value in self.owners.into_iter().rev() {
            crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
        }
    }
}

/// Supplies a literal or already evaluated runtime property name to the null Error branch.
enum WritePropertyName<'a> {
    Literal(&'a str),
    Runtime(LoweredValue),
}

/// Recovers nullable interface metadata only for writes, leaving ordinary reads boxed.
fn nullable_write_class(ctx: &LoweringContext<'_, '_>, value: LoweredValue) -> Option<String> {
    let ty = ctx.builder.value_php_type(value.value);
    if let Some((class, true)) = crate::ir_lower::expr::singular_object_class(&ty) {
        return Some(class.to_string());
    }
    let load = ctx.builder.value_defining_instruction(value.value)?;
    if load.op != Op::LoadStaticProperty { return None; }
    let Some(Immediate::Data(data)) = load.immediate else { return None; };
    let (class, property) = ctx.data.strings.get(data.as_raw() as usize)?.rsplit_once("::")?;
    let receiver = match class {
        "self" => StaticReceiver::Self_, "static" => StaticReceiver::Static,
        "parent" => StaticReceiver::Parent,
        name => StaticReceiver::Named(crate::names::Name::unqualified(name)),
    };
    let class = static_receiver_class_name(ctx, &receiver)?;
    let (_, ty) = ctx.classes.get(&class)?.static_properties.iter().find(|(name, _)| name == property)?;
    let (interface, nullable) = crate::ir_lower::expr::singular_object_class(ty)?;
    (nullable && ctx.interfaces.contains_key(interface.trim_start_matches('\\')))
        .then(|| interface.to_string())
}

/// Narrows a known nullable class for fixed-slot mutation, with PHP's operation-specific null Error.
fn narrow_nullable_write_receiver(
    ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, class: &str,
    property: WritePropertyName<'_>, verb: &str, span: Span,
) -> LoweredValue {
    let is_null = ctx.emit_value(Op::IsNull, vec![value.value], None, PhpType::Bool,
        Op::IsNull.default_effects(), Some(span));
    let null = ctx.builder.create_named_block("property.write.null", Vec::new());
    let present = ctx.builder.create_named_block("property.write.present", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value, then_target: null, then_args: Vec::new(),
        else_target: present, else_args: Vec::new(),
    });
    ctx.builder.position_at_end(null);
    match property {
        WritePropertyName::Literal(property) =>
            lower_throw_access_error(ctx, &format!("Attempt to {verb} property \"{property}\" on null"), span),
        WritePropertyName::Runtime(property) => lower_dynamic_null_error(ctx, property, verb, span),
    }
    ctx.builder.position_at_end(present);
    if ctx.interfaces.contains_key(class.trim_start_matches('\\')) {
        let boxed = ctx.emit_value(Op::Borrow, vec![value.value], None, PhpType::Mixed,
            Op::Borrow.default_effects(), Some(span));
        ctx.builder.set_value_ownership(boxed.value, Ownership::Borrowed);
        return boxed;
    }
    let target = PhpType::Object(class.to_string());
    ctx.emit_owned_value(Op::MixedUnbox, vec![value.value], None, target.clone(),
        Op::mixed_unbox_effects(&target), Some(span))
}

/// Builds the runtime-name Error through ordinary constructor lowering without replaying the name.
fn lower_dynamic_null_error(
    ctx: &mut LoweringContext<'_, '_>, property: LoweredValue, verb: &str, span: Span,
) {
    // The surrounding in-flight pin owns the name. This scratch alias must not create a
    // second consuming read when ordinary concat lowering uses it in the Error message.
    let temp = ctx.declare_hidden_temp(PhpType::Str);
    let name = ctx.emit_value(Op::Borrow, vec![property.value], None, PhpType::Str,
        Op::Borrow.default_effects(), Some(span));
    ctx.builder.set_value_ownership(name.value, Ownership::Borrowed);
    ctx.store_local(&temp, name, PhpType::Str, Some(span));
    let concat = |left, right| Expr::new(ExprKind::BinaryOp {
        left: Box::new(left), op: crate::parser::ast::BinOp::Concat, right: Box::new(right),
    }, span);
    let message = concat(concat(
        Expr::new(ExprKind::StringLiteral(format!("Attempt to {verb} property \"")), span),
        Expr::new(ExprKind::Variable(temp.clone()), span),
    ), Expr::new(ExprKind::StringLiteral("\" on null".to_string()), span));
    let error = lower_expr(ctx, &Expr::new(ExprKind::NewObject {
        class_name: crate::names::Name::unqualified("Error"), args: vec![message],
    }, span));
    ctx.emit_void(Op::UnsetLocal, Vec::new(), Some(Immediate::LocalSlot(ctx.local_slots[&temp])),
        Op::UnsetLocal.default_effects(), Some(span));
    terminate_throw(ctx, error.value);
}
