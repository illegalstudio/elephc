//! Purpose:
//! Binds mb_ereg_match to the same shared Oniguruma operation used by AOT.
//!
//! Called from:
//! - The Magician builtin registry when the managed mbregex provider is enabled.
//!
//! Key details:
//! - Eval owns no regex implementation or request cache; the generated runtime calls the bridge.

eval_builtin! {
    contract: "mb_ereg_match",
    area: String,
    direct: none,
    values: none,
}
