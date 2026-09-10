//! Purpose:
//! Binds `mb_convert_kana` to the shared mbstring engine and neutral PHP contract.
//!
//! Called from:
//! - The Magician builtin registry.
//!
//! Key details:
//! - Encoding, Unicode, state, and errors belong to the optional shared engine.

eval_builtin! {
    contract: "mb_convert_kana",
    area: String,
    direct: none,
    values: none,
}
