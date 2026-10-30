//! Purpose:
//! Share native XML property read-modify-write semantics across statement and expression forms.
//!
//! Called from:
//! - Property assignment lowering and parser-generated assignment expressions.
//!
//! Key details:
//! - The RHS precedes writable selection; computed receivers retain their earlier evaluation.
//! - Ordinary objects keep their getters and setters, including boxed property hooks.
//! - Captured owners and results remain unwind-visible during every callback.

use super::*;

/// One PHP-visible property name before it has been evaluated.
#[derive(Clone, Copy)]
enum Property<'a> { Named(&'a str), Dynamic(&'a Expr) }

/// A value held in a hidden local throughout the update.
struct Capture { slot: LocalSlotId, rooted: bool }

/// Captured operands shared by native updates and late runtime-name assignments.
struct Inputs { receiver: String, name: String, rhs: String, owners: Vec<Capture> }

/// Preserves late variable names and name conversions for native-capable dynamic assignments.
pub(super) fn lower_plain_dynamic(
    ctx: &mut LoweringContext<'_, '_>, object: &Expr, property: &Expr, value: &Expr, span: Span,
) -> bool {
    if !possible(ctx, object, Property::Dynamic(property))
        || !defer_native_property_write_receiver(ctx, object) { return false; }
    let expr = Expr::new(ExprKind::DynamicPropertyAccess {
        object: Box::new(object.clone()), property: Box::new(property.clone()),
    }, span);
    let Ok(inputs) = prepare_inputs(ctx, object, Property::Dynamic(property), value, false, &expr) else { return true; };
    lower_dynamic_property_assign_prepared(ctx, &local_expr(&inputs.receiver, span),
        &local_expr(&inputs.name, span), &local_expr(&inputs.rhs, span), span);
    retire(ctx, inputs.owners, span);
    true
}

/// Routes the parser's statement-form property self-read through the shared native update.
pub(crate) fn lower_statement(
    ctx: &mut LoweringContext<'_, '_>, object: &Expr, property: &str, value: &Expr, span: Span,
) -> bool {
    if !crate::parser::ast::is_compound_property_assignment_self_read(value, object, property, span, &[]) {
        return false;
    }
    let ExprKind::BinaryOp { op, right, .. } = &value.kind else { return false; };
    if !possible(ctx, object, Property::Named(property)) || operation(op).is_none() { return false; }
    let result = lower(ctx, object, Property::Named(property), op, right, false, false, false, "assign", value);
    if !ctx.builder.insertion_block_is_terminated() {
        crate::ir_lower::ownership::release_if_owned(ctx, result, Some(span));
    }
    true
}

/// Recognizes native-capable compound expressions and canonical property inc/dec captures.
pub(super) fn lower_assignment(
    ctx: &mut LoweringContext<'_, '_>, target: &Expr, value: &Expr, result: Option<&Expr>,
    prelude: &[Stmt], conditional: Option<&str>, expr: &Expr,
) -> Option<LoweredValue> {
    if conditional.is_some() { return None; }
    let (object, property) = match &target.kind {
        ExprKind::PropertyAccess { object, property } => (object.as_ref(), Property::Named(property)),
        ExprKind::DynamicPropertyAccess { object, property } => (object.as_ref(), Property::Dynamic(property)),
        _ => return None,
    };
    let scoped = prelude.iter().any(|stmt| matches!(&stmt.kind, StmtKind::Assign { name, .. }
        if name.starts_with(crate::parser::ast::SCOPED_WRITE_TEMP_PREFIX)));
    let ty = array_cast_source::array_cast_source_type_for_ir(ctx, object);
    let snapshot = matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) || singular_object_class(&ty).is_some();
    if !possible(ctx, object, property) && !(scoped && snapshot) { return None; }
    if let ExprKind::BinaryOp { left, op, right } = &value.kind {
        if left.as_ref() == target && value.span.line == expr.span.line
            && value.span.col == expr.span.col && operation(op).is_some() {
            if scoped { super::super::scoped_writes::consume_update_operands(ctx); }
            for stmt in prelude { crate::ir_lower::stmt::lower_stmt(ctx, stmt); }
            return Some(lower(ctx, object, property, op, right, false, scoped, false, "assign", expr));
        }
    }
    let ExprKind::Variable(assigned) = &value.kind else { return None; };
    let (position, operation_expr) = prelude.iter().enumerate().rev().find_map(|(position, stmt)| {
        let StmtKind::Assign { name, value } = &stmt.kind else { return None; };
        (name == assigned).then_some((position, value))
    })?;
    if operation_expr.span.line != expr.span.line || operation_expr.span.col != expr.span.col { return None; }
    let ExprKind::BinaryOp { left, op, right } = &operation_expr.kind else { return None; };
    operation(op)?;
    let (prefix, post) = if left.as_ref() == target { (position, false) } else {
        let ExprKind::Variable(old) = &left.kind else { return None; };
        let position = prelude[..position].iter().position(|stmt| matches!(&stmt.kind,
            StmtKind::Assign { name, value } if name == old && value == target))?;
        if !old.contains("__elephc_incdec_old_") { return None; }
        (position, result.is_some_and(|value| matches!(&value.kind, ExprKind::Variable(name) if name == old)))
    };
    if scoped { super::super::scoped_writes::consume_update_operands(ctx); }
    for stmt in &prelude[..prefix] { crate::ir_lower::stmt::lower_stmt(ctx, stmt); }
    let reread = result.is_some_and(|result| result == target);
    Some(lower(ctx, object, property, op, right, post, scoped, reread, "increment/decrement", expr))
}

/// Admits XML subclasses and erased receivers while leaving other typed objects unchanged.
fn possible(ctx: &LoweringContext<'_, '_>, object: &Expr, property: Property<'_>) -> bool {
    if !ctx.classes.contains_key("SimpleXMLElement")
        || matches!(property, Property::Named(name) if name.starts_with("__elephc_")) { return false; }
    let ty = array_cast_source::array_cast_source_type_for_ir(ctx, object);
    matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        || singular_object_class(&ty).is_some_and(|(class, _)| class.is_empty()
            || ctx.interfaces.contains_key(class)
            || crate::types::builtin_classes::has_native_dimensions(class, ctx.classes))
}

/// Captures operands once, then chooses the native update or the ordinary property operation.
fn lower(
    ctx: &mut LoweringContext<'_, '_>, object: &Expr, property: Property<'_>, op: &BinOp,
    right: &Expr, post: bool, scoped: bool, reread: bool, verb: &str, expr: &Expr,
) -> LoweredValue {
    let staging = prepublish_call_result(ctx, &PhpType::Mixed, expr.span);
    let inputs = match prepare_inputs(ctx, object, property, right, scoped, expr) {
        Ok(inputs) => inputs, Err(value) => return value,
    };
    let name_expr = local_expr(&inputs.name, expr.span);
    let right_expr = local_expr(&inputs.rhs, expr.span);
    let parent = load(ctx, &inputs.receiver, expr.span);
    let name_value = load(ctx, &inputs.name, expr.span);
    crate::ir_lower::stmt::property_write_receiver::guard_mixed_dynamic_write(
        ctx, parent, name_value, verb, expr.span,
    );
    let code = operation(op).expect("validated update operation") as i64;
    let result = if !ctx.classes.contains_key("SimpleXMLElement") {
        ordinary(ctx, parent, property, &name_expr, op, &right_expr, post, reread, expr)
    } else { property_access::native_xml_reads::dispatch(
        ctx, parent, expr, PhpType::Mixed, false,
        |ctx, receiver| lower_method_call_with_receiver(ctx, receiver, "__elephc_property_update", &[
            name_expr.clone(), right_expr.clone(), Expr::new(ExprKind::IntLiteral(code), expr.span),
            Expr::new(ExprKind::BoolLiteral(post), expr.span),
        ], Op::MethodCall, expr),
        |ctx, receiver| ordinary(ctx, receiver, property, &name_expr, op, &right_expr, post, reread, expr),
    ) };
    stage_call_result(ctx, staging.as_ref(), result, expr.span);
    retire(ctx, inputs.owners, expr.span);
    take_prepublished_call_result(ctx, staging, result, expr.span)
}

/// Evaluates computed operands early while loading variable names and coercing names after the RHS.
fn prepare_inputs(
    ctx: &mut LoweringContext<'_, '_>, object: &Expr, property: Property<'_>, right: &Expr, scoped: bool, expr: &Expr,
) -> Result<Inputs, LoweredValue> {
    let mut owners = Vec::new();
    let late = scoped || is_static_property_write_chain(object) || defer_native_property_write_receiver(ctx, object);
    let early = if late { None } else { Some(capture_receiver(ctx, object, &[], &mut owners, expr.span)?) };
    let raw_name = match property {
        Property::Dynamic(name) if !matches!(name.kind, ExprKind::Variable(_)) => {
            let value = lower_expr(ctx, name);
            if ctx.builder.insertion_block_is_terminated() { return Err(value); }
            Some(capture(ctx, value, &mut owners, expr.span))
        }
        _ => None,
    };
    let right = lower_expr(ctx, right);
    if ctx.builder.insertion_block_is_terminated() { return Err(right); }
    let rhs = capture(ctx, right, &mut owners, expr.span);
    let mut receiver = early;
    if receiver.is_none() && !matches!(object.kind, ExprKind::Variable(_)) {
        let right = load(ctx, &rhs, expr.span);
        receiver = Some(capture_receiver(ctx, object, &[right.value], &mut owners, expr.span)?);
    }
    let scoped_name = match property {
        Property::Dynamic(name) => super::super::scoped_writes::property_name(ctx, name, expr.span),
        Property::Named(_) => None,
    };
    let name = if let Some(name) = scoped_name { name } else { match property {
        Property::Named(name) => lower_expr(ctx, &Expr::new(ExprKind::StringLiteral(name.into()), expr.span)),
        Property::Dynamic(name) => {
            let raw = if let Some(raw) = raw_name { raw } else {
                let value = lower_expr(ctx, name);
                if ctx.builder.insertion_block_is_terminated() { return Err(value); }
                capture(ctx, value, &mut owners, expr.span)
            };
            let value = load(ctx, &raw, expr.span);
            property_access::coerce_runtime_property_name(ctx, value, expr.span)
        }
    } };
    let name = capture(ctx, name, &mut owners, expr.span);
    let receiver = if let Some(receiver) = receiver { receiver } else {
        let right = load(ctx, &rhs, expr.span);
        capture_receiver(ctx, object, &[right.value], &mut owners, expr.span)?
    };
    Ok(Inputs { receiver, name, rhs, owners })
}

/// Shares the ordinary receiver snapshot and preserves coerced prefix results.
fn ordinary(
    ctx: &mut LoweringContext<'_, '_>, receiver: LoweredValue, property: Property<'_>, name: &Expr,
    op: &BinOp, right: &Expr, post: bool, reread: bool, expr: &Expr,
) -> LoweredValue {
    let staging = prepublish_call_result(ctx, &PhpType::Mixed, expr.span);
    let mut owners = Vec::new();
    let parent = capture(ctx, receiver, &mut owners, expr.span);
    let receiver = load(ctx, &parent, expr.span);
    let current = match property {
        Property::Named(property) => lower_property_get_from_value(ctx, receiver, property, Op::PropGet, expr),
        Property::Dynamic(_) => lower_dynamic_property_fetch_from_value(ctx, receiver, name, PropertyFetchMode::Read, expr),
    };
    let current = capture(ctx, current, &mut owners, expr.span);
    let operation = Expr::new(ExprKind::BinaryOp {
        left: Box::new(local_expr(&current, expr.span)), op: op.clone(), right: Box::new(right.clone()),
    }, expr.span);
    let next = lower_expr(ctx, &operation);
    let next = capture(ctx, next, &mut owners, expr.span);
    let receiver = local_expr(&parent, expr.span);
    let assigned = local_expr(&next, expr.span);
    match property {
        Property::Named(property) => crate::ir_lower::stmt::lower_stmt(ctx, &Stmt::new(
            StmtKind::PropertyAssign { object: Box::new(receiver), property: property.into(), value: assigned }, expr.span)),
        Property::Dynamic(_) => lower_dynamic_property_assign(ctx, &receiver, name, &assigned, expr.span),
    }
    let result = if reread {
        let receiver = load(ctx, &parent, expr.span);
        match property {
            Property::Named(property) => lower_property_get_from_value(ctx, receiver, property, Op::PropGet, expr),
            Property::Dynamic(_) => lower_dynamic_property_fetch_from_value(ctx, receiver, name, PropertyFetchMode::Read, expr),
        }
    } else { load(ctx, if post { &current } else { &next }, expr.span) };
    // The ordinary getter/arithmetic may produce a concrete scalar or string. The
    // common result owner has boxed storage and must outlive every operand lease.
    let result = ctx.box_value_as_mixed(result, PhpType::Mixed, Some(expr.span));
    stage_call_result(ctx, staging.as_ref(), result, expr.span);
    retire(ctx, owners, expr.span);
    take_prepublished_call_result(ctx, staging, result, expr.span)
}

/// Publishes a receiver slot before traversing leases that must unwind above it.
fn capture_receiver(
    ctx: &mut LoweringContext<'_, '_>, object: &Expr, operands: &[ValueId], owners: &mut Vec<Capture>, span: Span,
) -> Result<String, LoweredValue> {
    let checked = array_cast_source::array_cast_source_type_for_ir(ctx, object);
    let capture_type = match &checked {
        PhpType::Object(class) if !class.is_empty() && !ctx.interfaces.contains_key(class)
            && !crate::types::builtin_classes::has_native_dimensions(class, ctx.classes) => checked,
        _ => PhpType::Mixed,
    };
    let name = reserve(ctx, capture_type.clone(), owners, span);
    let receiver = lower_static_property_write_chain_with_operands(ctx, object, operands, span);
    if ctx.builder.insertion_block_is_terminated() { return Err(receiver.value); }
    let ty = ctx.builder.value_php_type(receiver.value.value);
    let borrowed = ctx.emit_value(Op::Borrow, vec![receiver.value.value], None, ty,
        Op::Borrow.default_effects(), Some(span));
    ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
    let captured = if capture_type == PhpType::Mixed {
        ctx.box_value_as_mixed(borrowed, PhpType::Mixed, Some(span))
    } else { borrowed };
    let snapshot = crate::ir_lower::ownership::copy_assignment_value(ctx, captured, Some(span));
    store_value_into_temp(ctx, &name, capture_type, snapshot, span);
    receiver.finish(ctx, span);
    Ok(name)
}

/// Captures a produced value using the same retaining-store contract as other call operands.
fn capture(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, owners: &mut Vec<Capture>, span: Span) -> String {
    let value = crate::ir_lower::ownership::copy_assignment_value(ctx, value, Some(span));
    let ty = ctx.builder.value_php_type(value.value);
    let name = reserve(ctx, ty.clone(), owners, span);
    store_value_into_temp(ctx, &name, ty, value, span);
    name
}

/// Registers an initially zeroed cleanup slot before callbacks can populate it.
fn reserve(ctx: &mut LoweringContext<'_, '_>, ty: PhpType, owners: &mut Vec<Capture>, span: Span) -> String {
    let rooted = Ownership::php_type_needs_lifetime_tracking(&ty);
    let name = ctx.declare_hidden_temp(ty);
    let slot = ctx.local_slots[&name];
    if rooted { register_owned_call_operand(ctx, slot, span); }
    owners.push(Capture { slot, rooted });
    name
}

/// Lends a captured operand while its registered slot owns cleanup.
fn load(ctx: &mut LoweringContext<'_, '_>, name: &str, span: Span) -> LoweredValue {
    let value = ctx.load_local(name, Some(span));
    if Ownership::php_type_needs_lifetime_tracking(&ctx.builder.value_php_type(value.value)) {
        ctx.builder.set_value_ownership(value.value, Ownership::Borrowed);
    }
    value
}

/// Names one captured PHP operand in a synthesized ordinary expression.
fn local_expr(name: &str, span: Span) -> Expr { Expr::new(ExprKind::Variable(name.into()), span) }

/// Clears every owner in reverse registration order, including during throwing cleanup.
fn retire(ctx: &mut LoweringContext<'_, '_>, owners: Vec<Capture>, span: Span) {
    for owner in owners.into_iter().rev() {
        if owner.rooted { unregister_owned_call_operand(ctx, owner.slot, span); }
        ctx.emit_void(Op::ReleaseLocalSlot, vec![], Some(Immediate::LocalSlot(owner.slot)),
            Op::ReleaseLocalSlot.default_effects(), Some(span));
    }
}

/// Maps frontend operators to the backend-neutral update codes consumed by the prelude.
fn operation(op: &BinOp) -> Option<elephc_builtin_contract::ObjectPropertyUpdate> {
    use elephc_builtin_contract::ObjectPropertyUpdate as Update;
    Some(match op {
        BinOp::Add => Update::Add, BinOp::Sub => Update::Sub,
        BinOp::Mul => Update::Mul, BinOp::Div => Update::Div,
        BinOp::Mod => Update::Mod, BinOp::Pow => Update::Pow,
        BinOp::Concat => Update::Concat, BinOp::BitAnd => Update::BitAnd,
        BinOp::BitOr => Update::BitOr, BinOp::BitXor => Update::BitXor,
        BinOp::ShiftLeft => Update::ShiftLeft, BinOp::ShiftRight => Update::ShiftRight,
        _ => return None,
    })
}
