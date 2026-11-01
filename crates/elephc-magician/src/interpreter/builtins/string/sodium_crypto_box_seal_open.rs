//! Purpose:
//! Declarative eval registry entry for `sodium_crypto_box_seal_open`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Shares `sodium_box` with the rest of the sealed-box family.

eval_builtin! {
    contract: "sodium_crypto_box_seal_open",
    area: String,
    direct: SodiumBox,
    values: SodiumBox,
}
