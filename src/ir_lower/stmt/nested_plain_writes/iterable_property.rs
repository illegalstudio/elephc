//! Purpose:
//! Keeps nested writes attached to declared `iterable` property storage.
//!
//! Called from:
//! - Plain nested indexed stores and appends after their source operands are captured.
//!
//! Key details:
//! - Exact runtime-class dispatch isolates raw iterable slots from other same-name properties.
//! - A boxed temporary carries COW relocation until the updated payload is stored back.
//! - Receiver, class-name, and temporary-cell owners remain unwind-visible.

use std::collections::HashSet;

use super::*;

/// One boxed property value that must be republished after its nested write.
struct Binding {
    receiver: LoweredValue,
    property: String,
    temp: String,
    owner: LocalSlotId,
}

/// Dispatches named property roots whose exact runtime declaration uses `iterable` storage.
pub(super) fn dispatch(
    ctx: &mut LoweringContext<'_, '_>,
    target: &Expr,
    span: Span,
    mut write: impl FnMut(&mut LoweringContext<'_, '_>, &Expr),
) -> bool {
    let Some((object, property)) = named_property_root(target) else {
        return false;
    };
    let mut classes = iterable_property_classes(ctx, property);
    if classes.is_empty() {
        return false;
    }
    classes.sort();

    let receiver = lower_expr(ctx, object);
    let object_value = ctx.emit_value(
        Op::TypePredicate,
        vec![receiver.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Object)),
        PhpType::Bool,
        Op::TypePredicate.default_effects(),
        Some(span),
    );
    let inspect = ctx
        .builder
        .create_named_block("iterable.property.object", Vec::new());
    let fallback = ctx
        .builder
        .create_named_block("iterable.property.fallback", Vec::new());
    let done = ctx
        .builder
        .create_named_block("iterable.property.done", Vec::new());
    let initial = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr {
        cond: object_value.value,
        then_target: inspect,
        then_args: Vec::new(),
        else_target: fallback,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(inspect);
    let target_fn = crate::ir::RuntimeFnId::GetClass;
    let runtime_class = ctx.emit_owned_value(
        Op::RuntimeCall,
        vec![receiver.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target_fn))),
        PhpType::Str,
        target_fn.effects(),
        Some(span),
    );
    let (runtime_class, class_owner) =
        crate::ir_lower::expr::root_owned_call_operand(ctx, runtime_class, span);
    let runtime_class_ty = ctx.builder.value_php_type(runtime_class.value);
    let runtime_class = ctx.emit_value(
        Op::Borrow,
        vec![runtime_class.value],
        None,
        runtime_class_ty,
        Op::Borrow.default_effects(),
        Some(span),
    );
    ctx.builder
        .set_value_ownership(runtime_class.value, Ownership::Borrowed);

    let mut merged: Option<HashSet<LocalSlotId>> = None;
    for class in classes {
        let matched = ctx
            .builder
            .create_named_block("iterable.property.class", Vec::new());
        let next = ctx
            .builder
            .create_named_block("iterable.property.next", Vec::new());
        let data = ctx.intern_string(class.trim_start_matches('\\'));
        let expected = ctx.emit_value(
            Op::ConstStr,
            Vec::new(),
            Some(Immediate::Data(data)),
            PhpType::Str,
            Op::ConstStr.default_effects(),
            Some(span),
        );
        let equal = ctx.emit_value(
            Op::StrictEq,
            vec![runtime_class.value, expected.value],
            None,
            PhpType::Bool,
            Op::StrictEq.default_effects(),
            Some(span),
        );
        let before = ctx.initialized_slots_snapshot();
        ctx.builder.terminate(Terminator::CondBr {
            cond: equal.value,
            then_target: matched,
            then_args: Vec::new(),
            else_target: next,
            else_args: Vec::new(),
        });

        ctx.builder.position_at_end(matched);
        retire_optional_owner(ctx, class_owner, span);
        let concrete = borrow_concrete_receiver(ctx, receiver, &class, span);
        let (rewritten, binding) = bind(ctx, concrete, property, target, span);
        write(ctx, &rewritten);
        if !ctx.builder.insertion_block_is_terminated() {
            binding.finish(ctx, span);
            join(ctx, done, &mut merged);
        }

        ctx.builder.position_at_end(next);
        ctx.restore_initialized_slots(before);
    }

    retire_optional_owner(ctx, class_owner, span);
    ctx.builder.terminate(Terminator::Br {
        target: fallback,
        args: Vec::new(),
    });
    ctx.builder.position_at_end(fallback);
    ctx.restore_initialized_slots(initial.clone());
    write(ctx, target);
    if !ctx.builder.insertion_block_is_terminated() {
        join(ctx, done, &mut merged);
    }
    ctx.builder.position_at_end(done);
    ctx.restore_initialized_slots(merged.unwrap_or(initial));
    true
}

/// Finds the named property below a captured nested dimension chain.
fn named_property_root(target: &Expr) -> Option<(&Expr, &str)> {
    let mut root = target;
    while let ExprKind::ArrayAccess { array, .. } = &root.kind {
        root = array;
    }
    let ExprKind::PropertyAccess { object, property } = &root.kind else {
        return None;
    };
    Some((object, property))
}

/// Collects exact classes whose ordinary visible slot has raw iterable storage.
fn iterable_property_classes(ctx: &LoweringContext<'_, '_>, property: &str) -> Vec<String> {
    ctx.classes
        .iter()
        .filter_map(|(class, info)| {
            if info
                .property_hooks
                .get(property)
                .is_some_and(|hooks| hooks.any())
                || !crate::ir_lower::expr::property_is_accessible_for_ir(
                    ctx, class, info, property,
                )
            {
                return None;
            }
            object_type_property_type(ctx, &PhpType::Object(class.clone()), property)
                .is_some_and(|ty| ty.codegen_repr() == PhpType::Iterable)
                .then(|| class.clone())
        })
        .collect()
}

/// Borrows the exact object payload selected by the runtime class-name ladder.
fn borrow_concrete_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: LoweredValue,
    class: &str,
    span: Span,
) -> LoweredValue {
    let ty = ctx.builder.value_php_type(receiver.value);
    let concrete = PhpType::Object(class.to_string());
    let value = if matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        ctx.emit_value(
            Op::MixedUnbox,
            vec![receiver.value],
            None,
            concrete,
            Op::mixed_unbox_effects(&PhpType::Object(class.to_string())),
            Some(span),
        )
    } else {
        ctx.emit_value(
            Op::Borrow,
            vec![receiver.value],
            None,
            concrete,
            Op::Borrow.default_effects(),
            Some(span),
        )
    };
    ctx.builder
        .set_value_ownership(value.value, Ownership::Borrowed);
    value
}

/// Boxes the selected raw property and rewrites its source root to that attached temporary.
fn bind(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: LoweredValue,
    property: &str,
    target: &Expr,
    span: Span,
) -> (Expr, Binding) {
    guard_property_dimension_fetch(
        ctx,
        receiver,
        property,
        span,
        PropertyDimensionFetchMode::Write,
    );
    let data = ctx.intern_string(property);
    let current = ctx.emit_owned_value(
        Op::PropGet,
        vec![receiver.value],
        Some(Immediate::Data(data)),
        PhpType::Iterable,
        Op::PropGet.default_effects(),
        Some(span),
    );
    let boxed = ctx.box_value_as_mixed(current, PhpType::Mixed, Some(span));
    let temp = ctx.declare_synthetic_php_local(PhpType::Mixed);
    let stored = crate::ir_lower::ownership::acquire_if_refcounted(ctx, boxed, Some(span));
    ctx.store_call_argument_local(&temp, stored, PhpType::Mixed, Some(span));
    if stored.value != boxed.value && ctx.value_is_owning_temporary(boxed) {
        crate::ir_lower::ownership::release_if_owned(ctx, boxed, Some(span));
    }
    let owner = ctx.local_slots[&temp];
    crate::ir_lower::expr::register_owned_call_operand(ctx, owner, span);
    let replacement = Expr::new(ExprKind::Variable(temp.clone()), span);
    (
        replace_property_root(target, &replacement),
        Binding {
            receiver,
            property: property.to_string(),
            temp,
            owner,
        },
    )
}

/// Replaces only the root property while preserving every captured dimension.
fn replace_property_root(target: &Expr, replacement: &Expr) -> Expr {
    let kind = match &target.kind {
        ExprKind::ArrayAccess { array, index } => ExprKind::ArrayAccess {
            array: Box::new(replace_property_root(array, replacement)),
            index: index.clone(),
        },
        ExprKind::PropertyAccess { .. } => return replacement.clone(),
        _ => unreachable!("an iterable property dispatch has one named property root"),
    };
    Expr::new(kind, target.span)
}

impl Binding {
    /// Republishes the possibly relocated payload, then retires the boxed temporary.
    fn finish(self, ctx: &mut LoweringContext<'_, '_>, span: Span) {
        let cell = ctx.load_local(&self.temp, Some(span));
        let borrowed = ctx.emit_value(
            Op::Borrow,
            vec![cell.value],
            None,
            PhpType::Mixed,
            Op::Borrow.default_effects(),
            Some(span),
        );
        ctx.builder
            .set_value_ownership(borrowed.value, Ownership::Borrowed);
        let data = ctx.intern_string(&self.property);
        ctx.emit_void(
            Op::PropSet,
            vec![self.receiver.value, borrowed.value],
            Some(Immediate::Data(data)),
            Op::PropSet.default_effects(),
            Some(span),
        );
        crate::ir_lower::expr::retire_owned_call_operand(ctx, self.owner, span);
    }
}

/// Joins one normal class arm and intersects its initialized-local facts.
fn join(
    ctx: &mut LoweringContext<'_, '_>,
    done: BlockId,
    merged: &mut Option<HashSet<LocalSlotId>>,
) {
    let slots = ctx.initialized_slots_snapshot();
    *merged = Some(merged.take().map_or_else(
        || slots.clone(),
        |previous| previous.intersection(&slots).copied().collect(),
    ));
    ctx.builder.terminate(Terminator::Br {
        target: done,
        args: Vec::new(),
    });
}

/// Retires a branch-local owner when one was required for the class-name result.
fn retire_optional_owner(
    ctx: &mut LoweringContext<'_, '_>,
    owner: Option<LocalSlotId>,
    span: Span,
) {
    if let Some(owner) = owner {
        crate::ir_lower::expr::retire_owned_call_operand(ctx, owner, span);
    }
}
