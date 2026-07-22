//! Purpose:
//! Home of the PHP `function_exists` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `lazy_check: true` so the hook controls inference: it infers the single argument
//!   once and, for a string-literal name, forces resolution of any not-yet-instantiated
//!   declaration or variant group (matching legacy behaviour exactly).
//! - The actual check logic lives in `callables::check_function_exists` (in the checker
//!   module tree) because it accesses checker internals unavailable from here.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "function_exists",
    area: Callables,
    params: [function: Str],
    returns: Bool,
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::FunctionExists,
    ),
    summary: "Returns true if the given function has been defined.",
    php_manual: "function.function-exists",
}

/// Delegates to `check_function_exists` which lives in the checker's callables module.
///
/// The implementation accesses checker internals (`fn_decls`, `functions`,
/// `function_variant_groups`, `canonical_function_name_folded`, `check_function_call`,
/// `ensure_function_variant_group_signature`) that are only accessible from within the
/// `types::checker::builtins` module tree.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::check_function_exists(cx.checker, cx.args, cx.span, cx.env)
}
