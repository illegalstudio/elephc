//! Purpose:
//! Binds `mb_regex_encoding` to the shared mbstring engine and neutral PHP contract.
//!
//! Called from:
//! - The Magician builtin registry.
//!
//! Key details:
//! - Regex settings, validation, and request lifetime belong to the shared engine.

eval_builtin! {
    contract: "mb_regex_encoding",
    area: String,
    direct: none,
    values: none,
}
