//! Purpose:
//! Binds mb_convert_variables to the shared live-variable conversion contract.
//!
//! Called from:
//! - Magician's builtin registry and shared runtime dispatcher.
//!
//! Key details:
//! - Direct and variadic arguments retain their caller reference identities.
//! - Native-backed storage traversal is supplied through the protected V6 host.

eval_builtin! {
    contract: "mb_convert_variables",
    area: String,
    direct: none,
    values: none,
}
