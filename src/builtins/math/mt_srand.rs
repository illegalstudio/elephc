//! Purpose:
//! Home of the PHP `mt_srand` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Seeds php's Mersenne Twister, which `mt_rand()`, `rand()`, `shuffle()` and `array_rand()`
//!   then draw from exactly as php does. A null or missing seed draws one from the CSPRNG;
//!   `MT_RAND_PHP` selects php's deprecated legacy twist and raises its deprecation.

builtin! {
    contract: "mt_srand",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::MtSrand,
    ),
}
