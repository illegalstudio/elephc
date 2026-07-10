//! Purpose:
//! Home of the PHP `class_alias` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - The check hook always errors: `class_alias()` is only supported as a top-level
//!   statement with literal class names (handled by the AST-level resolver before
//!   reaching the type checker). Any direct call that reaches this hook is rejected.
//! - Arguments are pre-inferred by the registry common path before the hook runs.
//! - `lower` is a thin wrapper over `types::lower_class_alias` (not parameterized).

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "class_alias",
    area: Callables,
    params: [class: Str, alias: Str, autoload: Bool = DefaultSpec::Bool(true)],
    returns: Bool,
    check: check,
    lower: lower,
    summary: "Creates an alias for a class.",
    php_manual: "function.class-alias",
}

/// Rejects any direct `class_alias()` call that reaches the type checker.
///
/// AOT compilation resolves `class_alias()` at the top-level statement stage only.
/// Direct calls in other contexts are not supported and must be rejected here.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Err(CompileError::new(
        cx.span,
        "class_alias() is only supported as a top-level statement with literal class names",
    ))
}

/// Lowers a `class_alias` call by dispatching to the shared class-alias emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::types::lower_class_alias(ctx, inst)
}
