//! Purpose:
//! Home of the PHP `array_splice` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - The golden signature is `first_param_ref(optional(["array","offset","length"], required=2, [null]))`:
//!   3 params, `array` by-ref, `length` optional with default null, arity 2-3. The `ref` marker
//!   is mandatory — it is what makes by-reference mutation lower correctly (ir_lower reads
//!   `ref_params` from the registry sig).
//! - `check` reproduces the legacy rule: `Mixed`/`Union` first arg yields `Mixed`; `Array`
//!   or `AssocArray` yields the first-arg type; any other type is an error. All remaining
//!   args are inferred for side effects.
//! - `lower` is a thin wrapper over the shared `arrays::lower_array_splice` emitter.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "array_splice",
    area: Array,
    params: [ref array: Mixed, offset: Int, length: Mixed = DefaultSpec::Null],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Removes a portion of the array and replaces it with something else.",
    php_manual: "https://www.php.net/manual/en/function.array-splice.php",
}

/// Returns the result type for an `array_splice` call.
///
/// Arity (2 or 3 args) is pre-validated by the registry. The first argument is re-inferred
/// to drive the return type; remaining arguments are inferred for side effects. `Mixed` or
/// `Union` first arguments yield `Mixed` (opaque path); `Array`/`AssocArray` yield the
/// first-arg type; any other type is a compile error.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    for arg in &cx.args[1..] {
        cx.checker.infer_type(arg, cx.env)?;
    }
    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        return Ok(PhpType::Mixed);
    }
    if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Err(CompileError::new(
            cx.span,
            &format!("{}() first argument must be array", cx.name),
        ));
    }
    Ok(ty)
}

/// Lowers an `array_splice` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::arrays::lower_array_splice(ctx, inst)
}
