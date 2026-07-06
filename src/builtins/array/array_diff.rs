//! Purpose:
//! Home of the PHP `array_diff` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - The PHP golden signature is `variadic(&["array"], "arrays")` (one regular `array`
//!   param plus a variadic `arrays`). `min_args: 2` enforces at least two arrays; the variadic
//!   shape (unbounded max) accepts three or more (`a \ b \ c`), matching PHP.
//! - `check` requires every argument to be an indexed or associative array; the result preserves
//!   the first-operand type (`array_diff(a, b, c)` keeps `a`'s shape). A three-plus-array call is
//!   lowered by rewriting it into left-nested two-array calls (see `src/ir_lower/expr/mod.rs`).
//! - `lower` is a thin wrapper over the shared `arrays::lower_array_diff` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "array_diff",
    area: Array,
    params: [array: Mixed],
    variadic: "arrays",
    min_args: 2,
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Computes the difference of arrays.",
    php_manual: "https://www.php.net/manual/en/function.array-diff.php",
}

/// Validates every argument is an array and returns the first operand's (preserved) type.
///
/// Arity (>= 2 args) is pre-validated by `check_arity`. Every argument must be an array
/// (`array_diff(a, b, c)` compares `a` against both `b` and `c`); the result preserves the
/// first-operand array shape. Types are re-inferred here to validate and drive the return type;
/// the registry already inferred every argument once for side effects.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let mut first = None;
    for arg in cx.args {
        let ty = cx.checker.infer_type(arg, cx.env)?;
        if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
            return Err(CompileError::new(
                cx.span,
                &format!("{}() arguments must be arrays", cx.name),
            ));
        }
        if first.is_none() {
            first = Some(ty);
        }
    }
    Ok(first.expect("array_diff requires at least two array arguments"))
}

/// Lowers an `array_diff` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::arrays::lower_array_diff(ctx, inst)
}
