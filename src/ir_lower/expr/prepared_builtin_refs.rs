//! Purpose:
//! Gives prepared builtin value graphs live, addressable reference arguments.
//!
//! Called from:
//! - Shared builtin argument planning and signature-argument lowering.
//!
//! Key details:
//! - Aliases are established at the argument source position and rooted in the call ledger.
//! - The shared call planner retains named/spread ordering; destination storage owns writeback.

use super::*;

/// Selects reviewed prepared graphs whose checked conversions need an original writable binding.
pub(super) fn active(ctx: &LoweringContext<'_, '_>, name: &str) -> bool {
    crate::semantics::conversions::checked_casts(ctx.semantic_profile, ctx.builtin_source_mode())
        && crate::builtins::registry::lookup(name).is_some_and(|def| matches!(
            def.spec.semantics.callable, crate::builtins::semantics::BuiltinCallablePolicy::Prepared(_))
            && def.params.iter().zip(&def.ref_params).any(|((_, ty), by_ref)| *by_ref && *ty == PhpType::Mixed))
}

/// Binds one Mixed property or element through the same cells as ordinary PHP references.
pub(super) fn lower_argument(ctx: &mut LoweringContext<'_, '_>, sig: &FunctionSig,
    index: usize, arg: &Expr) -> Option<ValueId> {
    if !sig.ref_params.get(index).copied().unwrap_or(false)
        || sig.params.get(index).is_none_or(|(_, ty)| *ty != PhpType::Mixed)
        || !ctx.call_argument_evaluation_scopes.last().is_some_and(|scope|
            scope.prepared_builtin_reference_arguments && scope.expression_depth == ctx.expression_depth)
    { return None; }
    let mut place = arg;
    while let ExprKind::NamedArg { value, .. } | ExprKind::ErrorSuppress(value) = &place.kind { place = value; }
    if !mixed_place(ctx, place) { return None; }
    let alias = ctx.declare_synthetic_php_local(PhpType::Mixed);
    let owner = ctx.declare_ref_cell_owner(&alias, PhpType::Mixed);
    ctx.builder.mark_reference_argument_owner(owner);
    publish_scoped_ref_receiver_alias(ctx, &alias, place.span);
    crate::ir_lower::stmt::lower_ref_assign(ctx, &alias, place, place.span);
    let variable = Expr::new(ExprKind::Variable(alias), place.span);
    Some(lower_arg_with_signature(ctx, sig, index, &variable))
}

/// Recognizes original Mixed storage that can expose a managed reference without narrowing.
pub(super) fn mixed_place(ctx: &LoweringContext<'_, '_>, place: &Expr) -> bool {
    if let ExprKind::NamedArg { value, .. } | ExprKind::ErrorSuppress(value) = &place.kind {
        return mixed_place(ctx, value);
    }
    match &place.kind {
        ExprKind::PropertyAccess { .. } => super::ref_place_args::live_mixed_property_argument(ctx, place, &PhpType::Mixed),
        ExprKind::StaticPropertyAccess { .. } => super::ref_place_args::static_place_type(ctx, place) == Some(PhpType::Mixed),
        ExprKind::ArrayAccess { array, .. } => {
            super::ref_place_args::static_place_type(ctx, place) == Some(PhpType::Mixed)
                || matches!(super::array_cast_source::array_cast_source_type_for_ir(ctx, array).codegen_repr(),
                    PhpType::Mixed | PhpType::Union(_))
        }
        _ => false,
    }
}
