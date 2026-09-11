//! Purpose:
//! Binds `mb_decode_numericentity` to the shared mbstring engine and neutral PHP contract.
//!
//! Called from:
//! - The Magician builtin registry.
//!
//! Key details:
//! - Encoding semantics and argument defaults remain in the shared engine and contract.

eval_builtin! {
    contract: "mb_decode_numericentity",
    area: String,
    direct: none,
    values: none,
}
