//! Purpose:
//! Home of the PHP `in_array` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the second argument is an array and returns `Bool`.
//! - The optional `strict` param (min=2, max=3) is accepted: elephc's element comparison is
//!   value/byte-exact, so it already yields strict semantics for a needle whose type matches the
//!   array element type (the supported case). The emitter evaluates but ignores the flag value.
//! - `lower` is a thin wrapper over the shared `arrays::lower_in_array` emitter.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "in_array",
    area: Array,
    params: [needle: Mixed, haystack: Mixed, strict: Bool = DefaultSpec::Bool(false)],
    returns: Bool,
    check: check,
    lower: lower,
    summary: "Checks if a value exists in an array.",
    php_manual: "https://www.php.net/manual/en/function.in-array.php",
}

/// Validates that the second argument is an array and returns `Bool`.
///
/// Infers all arguments (including the optional `strict` flag, so its type/side effects are
/// checked) and validates that `haystack` is an array. Returns the `Bool` return type.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    let arr_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if let Some(strict) = cx.args.get(2) {
        cx.checker.infer_type(strict, cx.env)?;
    }
    if !matches!(arr_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Err(CompileError::new(
            cx.span,
            "in_array() second argument must be array",
        ));
    }
    Ok(PhpType::Bool)
}

/// Lowers an `in_array` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::arrays::lower_in_array(ctx, inst)
}
