//! Purpose:
//! Home of the PHP `define` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` registers a literal name's type in `checker.constants` as a compile-time side
//!   effect; a computed name goes to the run-time constant table instead.
//! - The hook calls `infer_type` on the value argument to obtain its type for registration.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    contract: "define",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Define,
    ),
}

/// Validates that the first argument is a string literal and registers the constant.
///
/// Checks that `constant_name` is a `StringLiteral` expression (AOT requirement);
/// infers the type of `value`; and registers the constant's name→type mapping in
/// `checker.constants` so that subsequent `defined()` and `constant()` calls see it.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let name_str = match &cx.args[0].kind {
        ExprKind::StringLiteral(s) => s.clone(),
        // A computed name lands in the run-time constant table (`crate::dynamic_define_prelude`),
        // which the lowering routes the call to; there is nothing to register at compile time.
        // Checking the call the lowering emits is also what resolves the table's signature: a
        // function no source calls is never resolved, and the lowering only routes to a resolved one.
        _ => {
            let table_call = crate::parser::ast::Expr::new(
                ExprKind::FunctionCall {
                    name: crate::names::Name::unqualified(
                        crate::dynamic_define_prelude::DYNAMIC_CONSTANTS_NAME,
                    ),
                    args: vec![
                        crate::parser::ast::Expr::new(
                            ExprKind::IntLiteral(crate::dynamic_define_prelude::OP_DEFINE),
                            cx.span,
                        ),
                        cx.args[0].clone(),
                        cx.args[1].clone(),
                    ],
                },
                cx.span,
            );
            cx.checker.infer_type(&table_call, cx.env)?;
            return Ok(PhpType::Bool);
        }
    };
    let ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    cx.checker
        .runtime_define_names
        .insert(name_str.trim_start_matches('\\').to_string());
    cx.checker.constants.entry(name_str).or_insert(ty);
    Ok(PhpType::Bool)
}
