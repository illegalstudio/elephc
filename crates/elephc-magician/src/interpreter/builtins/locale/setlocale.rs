//! Purpose:
//! Declares the eval binding for setlocale.
//!
//! Called from:
//! - The declarative eval registry.
//!
//! Key details:
//! - Execution forwards to the host's compiled locale prelude.

eval_builtin! { contract: "setlocale", area: String, direct: Locale, values: Locale }
