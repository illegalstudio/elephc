//! Purpose:
//! Binds array_push to a live property or element reference before evaluating its values.
//!
//! Called from:
//! - The by-reference builtin place dispatcher.
//!
//! Key details:
//! - A scoped managed alias preserves argument-selected storage and partial writes on throws.
//! - Managed cells preserve selection after object and index snapshots are retired.

use super::*;
use super::super::{array_push_spread, begin_call_argument_evaluation,
    lower_ref_assign_array_elem, lower_ref_assign_call,
    prepare_scoped_addressable_ref_array_receiver};
use super::super::call_operand_owners::publish_scoped_ref_receiver_alias;

/// Routes native array places through a managed alias instead of an early value snapshot.
pub(super) fn lower_live_array_push_place(
    ctx: &mut LoweringContext<'_, '_>, name: &Name, sig: &FunctionSig,
    args: &[Expr], expr: &Expr,
) -> Option<LoweredValue> {
    let definition = crate::builtins::registry::lookup(name.as_str())?;
    if !matches!(definition.spec.semantics.lowering,
        crate::builtins::semantics::BuiltinLowering::Runtime(
            crate::ir::RuntimeCallTarget::Function(crate::ir::RuntimeFnId::ArrayPush)))
    { return None; }
    let (index, place) = args.iter().enumerate().find_map(|(index, arg)| {
        let (parameter, place) = ref_param_binding(sig, index, arg)?;
        (parameter == 0 && is_candidate_place_shape(place)).then_some((index, place))
    })?;
    let mixed_place = static_place_type(ctx, place)
        .is_some_and(|ty| ty.codegen_repr() == PhpType::Mixed);
    let plan = property_plan(ctx, place).cloned();
    if (!is_array_place(ctx, place, &sig.params[0].1) && !mixed_place && plan.is_none())
        || (plan.as_ref().and_then(|plan| plan.rejection.as_ref()).is_none()
            && !addressable(ctx, place)) { return None; }
    begin_call_argument_evaluation(ctx);
    let mut snapshots = Vec::new();
    if let Some(message) = plan.as_ref().and_then(|plan| plan.rejection.as_deref()) {
        let frozen = freeze_place(ctx, place, &mut snapshots);
        if let Some(root) = property_root(&frozen) {
            if let ExprKind::PropertyAccess { property, .. } = &root.kind {
                let getter = crate::names::php_symbol_key(&crate::names::property_hook_get_method(property));
                let reads_hook = !ctx.in_own_property_accessor(property)
                    && plan.as_ref().and_then(|plan| plan.class_name.as_ref())
                        .and_then(|class| ctx.classes.get(class))
                        .and_then(|info| info.methods.get(&getter)).is_some_and(|sig| !sig.by_ref_return);
                if reads_hook {
                    let value = lower_expr(ctx, root);
                    if ctx.value_is_owning_temporary(value) {
                        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(expr.span));
                    }
                }
            }
        }
        let result = crate::ir_lower::stmt::lower_throw_access_error_expr_as(
            ctx, message, expr.span, PhpType::Int);
        return Some(array_push_spread::finish_scope(ctx, result));
    }
    let frozen = freeze_place(ctx, place, &mut snapshots);
    let alias = if reference_getter_method(ctx, place).is_some() {
        prepare_reference_getter_place(ctx, &frozen)
            .expect("preflighted reference getter must produce a managed array place")
    } else {
        prepare_scoped_addressable_ref_array_receiver(ctx, &frozen)
            .expect("preflighted native array place must be addressable").0
    };
    if let ExprKind::Variable(name) = &alias.kind {
        if ctx.ref_cell_owner_slot(name).is_some() {
            // The managed cell outlives its selected object and index snapshots independently.
            // Clear their values while leaving the outer unwind records balanced below the lease.
            for snapshot in snapshots.iter().rev() {
                let slot = ctx.local_slots[snapshot];
                ctx.emit_void(Op::ReleaseLocalSlot, Vec::new(), Some(Immediate::LocalSlot(slot)),
                    Op::ReleaseLocalSlot.default_effects(), Some(expr.span));
            }
        }
    }
    let mut rewritten = args.to_vec();
    rewritten[index] = match &args[index].kind {
        ExprKind::NamedArg { name, .. } => Expr::new(ExprKind::NamedArg {
            name: name.clone(), value: Box::new(alias),
        }, args[index].span),
        _ => alias,
    };
    let result = lower_function_call(ctx, name, &rewritten, expr);
    Some(array_push_spread::finish_scope(ctx, result))
}

/// Proves that reference preparation can bind the complete place without a value-fetch fallback.
fn addressable(ctx: &LoweringContext<'_, '_>, place: &Expr) -> bool {
    match &place.kind {
        ExprKind::Variable(_) => true,
        ExprKind::PropertyAccess { .. } => reference_getter_method(ctx, place).is_some()
            || property_plan(ctx, place).is_some_and(|plan| plan.live_addressable),
        ExprKind::StaticPropertyAccess { .. } => static_place_type(ctx, place)
            .is_some_and(|ty| ty.supports_live_array_push_place()),
        ExprKind::ArrayAccess { array, .. } => {
            let unsafe_indexed_alias = indexed_place_root_is_ref_bound(ctx, place)
                && indexed_place_has_native_array_container(ctx, place);
            !unsafe_indexed_alias && addressable(ctx, array)
        }
        _ => false,
    }
}

/// Returns whether this indexed place or one of its parents uses native packed-array storage.
fn indexed_place_has_native_array_container(ctx: &LoweringContext<'_, '_>, place: &Expr) -> bool {
    let mut current = place;
    loop {
        if matches!(
            static_place_type(ctx, current).map(|ty| ty.codegen_repr()),
            Some(PhpType::Array(_))
        ) {
            return true;
        }
        match &current.kind {
            ExprKind::ArrayAccess { array, .. } | ExprKind::ErrorSuppress(array) => {
                current = array;
            }
            _ => return false,
        }
    }
}

/// Returns whether an indexed place is rooted in a local already bound to a PHP reference cell.
fn indexed_place_root_is_ref_bound(ctx: &LoweringContext<'_, '_>, place: &Expr) -> bool {
    let mut root = place;
    while let ExprKind::ArrayAccess { array, .. } | ExprKind::ErrorSuppress(array) = &root.kind {
        root = array;
    }
    matches!(&root.kind, ExprKind::Variable(name) if ctx.is_ref_bound_local(name))
}

/// Leases the managed cell returned by a by-reference getter before later call arguments run.
fn prepare_reference_getter_place(
    ctx: &mut LoweringContext<'_, '_>, place: &Expr,
) -> Option<Expr> {
    match &place.kind {
        ExprKind::PropertyAccess { object, .. } => {
            let method = reference_getter_method(ctx, place)?;
            let call = Expr::new(ExprKind::MethodCall {
                object: object.clone(), method, args: Vec::new(),
            }, place.span);
            let alias = ctx.declare_synthetic_php_local(PhpType::Mixed);
            lower_ref_assign_call(ctx, &alias, &call, place.span);
            publish_scoped_ref_receiver_alias(ctx, &alias, place.span);
            Some(Expr::new(ExprKind::Variable(alias), place.span))
        }
        ExprKind::ArrayAccess { array, index } => {
            let parent = prepare_reference_getter_place(ctx, array)?;
            let element = Expr::new(ExprKind::ArrayAccess {
                array: Box::new(parent),
                index: index.clone(),
            }, place.span);
            let alias = ctx.declare_synthetic_php_local(PhpType::Mixed);
            lower_ref_assign_array_elem(ctx, &alias, &element, place.span);
            publish_scoped_ref_receiver_alias(ctx, &alias, place.span);
            Some(Expr::new(ExprKind::Variable(alias), place.span))
        }
        ExprKind::ErrorSuppress(inner) => prepare_reference_getter_place(ctx, inner),
        _ => None,
    }
}

/// Returns the synthetic getter method when the root property hands out its cell by reference.
fn reference_getter_method(ctx: &LoweringContext<'_, '_>, place: &Expr) -> Option<String> {
    match &place.kind {
        ExprKind::PropertyAccess { property, .. } => {
            if ctx.in_own_property_accessor(property) { return None; }
            let plan = property_plan(ctx, place)?;
            let method = crate::names::property_hook_get_method(property);
            let key = crate::names::php_symbol_key(&method);
            ctx.classes.get(plan.class_name.as_ref()?)?.methods.get(&key)
                .is_some_and(|sig| sig.by_ref_return).then_some(method)
        }
        ExprKind::ArrayAccess { array, .. } | ExprKind::ErrorSuppress(array) =>
            reference_getter_method(ctx, array),
        _ => None,
    }
}

/// Captures object identity and index values at their original argument evaluation point.
fn freeze_place(ctx: &mut LoweringContext<'_, '_>, place: &Expr, snapshots: &mut Vec<String>) -> Expr {
    match &place.kind {
        ExprKind::PropertyAccess { object, property } => {
            let temp = array_push_spread::stage_value(ctx, object);
            if let Some(class) = property_plan(ctx, place)
                .and_then(|plan| plan.class_name.clone()) {
                let object = ctx.load_local(&temp, Some(place.span));
                let class_data = ctx.intern_class_name(&class);
                let matches = ctx.emit_value(Op::InstanceOf, vec![object.value],
                    Some(Immediate::Data(class_data)), PhpType::Bool,
                    Op::InstanceOf.default_effects(), Some(place.span));
                crate::ir_lower::ownership::release_if_owned(ctx, object, Some(place.span));
                array_push_spread::require(ctx, matches, "TypeError",
                    "array_push() property receiver class is not stable at this source position",
                    true, place.span);
                // The source plan and runtime guard agree on this non-null class.
                // Frame storage may stay Mixed while the load recovers its native object view.
                ctx.set_local_type(&temp, PhpType::Object(class));
            }
            snapshots.push(temp.clone());
            Expr::new(ExprKind::PropertyAccess {
                object: Box::new(Expr::new(ExprKind::Variable(temp), object.span)),
                property: property.clone(),
            }, place.span)
        }
        ExprKind::ArrayAccess { array, index } => {
            let array = freeze_place(ctx, array, snapshots);
            let temp = array_push_spread::stage_value(ctx, index);
            snapshots.push(temp.clone());
            Expr::new(ExprKind::ArrayAccess {
                array: Box::new(array),
                index: Box::new(Expr::new(ExprKind::Variable(temp), index.span)),
            }, place.span)
        }
        _ => place.clone(),
    }
}

/// Reads the checker's decision for the property at the root of a nested array place.
fn property_plan<'a>(
    ctx: &'a LoweringContext<'_, '_>, place: &Expr,
) -> Option<&'a crate::types::ArrayPushPropertyPlan> {
    match &place.kind {
        ExprKind::PropertyAccess { .. } => ctx.array_push_property_plans
            .get(&(ctx.loop_storage_scope.clone(), place.span))
            .filter(|plan| plan.class_name.is_some()),
        ExprKind::ArrayAccess { array, .. } | ExprKind::ErrorSuppress(array) => property_plan(ctx, array),
        _ => None,
    }
}

/// Finds the property fetch whose reference rules apply to a nested dimension argument.
fn property_root(place: &Expr) -> Option<&Expr> {
    match &place.kind {
        ExprKind::PropertyAccess { .. } => Some(place),
        ExprKind::ArrayAccess { array, .. } | ExprKind::ErrorSuppress(array) => property_root(array),
        _ => None,
    }
}
