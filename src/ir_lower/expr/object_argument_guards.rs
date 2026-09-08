//! Purpose:
//! Checks dynamic object arguments at direct user-function call boundaries.
//!
//! Called from:
//! - `super::function_calls::lower_function_call` after shared argument planning.
//!
//! Key details:
//! - Arguments are already evaluated in PHP source order; diagnostics borrow those
//!   values and failure releases owned argument temporaries before throwing.

use super::*;
use crate::synthetic_class::{e_binop, e_call, e_str, e_var};

/// Emits runtime class/interface checks without changing argument storage or ownership.
pub(super) fn guard_object_arguments(
    ctx: &mut LoweringContext<'_, '_>,
    function: &str,
    signature: &FunctionSig,
    operands: &[ValueId],
    span: Span,
) {
    for (index, ((parameter, expected), value)) in signature.params.iter().zip(operands).enumerate() {
        let actual = ctx.builder.value_php_type(*value);
        if signature.ref_params.get(index).copied().unwrap_or(false)
            || !crate::types::param_binding::requires_object_argument_guard(expected, &actual)
        {
            continue;
        }
        let PhpType::Object(class) = expected else { unreachable!() };
        let data = ctx.intern_class_name(class);
        let condition = ctx.emit_value(Op::InstanceOf, vec![*value], Some(Immediate::Data(data)),
            PhpType::Bool, Op::InstanceOf.default_effects(), Some(span));
        let valid = ctx.builder.create_named_block("argument.object.valid", Vec::new());
        let invalid = ctx.builder.create_named_block("argument.object.invalid", Vec::new());
        ctx.builder.terminate(Terminator::CondBr {
            cond: condition.value, then_target: valid, then_args: Vec::new(),
            else_target: invalid, else_args: Vec::new(),
        });
        ctx.builder.position_at_end(invalid);
        let alias = ctx.declare_borrowed_hidden_temp(actual.clone());
        ctx.store_local(&alias, LoweredValue { value: *value, ir_type: value_ir_type(&actual) },
            actual, Some(span));
        let prefix = format!("{function}(): Argument #{} (${parameter}) must be of type {class}, ", index + 1);
        let message = e_binop(e_str(&prefix), BinOp::Concat,
            e_binop(argument_type_name(&alias, span), BinOp::Concat, e_str(" given")));
        let error = build_exception_from_expr(ctx, "TypeError", message, span);
        release_owned_call_arg_temporaries(ctx, operands, None, &ReturnArgAlias::Unknown, span);
        ctx.emit_void(Op::ThrowException, vec![error.value], None,
            Op::ThrowException.default_effects(), Some(span));
        ctx.builder.terminate(Terminator::Unreachable);
        ctx.builder.position_at_end(valid);
    }
}

/// Builds PHP's argument type spelling from a borrowed, already evaluated value.
pub(super) fn argument_type_name(variable: &str, span: Span) -> Expr {
    let boolean = Expr::new(ExprKind::Ternary {
        condition: Box::new(e_var(variable)), then_expr: Box::new(e_str("true")),
        else_expr: Box::new(e_str("false")),
    }, span);
    Expr::new(ExprKind::Match {
        subject: Box::new(e_call("gettype", vec![e_var(variable)])),
        arms: vec![
            (vec![e_str("integer")], e_str("int")),
            (vec![e_str("double")], e_str("float")),
            (vec![e_str("boolean")], boolean),
            (vec![e_str("NULL")], e_str("null")),
            (vec![e_str("object")], e_call("get_class", vec![e_var(variable)])),
        ],
        default: Some(Box::new(e_call("gettype", vec![e_var(variable)]))),
    }, span)
}
