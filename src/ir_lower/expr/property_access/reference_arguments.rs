//! Purpose:
//! Selects writable XML arguments without changing ordinary arguments behind erased receivers.
//!
//! Called from:
//! - Runtime-selected callable argument lowering after its passing-mode probe.
//!
//! Key details:
//! - Receiver and name effects run once at their PHP source position.
//! - Only the native receiver arm publishes an independent reference marker.
//! - Ordinary receiver arms retain the existing read and binder behavior.

use super::*;

/// Produces a native reference marker or an ordinary argument value after inspecting the receiver.
pub(crate) fn lower_runtime_reference_argument(
    ctx: &mut LoweringContext<'_, '_>, arg: &Expr, alias: &str,
) -> LoweredValue {
    match &arg.kind {
        ExprKind::NamedArg { value, .. } => lower_runtime_reference_argument(ctx, value, alias),
        ExprKind::PropertyAccess { object, property } => {
            let object = lower_property_write_receiver(ctx, object);
            native_xml_reads::dispatch(ctx, object, arg, PhpType::Mixed, false,
                |ctx, object| {
                    let name = Expr::new(ExprKind::StringLiteral(property.clone()), arg.span);
                    let method = elephc_builtin_contract::lookup_class("SimpleXMLElement")
                        .and_then(|class| class.operators)
                        .and_then(|operators| operators.property_method(elephc_builtin_contract::ObjectProperty::ReferenceSelect))
                        .expect("native XML reference adapter is catalogued");
                    let value = lower_method_call_with_receiver_direct(ctx, object, method, &[name], Op::MethodCall, arg);
                    marker(ctx, alias, value, arg.span)
                },
                |ctx, object| lower_property_get_without_native_dispatch(ctx, object, property, Op::PropGet, arg))
        }
        ExprKind::DynamicPropertyAccess { object, property } => {
            let (object, name) = if matches!(object.kind, ExprKind::Variable(_)) {
                let name = lower_name(ctx, property);
                (lower_property_write_receiver(ctx, object), name)
            } else {
                let object = lower_property_write_receiver(ctx, object);
                let (object, owner) = root_method_receiver(ctx, object, arg.span);
                let name = lower_name(ctx, property);
                let object = if let Some(owner) = owner {
                    let ty = ctx.builder.value_php_type(object.value);
                    take_published_container(ctx, owner, ty, arg.span)
                } else { object };
                (object, name)
            };
            native_xml_reads::dynamic_reference_argument(ctx, object, name, alias, arg)
        }
        ExprKind::ArrayAccess { array, index } => {
            let object = lower_property_write_receiver(ctx, array);
            native_xml_reads::dispatch(ctx, object, arg, PhpType::Mixed, false,
                |ctx, object| {
                    let value = lower_native_dimension_write_select(ctx, object, index, arg);
                    marker(ctx, alias, value, arg.span)
                },
                |ctx, object| array_access::lower_array_access_from_lowered_receiver(ctx, object, index, arg))
        }
        _ => lower_expr(ctx, arg),
    }
}

/// Coerces a name once before the shared dynamic selector publishes its lifetime.
fn lower_name(ctx: &mut LoweringContext<'_, '_>, property: &Expr) -> LoweredValue {
    let value = lower_expr(ctx, property);
    coerce_runtime_property_name(ctx, value, property.span)
}

/// Publishes a native selection in the caller-owned cell before constructing its binder marker.
pub(super) fn marker(ctx: &mut LoweringContext<'_, '_>, alias: &str, value: LoweredValue, span: Span) -> LoweredValue {
    store_value_into_temp(ctx, alias, PhpType::Mixed, value, span);
    lower_invoker_ref_variadic_arg_marker(ctx, alias, span)
}
