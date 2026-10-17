//! Purpose:
//! Home of the PHP `srand` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - An alias of `mt_srand()` since PHP 7.1: seeds php's Mersenne Twister, which `mt_rand()`, `rand()`, `shuffle()` and `array_rand()`
//!   then draw from exactly as php does. A null or missing seed draws one from the CSPRNG;
//!   `MT_RAND_PHP` selects php's deprecated legacy twist and raises its deprecation.

builtin! {
    contract: "srand",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Srand,
    ),
}
