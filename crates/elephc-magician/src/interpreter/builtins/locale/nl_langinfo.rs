//! Purpose:
//! Declares the eval binding for nl_langinfo.
//!
//! Called from:
//! - The declarative eval registry.
//!
//! Key details:
//! - Execution forwards to the host's compiled locale prelude.

eval_builtin! { contract: "nl_langinfo", area: String, direct: Locale, values: Locale }
