//! Purpose:
//! Home of the PHP `preg_replace_callback` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `lazy_check: true` is required: `contextual_closure_sig` injects `array<string>` for the
//!   closure's `$matches` parameter BEFORE the closure body is inferred. Pre-inference would
//!   mistype the closure. The check hook controls argument inference order.
//! - The actual check logic lives in the checker submodule
//!   `crate::types::checker::builtins::callables::preg_replace_callback::check`, which also
//!   enforces the arity guard independently (needed for the first-class-callable path).

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "preg_replace_callback",
    area: Callables,
    params: [pattern: Str, callback: Mixed, subject: Str],
    returns: Str,
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::with_argument_lowering(
        crate::builtins::semantics::runtime_fn_semantics(
            crate::ir::RuntimeFnId::PregReplaceCallback,
        ),
        crate::builtins::semantics::BuiltinArgumentLowering::PregReplaceCallback,
    ),
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
