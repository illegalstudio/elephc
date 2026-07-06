//! Purpose:
//! Home of the PHP `call_user_func_array` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `lazy_check: true` so the hook controls all inference: the eager `for arg in args`
//!   loop is the single inference pass, matching legacy behaviour exactly.
//! - The actual check logic lives in `callables::check_call_user_func_array` (in the
//!   checker module tree) because it accesses checker internals unavailable from here.
//! - `lower` is a thin wrapper over `lower_call_user_func_builtin_escape`, parameterized
//!   with the canonical function name.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "call_user_func_array",
    area: Callables,
    params: [callback: Mixed, args: Mixed],
    returns: Mixed,
    check: check,
    lazy_check: true,
    lower: lower,
    summary: "Calls a callback with an array of parameters.",
    php_manual: "function.call-user-func-array",
}

/// Delegates to `check_call_user_func_array` which lives in the checker's callables module.
///
/// The implementation accesses checker internals (callable targets, first-class callable
/// targets, function signatures, extern names, and the full expression type inference
/// machinery) that are only accessible from within the `types::checker::builtins` module tree.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::check_call_user_func_array(cx.checker, cx.args, cx.span, cx.env)
}

/// Lowers a `call_user_func_array` builtin-call escape by dispatching to the shared emitter.
///
/// This path is reached only for the rare truly-dynamic case where the static lowering in
/// `ir_lower` could not resolve the callback; it rejects the instruction with a diagnostic
/// to guide the user toward a statically resolvable form.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::arrays::lower_call_user_func_builtin_escape(ctx, inst, "call_user_func_array")
}
