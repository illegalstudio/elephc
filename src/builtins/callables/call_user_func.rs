//! Purpose:
//! Home of the PHP `call_user_func` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `lazy_check: true` so the hook controls all inference: the eager `for arg in args`
//!   loop is the single inference pass, matching legacy behaviour exactly.
//! - The actual check logic lives in `callables::check_call_user_func` (in the checker
//!   module tree) because it accesses checker internals unavailable from here.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "call_user_func",
    area: Callables,
    params: [callback: Mixed],
    variadic: "args",
    returns: Mixed,
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CallUserFunc,
    ),
    summary: "Calls a callback with the given arguments.",
    php_manual: "function.call-user-func",
}

/// Delegates to `check_call_user_func` which lives in the checker's callables module.
///
/// The implementation accesses checker internals (callable targets, first-class callable
/// targets, function signatures, extern names, and the full expression type inference
/// machinery) that are only accessible from within the `types::checker::builtins` module tree.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::check_call_user_func(cx.checker, cx.args, cx.span, cx.env)
}
