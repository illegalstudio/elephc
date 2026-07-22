//! Purpose:
//! Home of the PHP `round` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `round` is a pure-data builtin whose return type
//!   (`Float`) is fully determined by its declaration.
//! - The second parameter `precision` is optional with a default of `0`, matching
//!   PHP's `round(num, precision = 0)` signature. The registry enforces 1-2 args.

use crate::builtins::spec::DefaultSpec;

builtin! {
    name: "round",
    area: Math,
    params: [num: Float, precision: Int = DefaultSpec::Int(0)],
    returns: Float,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Round,
    ),
    summary: "Rounds a float.",
    php_manual: "https://www.php.net/manual/en/function.round.php",
}
