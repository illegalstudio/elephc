//! Purpose:
//! Binds mb_eregi to shared case-insensitive matching with persistent eval references.
//!
//! Called from:
//! - The Magician builtin registry when the managed mbregex provider is enabled.
//!
//! Key details:
//! - No interpreter regex implementation or detached output writeback is involved.

eval_builtin! {
    contract: "mb_eregi",
    area: String,
    direct: none,
    values: none,
}
