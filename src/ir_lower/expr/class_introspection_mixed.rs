//! Purpose:
//! Validates boxed arguments to AOT class introspection before selecting class metadata.
//!
//! Called from:
//! - `super::class_introspection::lower_class_introspection_value()`.
//!
//! Key details:
//! - Checks borrow one published input owner. Success retires it explicitly; exception
//!   unwinding retires it after taking ownership of the pending TypeError.
//! - The extracted name is rooted outside the input, whose destructor can itself throw.
//! - Runtime predicates distinguish objects from strings without coercing invalid PHP values.
//! - `get_class_vars()` accepts only a string tag; `get_class_methods()` also accepts an object.

use super::*;

/// Returns an owned class-name string from a boxed value, throwing for every tag `kind` rejects.
///
/// The returned string is a detached copy (the `Mixed`-to-string cast never aliases the cell
/// payload), so the input owner is released before the name is handed to class dispatch.
pub(super) fn lower_mixed_class_name(
    ctx: &mut LoweringContext<'_, '_>,
    kind: ClassIntrospectionKind,
    argument: LoweredValue,
    expr: &Expr,
) -> LoweredValue {
    let name_temp = ctx.declare_owned_hidden_temp(PhpType::Str);
    let name_slot = ctx.local_slots[&name_temp];
    register_owned_call_operand(ctx, name_slot, expr.span);
    let input_temp = ctx.declare_hidden_temp(PhpType::Mixed);
    store_value_into_temp(ctx, &input_temp, PhpType::Mixed, argument, expr.span);
    // The store took this lowering's own reference to the boxed argument. Publishing that slot
    // makes it reachable from same-frame exception cleanup as well as explicit retirement.
    let input_slot = ctx.local_slots[&input_temp];
    register_owned_call_operand(ctx, input_slot, expr.span);
    let string_block = ctx.builder.create_named_block("class.introspection.string", Vec::new());
    let invalid_block = ctx.builder.create_named_block("class.introspection.invalid", Vec::new());
    let merge = ctx.builder.create_named_block("class.introspection.name", Vec::new());

    if kind.accepts_object() {
        let object_block = ctx.builder.create_named_block("class.introspection.object", Vec::new());
        let string_check = ctx.builder.create_named_block("class.introspection.string.check", Vec::new());
        branch_on_input_type(ctx, &input_temp, crate::ir::PhpTypePredicate::Object, object_block, string_check, expr.span);
        ctx.builder.position_at_end(object_block);
        let input = borrow_published_input(ctx, &input_temp, expr.span);
        let target = crate::ir::RuntimeFnId::GetClass;
        let name = ctx.emit_value(
            Op::RuntimeCall,
            vec![input.value],
            Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(target))),
            PhpType::Str,
            target.effects(),
            Some(expr.span),
        );
        store_value_into_temp(ctx, &name_temp, PhpType::Str, name, expr.span);
        branch_to(ctx, merge);
        ctx.builder.position_at_end(string_check);
    }

    branch_on_input_type(ctx, &input_temp, crate::ir::PhpTypePredicate::String, string_block, invalid_block, expr.span);
    ctx.builder.position_at_end(string_block);
    let input = borrow_published_input(ctx, &input_temp, expr.span);
    store_value_into_temp(ctx, &name_temp, PhpType::Str, input, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(invalid_block);
    throw_invalid_introspection_argument(ctx, kind, &input_temp, expr.span);

    ctx.builder.position_at_end(merge);
    retire_owned_call_operand(ctx, input_slot, expr.span);
    unregister_owned_call_operand(ctx, name_slot, expr.span);
    take_owned_temp(ctx, &name_temp, expr.span)
}

/// Reads the published boxed input without moving the reference its owner slot holds.
///
/// The slot stays the only owner for the whole validation, so every consumer of this load must
/// see a borrow: a consumer that treated it as a temporary would free the caller's own value.
fn borrow_published_input(
    ctx: &mut LoweringContext<'_, '_>,
    input_temp: &str,
    span: Span,
) -> LoweredValue {
    let value = ctx.load_local(input_temp, Some(span));
    ctx.builder.set_value_ownership(value.value, Ownership::Borrowed);
    value
}

/// Branches on a PHP type predicate without transferring the published boxed argument.
fn branch_on_input_type(
    ctx: &mut LoweringContext<'_, '_>,
    input_temp: &str,
    predicate: crate::ir::PhpTypePredicate,
    success: BlockId,
    failure: BlockId,
    span: Span,
) {
    let input = borrow_published_input(ctx, input_temp, span);
    let condition = ctx.emit_value(
        Op::TypePredicate,
        vec![input.value],
        Some(Immediate::TypePredicate(predicate)),
        PhpType::Bool,
        Op::TypePredicate.default_effects(),
        Some(span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: condition.value,
        then_target: success,
        then_args: Vec::new(),
        else_target: failure,
        else_args: Vec::new(),
    });
}

/// Emits a catchable TypeError with PHP argument-type names instead of scalar string coercion.
///
/// The type name comes from `gettype()`, normalized to PHP's type-error spelling. Throwing
/// publishes the exception before unwinding the input owner, whose destructor may throw too.
fn throw_invalid_introspection_argument(
    ctx: &mut LoweringContext<'_, '_>,
    kind: ClassIntrospectionKind,
    input_temp: &str,
    span: Span,
) {
    let type_call = Expr::new(ExprKind::FunctionCall {
        name: Name::unqualified("gettype"),
        args: vec![Expr::new(ExprKind::Variable(input_temp.to_string()), span)],
    }, span);
    let type_temp = ctx.declare_hidden_temp(PhpType::Str);
    store_expr_into_temp(ctx, &type_temp, PhpType::Str, &type_call, span);
    let type_slot = ctx.local_slots[&type_temp];
    let type_var = Expr::new(ExprKind::Variable(type_temp.clone()), span);
    let type_name = Expr::new(ExprKind::Match {
        subject: Box::new(type_var.clone()),
        arms: [("integer", "int"), ("double", "float"), ("boolean", "bool"), ("NULL", "null")]
            .into_iter().map(|(source, target)| (
                vec![Expr::new(ExprKind::StringLiteral(source.to_string()), span)],
                Expr::new(ExprKind::StringLiteral(target.to_string()), span),
            )).collect(),
        default: Some(Box::new(type_var)),
    }, span);
    let message = Expr::new(ExprKind::BinaryOp {
        left: Box::new(Expr::new(ExprKind::BinaryOp {
            left: Box::new(Expr::new(
                ExprKind::StringLiteral(kind.invalid_argument_message().to_string()),
                span,
            )),
            op: BinOp::Concat,
            right: Box::new(type_name),
        }, span)),
        op: BinOp::Concat,
        right: Box::new(Expr::new(ExprKind::StringLiteral(" given".to_string()), span)),
    }, span);
    let exception = lower_expr(ctx, &Expr::new(ExprKind::NewObject {
        class_name: Name::unqualified("TypeError"),
        args: vec![message],
    }, span));
    // The type name cannot invoke user cleanup. Input/name records stay attached until
    // Throw publishes the exception, then the unwinder retires both before any catch.
    ctx.emit_void(
        Op::ReleaseLocalSlot,
        Vec::new(),
        Some(Immediate::LocalSlot(type_slot)),
        Op::ReleaseLocalSlot.default_effects(),
        Some(span),
    );
    ctx.builder.terminate(Terminator::Throw { value: exception.value });
}
