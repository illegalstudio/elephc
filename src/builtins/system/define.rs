//! Purpose:
//! Home of the PHP `define` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the first argument is a string literal and registers the
//!   constant's type in `checker.constants` as a compile-time side effect.
//! - The hook calls `infer_type` on the value argument to obtain its type for registration.
//! - `lower` delegates to the module-level `lower_define` in `src/codegen/lower_inst/builtins.rs`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    name: "define",
    area: System,
    params: [constant_name: Str, value: Mixed],
    returns: Bool,
    check: check,
    lower: lower,
    summary: "Defines a named constant at compile time.",
}

/// Validates that the first argument is a string literal and registers the constant.
///
/// Checks that `constant_name` is a `StringLiteral` expression (AOT requirement);
/// infers the type of `value`; and registers the constant's name→type mapping in
/// `checker.constants` so that subsequent `defined()` and `constant()` calls see it.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let name_str = match &cx.args[0].kind {
        ExprKind::StringLiteral(s) => s.clone(),
        _ => {
            return Err(CompileError::new(
                cx.span,
                "define() first argument must be a string literal",
            ));
        }
    };
    let ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    cx.checker.constants.entry(name_str).or_insert(ty);
    Ok(PhpType::Bool)
}

/// Lowers a `define` call by delegating to the shared module-level emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::lower_define(ctx, inst)
}
