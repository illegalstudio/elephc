//! Purpose:
//! Joins the internal INI prelude helper to the shared native request coordinator for eval calls.
//!
//! Called from:
//! - The Magician builtin registry.
//!
//! Key details:
//! - Boxed dispatch preserves string identity and contains PHP exceptions at the native boundary.

eval_builtin! {
    contract: "__elephc_shared_ini",
    area: String,
    direct: none,
    values: none,
}
