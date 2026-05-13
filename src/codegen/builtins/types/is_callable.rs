use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::checker::builtins::is_supported_builtin_function;
use crate::types::PhpType;

/// is_callable(value): bool
///
/// Static evaluation when the argument's compile-time type is Callable
/// (closures, first-class callables) or a string literal that resolves
/// to a known builtin / user function. Non-literal strings, arrays, and
/// other dynamic shapes return false here pending runtime lookup
/// (PHP also accepts `[$obj, "method"]` pairs and objects implementing
/// `__invoke` — those routes are tracked as follow-ups).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("is_callable()");

    // Compile-time string literal: defer to the same lookup as
    // function_exists() — known catalog builtin or user-declared
    // function ⇒ true, else false. Evaluating the literal expression
    // has no side effects, so we skip emit_expr.
    if let ExprKind::StringLiteral(name) = &args[0].kind {
        let known = ctx.functions.contains_key(name)
            || is_supported_builtin_function(name)
            || ctx.function_variant_groups.contains(name);
        let val: i64 = if known { 1 } else { 0 };
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), val);
        return Some(PhpType::Bool);
    }

    // Otherwise evaluate the expression for side effects and decide
    // statically based on its compile-time type.
    let ty = emit_expr(&args[0], emitter, ctx, data);
    let val: i64 = if ty == PhpType::Callable { 1 } else { 0 };
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), val);
    Some(PhpType::Bool)
}
