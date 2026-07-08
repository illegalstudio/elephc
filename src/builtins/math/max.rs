//! Purpose:
//! Home of the PHP `max` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - A `check` hook is required because the return type depends on argument types:
//!   any Float argument widens the result to Float; otherwise the result is Int.
//! - `min_args: 2` enforces the legacy requirement that at least two values be provided.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "max",
    area: Math,
    params: [value: Mixed],
    variadic: "values",
    min_args: 2,
    arity_error: "max() requires at least 2 arguments",
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Find highest value.",
    php_manual: "https://www.php.net/manual/en/function.max.php",
}

/// Returns Float when any argument is Float, otherwise returns Int.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let mut has_float = false;
    for arg in cx.args {
        let t = cx.checker.infer_type(arg, cx.env)?;
        if t == PhpType::Float {
            has_float = true;
        }
    }
    if has_float {
        Ok(PhpType::Float)
    } else {
        Ok(PhpType::Int)
    }
}

/// Lowers a `max` call by dispatching to the shared min/max emitter with `want_max = true`.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::math::lower_min_max(ctx, inst, true)
}
