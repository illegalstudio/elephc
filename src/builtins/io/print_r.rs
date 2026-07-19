//! Purpose:
//! Home of the PHP `print_r` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` refines the return type from the literal `$return` flag:
//!   `print_r($v, true)` returns `Str` (the rendered output), `print_r($v)` /
//!   `print_r($v, false)` echo and return `Bool` (true), and a runtime flag returns
//!   `Mixed` (`string|bool`, boxed). The EIR-side return typing lives in
//!   `crate::ir_lower::expr::print_r_builtin_return_type_for_args` and the codegen
//!   dispatch in `debug::lower_print_r` follows the same result type — the three
//!   must stay aligned.
//! - `lower` is a thin wrapper over `debug::lower_print_r` in the EIR backend.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    name: "print_r",
    area: Io,
    params: [value: Mixed, r#return: Bool = DefaultSpec::Bool(false)],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Prints human-readable information about a variable.",
    php_manual: "function.print-r",
}

/// Refines `print_r`'s return type from the `$return` flag: a literal `true` selects
/// return mode (`Str`), a literal `false` (or an omitted flag) keeps PHP's echo mode
/// (`Bool`, always true), and a runtime flag yields boxed `Mixed` (`string|bool`)
/// because the mode is only selected at run time.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    match cx.args.get(1) {
        Some(flag) => match &flag.kind {
            ExprKind::BoolLiteral(true) => Ok(PhpType::Str),
            ExprKind::BoolLiteral(false) => Ok(PhpType::Bool),
            _ => Ok(PhpType::Mixed),
        },
        None => Ok(PhpType::Bool),
    }
}

/// Lowers a `print_r` call by dispatching to the shared debug emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::debug::lower_print_r(ctx, inst)
}
