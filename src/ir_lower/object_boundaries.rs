//! Purpose:
//! Rejects runtime nulls at declared non-null object parameter and return boundaries.
//!
//! Called from:
//! - Function entry lowering and declared return storage coercion.
//!
//! Key details:
//! - Native property reads can return null despite their useful static object class hint.
//! - The existing frontend verifies nominal classes; these guards enforce non-nullness.
//! - All call surfaces share the callee entry, including references and inlined methods.

use crate::ir::{Op, Terminator};
use crate::ir_lower::context::{LoweredValue, LoweringContext};
use crate::span::Span;
use crate::types::PhpType;

/// Checks declared object parameters before user statements or constructor promotions run.
pub(super) fn guard_parameters(ctx: &mut LoweringContext<'_, '_>) {
    let Some(signature) = ctx.builder.function().signature.clone() else { return; };
    let callable = callable_label(ctx);
    for (index, (name, ty)) in signature.params.iter().enumerate() {
        let mut declared_type = if *ty == PhpType::Mixed && signature.ref_params.get(index).copied().unwrap_or(false) {
            signature.param_type_exprs.get(index).and_then(Option::as_ref)
                .map(|hint| ctx.type_expr_to_php_type_for_value(hint)).unwrap_or_else(|| ty.clone())
        } else { ty.clone() };
        if let PhpType::Object(class) = &declared_type {
            if class.eq_ignore_ascii_case("self") || class.eq_ignore_ascii_case("static") {
                if let Some(class) = &ctx.current_class { declared_type = PhpType::Object(class.clone()); }
            } else if class.eq_ignore_ascii_case("parent") {
                if let Some(parent) = ctx.current_class.as_ref().and_then(|class| ctx.classes.get(class))
                    .and_then(|class| class.parent.clone()) { declared_type = PhpType::Object(parent); }
            }
        }
        if name == "this" || !signature.declared_params.get(index).copied().unwrap_or(false)
            || !crate::types::builtin_classes::native_object_sink_needs_null_guard(&declared_type, ctx.classes, ctx.interfaces)
        { continue; }
        let value = ctx.load_local(name, None);
        let expected = crate::types::diagnostic_names::php_type_name(&declared_type);
        let message = format!(
            "{callable}(): Argument #{} (${name}) must be of type {expected}, null given", index + 1,
        );
        reject_null(ctx, value, &message, Span::dummy(), true);
    }
}

/// Checks a declared object return before a nullable box loses its runtime null tag.
pub(super) fn guard_return(
    ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Option<Span>,
) {
    if !ctx.return_type_is_declared
        || !crate::types::builtin_classes::native_object_sink_needs_null_guard(
            &ctx.return_php_type, ctx.classes, ctx.interfaces,
        )
    {
        return;
    }
    let expected = crate::types::diagnostic_names::php_type_name(&ctx.return_php_type);
    let callable = callable_label(ctx);
    let message = format!("{callable}(): Return value must be of type {expected}, null returned");
    reject_null(ctx, value, &message, span.unwrap_or_else(Span::dummy), false);
}

/// Gives property accessors the PHP label rather than their synthetic method symbol.
fn callable_label(ctx: &LoweringContext<'_, '_>) -> String {
    if let Some(label) = &ctx.builder.function().display_name { return label.clone(); }
    if let Some((class, method)) = ctx.owner_name().split_once("::") {
        if let Some(info) = ctx.classes.get(class) {
            for property in info.property_hooks.keys() {
                for (kind, accessor) in [("get", crate::names::property_hook_get_method(property)),
                    ("set", crate::names::property_hook_set_method(property))] {
                    if method.eq_ignore_ascii_case(&accessor) { return format!("{class}::${property}::{kind}"); }
                }
            }
        }
    }
    ctx.owner_name().to_string()
}

/// Branches on the shared null representation and balances a temporary on both outcomes.
fn reject_null(
    ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, message: &str,
    span: Span, consume_read: bool,
) {
    let condition = ctx.emit_value(Op::IsNull, vec![value.value], None, PhpType::Bool,
        Op::IsNull.default_effects(), Some(span));
    let owned = ctx.value_is_owning_temporary(value);
    if consume_read && owned {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
    let initialized = ctx.initialized_slots_snapshot();
    let rejected = ctx.builder.create_named_block("object.boundary.null", vec![]);
    let accepted = ctx.builder.create_named_block("object.boundary.accepted", vec![]);
    ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
        then_target: rejected, then_args: vec![], else_target: accepted, else_args: vec![] });
    ctx.builder.position_at_end(rejected);
    if !consume_read && owned {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
    crate::ir_lower::stmt::lower_throw_access_exception(ctx, "TypeError", message, span);
    ctx.builder.position_at_end(accepted);
    ctx.restore_initialized_slots(initialized);
}
