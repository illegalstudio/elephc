//! Purpose:
//! Re-exports the authoritative mbstring INI catalog for request-state consumers.
//!
//! Called from:
//! - Shared request configuration, getters, and startup handlers.
//!
//! Key details:
//! - Compiler selection and bridge validation use the same names, indices, and defaults.

pub(super) use elephc_builtin_contract::mbstring_abi::ini::catalog::{
    lookup, Key, DIRECTIVES, REGISTRATION_ORDER,
};
