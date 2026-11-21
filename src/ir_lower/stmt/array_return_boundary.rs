//! Purpose:
//! Verifies declared PHP array returns without narrowing packed-or-keyed storage.
//!
//! Called from:
//! - `return_coercions::coerce_to_return_type()` for an opaque declared-array value.
//!
//! Key details:
//! - Uses existing type predicates and ordinary TypeError construction through AST/EIR.
//! - The guard accepts arrays only; it never casts an object or scalar to an array.
//! - A source lease survives diagnostic construction and transfers exactly once on success.

use super::*;
use crate::synthetic_class::{e_binop, e_call, e_new_fq, e_str, e_var};

/// Guards one opaque array return and keeps its original key/storage representation.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, source: LoweredValue, span: Span,
) -> LoweredValue {
    let owner = super::string_return_boundary::hold(ctx, source, span);
    let alias = super::string_return_boundary::borrowed_alias(ctx, &owner, span);
    let valid = lower_expr(ctx, &e_call("is_array", vec![e_var(&alias)]));
    let accepted = ctx.builder.create_named_block("return.array_accepted", Vec::new());
    let rejected = ctx.builder.create_named_block("return.array_rejected", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: valid.value, then_target: accepted, then_args: Vec::new(),
        else_target: rejected, else_args: Vec::new(),
    });
    ctx.builder.position_at_end(rejected);
    let prefix = format!("{}(): Return value must be of type array, ", ctx.owner_name());
    let message = e_binop(
        e_binop(e_str(&prefix), crate::parser::ast::BinOp::Concat,
            rejected_type_expr(&alias)),
        crate::parser::ast::BinOp::Concat, e_str(" returned"),
    );
    let error = lower_expr(ctx, &e_new_fq("TypeError", vec![message]));
    super::string_return_boundary::clear_borrowed_alias(ctx, &alias, span);
    super::string_return_boundary::release_slot(ctx, &owner, span);
    ctx.builder.terminate(Terminator::Throw { value: error.value });

    ctx.builder.position_at_end(accepted);
    super::string_return_boundary::clear_borrowed_alias(ctx, &alias, span);
    let source = ctx.load_local(&owner, Some(span));
    // MixedBox on an already boxed source forwards its pointer without retaining it. Acquire
    // the checked result explicitly before retiring the source lease or its deferred slot view.
    let result = ctx.emit_owned_value(
        Op::Acquire, vec![source.value], None, ctx.return_php_type.clone(),
        Op::Acquire.default_effects(), Some(span),
    );
    super::string_return_boundary::release_slot(ctx, &owner, span);
    result
}

/// Return verification names boolean values literally, unlike get_debug_type's general bool name.
fn rejected_type_expr(source: &str) -> Expr {
    let boolean = Expr::new(ExprKind::Ternary {
        condition: Box::new(e_var(source)), then_expr: Box::new(e_str("true")),
        else_expr: Box::new(e_str("false")),
    }, Span::synthetic());
    Expr::new(ExprKind::Ternary {
        condition: Box::new(e_call("is_bool", vec![e_var(source)])),
        then_expr: Box::new(boolean),
        else_expr: Box::new(super::string_return_boundary::debug_type_expr(source)),
    }, Span::synthetic())
}
