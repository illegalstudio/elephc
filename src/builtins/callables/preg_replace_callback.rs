//! Purpose:
//! Home of the PHP `preg_replace_callback` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `lazy_check: true` is required: `contextual_closure_sig` injects `array<string>` for the
//!   closure's `$matches` parameter BEFORE the closure body is inferred. Pre-inference would
//!   mistype the closure. The check hook controls argument inference order.
//! - The actual check logic lives in the checker submodule
//!   `crate::types::checker::builtins::callables::preg_replace_callback::check`, which also
//!   enforces the arity guard independently (needed for the first-class-callable path).
//! - `lower` is a thin wrapper over `lower_preg_replace_callback` (not parameterized).

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "preg_replace_callback",
    area: Callables,
    params: [pattern: Str, callback: Mixed, subject: Str],
    returns: Str,
    check: check,
    lazy_check: true,
    lower: lower,
    summary: "Performs a regular expression search and replace using a callback.",
    php_manual: "function.preg-replace-callback",
}

/// Delegates to `check_preg_replace_callback_first_class_call`, which controls closure
/// argument inference order and injects `array<string>` as the contextual type for
/// `$matches` before the closure body is inferred.
///
/// That function lives in the checker's callables module (re-exported as `pub(crate)` from
/// `types::checker::builtins`) and is already used for the first-class-callable path; the
/// home simply reuses it so both paths share the same implementation.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::check_preg_replace_callback_first_class_call(
        cx.checker,
        cx.args,
        cx.span,
        cx.env,
    )
}

/// Lowers a `preg_replace_callback` call by dispatching to the shared EIR emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::regex::lower_preg_replace_callback(ctx, inst)
}
