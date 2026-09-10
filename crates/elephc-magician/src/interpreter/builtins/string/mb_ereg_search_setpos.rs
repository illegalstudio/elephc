//! Purpose:
//! Binds mb_ereg_search_setpos to the shared mbregex request operation.
//!
//! Called from:
//! - The Magician builtin registry when the managed regex provider is available.
//!
//! Key details:
//! - Shared contracts own PHP signatures; the bridge owns state, captures, and errors.

eval_builtin! {
    contract: "mb_ereg_search_setpos",
    area: String,
    direct: none,
    values: none,
}
