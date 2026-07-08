//! Purpose:
//! Home of the PHP `abs` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - A `check` hook is required because the return type depends on the argument type:
//!   `Float` input returns `Float`, `Mixed`/Union-containing-Float returns `Mixed`,
//!   and all other inputs return `Int`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "abs",
    area: Math,
    params: [num: Mixed],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Absolute value.",
    php_manual: "https://www.php.net/manual/en/function.abs.php",
}

/// Returns the most precise result type for `abs($num)` based on the argument type.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    Ok(match ty {
        PhpType::Float => PhpType::Float,
        PhpType::Mixed => PhpType::Mixed,
        PhpType::Union(ref members) if members.iter().any(|m| *m == PhpType::Float) => {
            PhpType::Mixed
        }
        PhpType::Union(ref members) if members.iter().any(|m| *m == PhpType::Mixed) => {
            PhpType::Mixed
        }
        _ => PhpType::Int,
    })
}

/// Lowers an `abs` call by dispatching to the shared absolute-value emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::math::lower_abs(ctx, inst)
}
