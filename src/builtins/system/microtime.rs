//! Purpose:
//! Home of the PHP `microtime` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` inspects the literal value of the `as_float` argument to refine the return
//!   type: `true` → `Float`, `false` → `Str`, non-literal → `Union(Str, Float)`.
//!   The registry's common path pre-infers arguments; the hook must not call `infer_type`.
//! - `lower` is a thin wrapper over the shared `system::lower_microtime` emitter.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    name: "microtime",
    area: System,
    params: [as_float: Bool = DefaultSpec::Bool(false)],
    arity_error: "microtime() takes 0 or 1 arguments",
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Returns the current Unix timestamp with microseconds.",
}

/// Refines the return type of `microtime` based on the literal value of `as_float`.
///
/// Returns `Float` when `as_float` is the literal `true`, `Str` when it is the literal
/// `false` or absent, and `Union(Str, Float)` for any non-literal expression.
/// The registry pre-infers arguments, so this hook must not call `infer_type`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(match cx.args.first() {
        Some(arg) => match &arg.kind {
            ExprKind::BoolLiteral(true) => PhpType::Float,
            ExprKind::BoolLiteral(false) => PhpType::Str,
            _ => cx.checker.normalize_union_type(vec![PhpType::Str, PhpType::Float]),
        },
        None => PhpType::Str,
    })
}

/// Lowers a `microtime` call by dispatching to the shared system emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::system::lower_microtime(ctx, inst)
}
