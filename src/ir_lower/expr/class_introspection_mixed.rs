//! Purpose:
//! Validates boxed arguments to AOT class-method introspection before selecting class metadata.
//!
//! Called from:
//! - `super::class_introspection::lower_class_introspection_value()`.
//!
//! Key details:
//! - Roots the input once across type checks, class-name extraction, and catchable errors.
//! - Runtime predicates distinguish objects from strings without coercing invalid PHP values.

use super::*;

/// Returns an owned class-name string from a boxed object or string, throwing for other tags.
pub(super) fn lower_mixed_methods_class_name(
    ctx: &mut LoweringContext<'_, '_>,
    argument: LoweredValue,
    expr: &Expr,
) -> LoweredValue {
    let input_temp = ctx.declare_hidden_temp(PhpType::Mixed);
    store_value_into_temp(ctx, &input_temp, PhpType::Mixed, argument, expr.span);
    let name_temp = ctx.declare_owned_hidden_temp(PhpType::Str);
    let object_block = ctx.builder.create_named_block("class.methods.object", Vec::new());
    let string_check = ctx.builder.create_named_block("class.methods.string.check", Vec::new());
    let string_block = ctx.builder.create_named_block("class.methods.string", Vec::new());
    let invalid_block = ctx.builder.create_named_block("class.methods.invalid", Vec::new());
    let merge = ctx.builder.create_named_block("class.methods.name", Vec::new());

    branch_on_input_type(ctx, &input_temp, crate::ir::PhpTypePredicate::Object, object_block, string_check, expr.span);
    ctx.builder.position_at_end(object_block);
    let input = ctx.load_local(&input_temp, Some(expr.span));
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
    branch_on_input_type(ctx, &input_temp, crate::ir::PhpTypePredicate::String, string_block, invalid_block, expr.span);
    ctx.builder.position_at_end(string_block);
    let input = ctx.load_local(&input_temp, Some(expr.span));
    store_value_into_temp(ctx, &name_temp, PhpType::Str, input, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(invalid_block);
    throw_invalid_methods_argument(ctx, &input_temp, expr.span);

    ctx.builder.position_at_end(merge);
    ctx.clear_owned_hidden_temp(&input_temp, Some(expr.span));
    take_owned_temp(ctx, &name_temp, expr.span)
}

/// Branches on a PHP type predicate without transferring the rooted boxed argument.
fn branch_on_input_type(
    ctx: &mut LoweringContext<'_, '_>,
    input_temp: &str,
    predicate: crate::ir::PhpTypePredicate,
    success: BlockId,
    failure: BlockId,
    span: Span,
) {
    let input = ctx.load_local(input_temp, Some(span));
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
fn throw_invalid_methods_argument(ctx: &mut LoweringContext<'_, '_>, input_temp: &str, span: Span) {
    let type_call = Expr::new(ExprKind::FunctionCall {
        name: Name::unqualified("gettype"),
        args: vec![Expr::new(ExprKind::Variable(input_temp.to_string()), span)],
    }, span);
    let type_temp = ctx.declare_hidden_temp(PhpType::Str);
    store_expr_into_temp(ctx, &type_temp, PhpType::Str, &type_call, span);
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
            left: Box::new(Expr::new(ExprKind::StringLiteral(
                "get_class_methods(): Argument #1 ($object_or_class) must be an object or a valid class name, ".to_string(),
            ), span)),
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
    ctx.clear_owned_hidden_temp(&type_temp, Some(span));
    ctx.clear_owned_hidden_temp(input_temp, Some(span));
    ctx.builder.terminate(Terminator::Throw { value: exception.value });
}
