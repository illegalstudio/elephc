//! Purpose:
//! Preserve native XML selections along deferred property write receiver chains.
//!
//! Called from:
//! - Instance property assignment and property dimension write lowering.
//!
//! Key details:
//! - Deferred traversal materializes nodes only after assignment operands have run.
//! - Ordinary object reads retain their existing dispatch and ownership rules.

use super::*;

/// Keeps storage-only XML receiver fetches after assignment operands, including indexed hops.
pub(crate) fn defer_native_property_write_receiver(
    ctx: &LoweringContext<'_, '_>, expr: &Expr,
) -> bool {
    let storage = match &expr.kind {
        ExprKind::Variable(_) | ExprKind::This | ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::PropertyAccess { object, .. } | ExprKind::DynamicPropertyAccess { object, .. } =>
            deferred_storage_chain(object),
        ExprKind::ArrayAccess { array, .. } => deferred_storage_chain(array),
        _ => false,
    };
    if !storage || !ctx.classes.contains_key("SimpleXMLElement") { return false; }
    let ty = super::super::array_cast_source::array_cast_source_type_for_ir(ctx, expr);
    matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        || is_opaque_property_object(ctx, &ty)
        || singular_object_class(&ty).is_some_and(|(class, _)|
            crate::types::builtin_classes::has_native_dimensions(class, ctx.classes))
}

/// Recognizes receiver paths whose computed operands have been stabilized by the parser.
fn deferred_storage_chain(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(_) | ExprKind::This | ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::PropertyAccess { object, .. } | ExprKind::DynamicPropertyAccess { object, .. } => deferred_storage_chain(object),
        ExprKind::ArrayAccess { array, .. } => deferred_storage_chain(array),
        _ => false,
    }
}

/// Evaluates writable receivers without converting missing XML selections to null.
pub(crate) fn lower_property_write_receiver(
    ctx: &mut LoweringContext<'_, '_>, expr: &Expr,
) -> LoweredValue {
    match &expr.kind {
        ExprKind::PropertyAccess { object, property } => {
            let object = lower_property_write_receiver(ctx, object);
            select_named(ctx, object, property, expr)
        }
        ExprKind::DynamicPropertyAccess { object, property } => {
            let (object, property) = if matches!(object.kind, ExprKind::Variable(_)) {
                let name = super::super::super::scoped_writes::property_name(ctx, property, expr.span)
                    .unwrap_or_else(|| { let name = lower_expr(ctx, property); coerce_runtime_property_name(ctx, name, expr.span) });
                (lower_property_write_receiver(ctx, object), name)
            } else {
                let object = lower_property_write_receiver(ctx, object);
                let pins = pin_in_flight_owners(ctx, &[object.value], expr.span);
                let name = super::super::super::scoped_writes::property_name(ctx, property, expr.span)
                    .unwrap_or_else(|| { let name = lower_expr(ctx, property); coerce_runtime_property_name(ctx, name, expr.span) });
                unpin_in_flight_owners(ctx, pins, expr.span);
                (object, name)
            };
            select_dynamic(ctx, object, property, expr)
        }
        ExprKind::ArrayAccess { array, index } => {
            let array = lower_property_write_receiver(ctx, array);
            lower_native_dimension_write_select(ctx, array, index, expr)
        }
        _ => lower_expr(ctx, expr),
    }
}

/// Selects a named XML child while retaining the ordinary getter for other receiver classes.
pub(in crate::ir_lower::expr) fn select_named(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str, expr: &Expr,
) -> LoweredValue {
    let object = refine_member_receiver(ctx, object, expr);
    let object = box_opaque_property_receiver(ctx, object, expr.span);
    if let Some(method) = native_property_method_for_value(ctx, object.value, property,
        elephc_builtin_contract::ObjectProperty::WriteSelect)
    {
        let name = Expr::new(ExprKind::StringLiteral(property.into()), expr.span);
        return lower_method_call_with_receiver(ctx, object, method, &[name], Op::MethodCall, expr);
    }
    let ty = ctx.builder.value_php_type(object.value);
    if !property.starts_with("__elephc_") && ctx.classes.contains_key("SimpleXMLElement")
        && matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
    {
        return native_xml_reads::select(ctx, object, property, expr);
    }
    lower_property_get_from_value(ctx, object, property, Op::PropGet, expr)
}

/// Preserves write-context target reads in parser-generated compound and inc/dec preludes.
pub(in crate::ir_lower::expr) fn lower_property_write_prelude(
    ctx: &mut LoweringContext<'_, '_>, target: &Expr, stmt: &Stmt, assignment_span: Span,
) -> bool {
    if !ctx.classes.contains_key("SimpleXMLElement") { return false; }
    if !matches!(target.kind, ExprKind::PropertyAccess { .. } | ExprKind::DynamicPropertyAccess { .. }) {
        return false;
    }
    let StmtKind::Assign { name, value } = &stmt.kind else { return false; };
    // Scoped native updates own their receiver; ordinary fallback keeps an ordinary read.
    if name.starts_with(crate::parser::ast::SCOPED_WRITE_TEMP_PREFIX) { return false; }
    let read = if value == target { name.contains("__elephc_incdec_old_") } else {
        matches!(&value.kind, ExprKind::BinaryOp { left, .. } if left.as_ref() == target
            && value.span.line == assignment_span.line && value.span.col == assignment_span.col)
    };
    if !read { return false; }
    let selected = lower_property_write_receiver(ctx, target);
    let ty = ctx.builder.value_php_type(selected.value);
    let temp = ctx.declare_owned_hidden_temp(ty.clone());
    store_value_into_temp(ctx, &temp, ty, selected, stmt.span);
    let slot = ctx.local_slots[&temp];
    register_owned_call_operand(ctx, slot, stmt.span);
    let read = Expr::new(ExprKind::Variable(temp), target.span);
    let value = if value == target { read } else {
        let ExprKind::BinaryOp { op, right, .. } = &value.kind else { unreachable!(); };
        Expr::new(ExprKind::BinaryOp { left: Box::new(read), op: op.clone(), right: right.clone() }, value.span)
    };
    crate::ir_lower::stmt::lower_stmt(ctx, &Stmt::new(StmtKind::Assign { name: name.clone(), value }, stmt.span));
    retire_owned_call_operand(ctx, slot, stmt.span);
    true
}

/// Selects with an evaluated name through the same native/ordinary ownership boundary.
pub(super) fn select_dynamic(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, name: LoweredValue, expr: &Expr,
) -> LoweredValue {
    let object = refine_member_receiver(ctx, object, expr);
    let property = match &expr.kind {
        ExprKind::DynamicPropertyAccess { property, .. } => property.as_ref().clone(),
        ExprKind::PropertyAccess { property, .. } => Expr::new(ExprKind::StringLiteral(property.clone()), expr.span),
        _ => Expr::new(ExprKind::StringLiteral(String::new()), expr.span),
    };
    let result_type = dynamic_property_get_result_type(ctx, object.value, &property, expr);
    lower_dynamic_property_fetch_or_select(ctx, object, name, result_type,
        PropertyFetchMode::Read, false, true, expr)
}
