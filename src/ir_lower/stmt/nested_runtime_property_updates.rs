//! Purpose:
//! Routes captured runtime property names through ordinary declared-property update kernels.
//!
//! Called from:
//! - Nested indexed updates after receiver, name, keys and compound RHS capture.
//!
//! Key details:
//! - A matching declared name keeps typed slot writeback, COW and visibility checks.
//! - Native XML and unmatched names retain the shared retained-parent update path.
//! - Captured operands are borrowed across the name ladder and evaluated only once.

use super::*;

/// Reuses named property writers when the captured runtime name identifies a declared slot.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, target: &Expr, value: &Expr, span: Span,
) -> bool {
    let Some((object, property)) = super::nested_array_writes::runtime_property_root(target) else {
        return false;
    };
    // A name ladder can reload a captured local. Deferred storage chains keep their own
    // once-only traversal in the retained-parent kernel instead of replaying accessors here.
    let ExprKind::Variable(receiver_name) = &object.kind else { return false; };
    let ty = ctx.local_type(receiver_name);
    if !matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_) | PhpType::Object(_)) {
        return false;
    }
    let bound = crate::ir_lower::expr::singular_object_class(&ty)
        .map(|(class, _)| class.to_string())
        .filter(|class| !class.is_empty() && !ctx.interfaces.contains_key(class));
    if bound.as_ref().is_some_and(|class|
        crate::types::builtin_classes::has_native_dimensions(class, ctx.classes)) {
        return false;
    }
    let mut names = ctx.classes.iter().filter(|(class, _)|
        bound.as_ref().is_none_or(|base| *class == base
            || crate::types::class_inherits_from(ctx.classes, class, base)))
        .flat_map(|(_, info)| info.properties.iter()
            .filter(|(_, ty)| container_slot(ctx, ty)).map(|(name, _)| name.clone()))
        .collect::<Vec<_>>();
    names.sort(); names.dedup();
    if names.is_empty() { return false; }
    let name = lower_expr(ctx, property);
    let done = ctx.builder.create_named_block("property.update.name.done", Vec::new());
    let initial = ctx.initialized_slots_snapshot();
    let mut merged: Option<HashSet<LocalSlotId>> = None;
    if bound.is_none() && ctx.classes.contains_key("SimpleXMLElement") {
        // Native properties are XML names, including spellings shared with ordinary slots.
        let receiver = lower_expr(ctx, object);
        let class = ctx.intern_class_name("SimpleXMLElement");
        let native = ctx.emit_value(Op::InstanceOf, vec![receiver.value], Some(Immediate::Data(class)),
            PhpType::Bool, Op::InstanceOf.default_effects(), Some(span));
        let xml = ctx.builder.create_named_block("property.update.name.xml", Vec::new());
        let ordinary = ctx.builder.create_named_block("property.update.name.ordinary", Vec::new());
        ctx.builder.terminate(Terminator::CondBr { cond: native.value,
            then_target: xml, then_args: Vec::new(), else_target: ordinary, else_args: Vec::new() });
        ctx.builder.position_at_end(xml);
        super::nested_array_writes::lower_captured_update(ctx, target, value, span);
        join(ctx, done, &mut merged);
        ctx.builder.position_at_end(ordinary);
        ctx.restore_initialized_slots(initial.clone());
    }
    for property in names {
        let data = ctx.intern_string(&property);
        let expected = ctx.emit_value(Op::ConstStr, Vec::new(), Some(Immediate::Data(data)),
            PhpType::Str, Op::ConstStr.default_effects(), Some(span));
        let equal = ctx.emit_value(Op::StrictEq, vec![name.value, expected.value], None,
            PhpType::Bool, Op::StrictEq.default_effects(), Some(span));
        let selected = ctx.builder.create_named_block("property.update.name.slot", Vec::new());
        let next = ctx.builder.create_named_block("property.update.name.next", Vec::new());
        ctx.builder.terminate(Terminator::CondBr { cond: equal.value,
            then_target: selected, then_args: Vec::new(), else_target: next, else_args: Vec::new() });
        ctx.builder.position_at_end(selected);
        let target = named_root(target, &property);
        let value = with_read(value, &target);
        let ExprKind::ArrayAccess { array, index } = &target.kind else { unreachable!() };
        if let ExprKind::PropertyAccess { object, property } = &array.kind {
            super::property_array_writes::lower_property_array_assign(
                ctx, object, property, index, &value, span,
            );
        } else {
            super::nested_array_writes::lower_nested_array_assign(ctx, &target, &value, span);
        }
        join(ctx, done, &mut merged);
        ctx.builder.position_at_end(next);
        ctx.restore_initialized_slots(initial.clone());
    }
    super::nested_array_writes::lower_captured_update(ctx, target, value, span);
    join(ctx, done, &mut merged);
    ctx.builder.position_at_end(done);
    ctx.restore_initialized_slots(merged.unwrap_or(initial));
    true
}

/// Limits named writer branches to declarations its existing container kernels can lower.
fn container_slot(ctx: &LoweringContext<'_, '_>, ty: &PhpType) -> bool {
    if let PhpType::Union(members) = ty {
        return members.iter().any(|member| container_slot(ctx, member));
    }
    matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Array(_) | PhpType::AssocArray { .. }
        | PhpType::Iterable | PhpType::Buffer(_))
        || type_satisfies_array_access_for_ir(ctx, ty)
        || matches!(ty.codegen_repr(), PhpType::Object(class)
            if crate::types::builtin_classes::has_native_dimensions(&class, ctx.classes))
}

/// Replaces only the canonical self-read, preserving captured RHS values and expression results.
pub(super) fn with_read(value: &Expr, read: &Expr) -> Expr {
    let mut value = value.clone();
    match &mut value.kind {
        ExprKind::BinaryOp { left, .. } => *left = Box::new(read.clone()),
        ExprKind::Assignment { value, prelude, .. } if prelude.is_empty() => {
            **value = with_read(value, read);
        }
        ExprKind::Assignment { prelude, .. } => {
            let StmtKind::Assign { value, .. } = &mut prelude[0].kind else {
                unreachable!("classified incdec stores its initial self-read");
            };
            *value = read.clone();
        }
        _ => unreachable!("classified updates have a canonical self-read"),
    }
    value
}

/// Replaces one stabilized dynamic property root without changing captured dimensions.
fn named_root(target: &Expr, property: &str) -> Expr {
    let kind = match &target.kind {
        ExprKind::ArrayAccess { array, index } => ExprKind::ArrayAccess {
            array: Box::new(named_root(array, property)), index: index.clone(),
        },
        ExprKind::DynamicPropertyAccess { object, .. } => ExprKind::PropertyAccess {
            object: object.clone(), property: property.into(),
        },
        _ => unreachable!("the runtime-name update has one property root"),
    };
    Expr::new(kind, target.span)
}

/// Joins live update arms after their own writer has retired every temporary owner.
fn join(
    ctx: &mut LoweringContext<'_, '_>, done: BlockId,
    merged: &mut Option<HashSet<LocalSlotId>>,
) {
    if ctx.builder.insertion_block_is_terminated() { return; }
    let slots = ctx.initialized_slots_snapshot();
    *merged = Some(merged.take().map_or_else(|| slots.clone(), |previous|
        previous.intersection(&slots).copied().collect()));
    ctx.builder.terminate(Terminator::Br { target: done, args: Vec::new() });
}
