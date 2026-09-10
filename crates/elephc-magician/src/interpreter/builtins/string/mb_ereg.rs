//! Purpose:
//! Binds mb_ereg to shared matching with persistent eval output references.
//!
//! Called from:
//! - The Magician builtin registry when the managed mbregex provider is enabled.
//!
//! Key details:
//! - Common argument evaluation retains the reference wrapper before runtime dispatch.

eval_builtin! {
    contract: "mb_ereg",
    area: String,
    direct: none,
    values: none,
}
