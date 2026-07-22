//! Purpose:
//! Home of the PHP `max` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - A `check` hook is required because the return type depends on argument types:
//!   any Float argument widens the result to Float; otherwise the result is Int.
//! - `min_args: 2` enforces the legacy requirement that at least two values be provided.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
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
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Max,
    ),
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
