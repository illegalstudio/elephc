//! Purpose:
//! Declares the Magician binding for `libxml_use_internal_errors`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Execution forwards to the compiled xml prelude through the shared xml dispatcher, so
//!   eval code and compiled code share one libxml error state.

eval_builtin! { contract: "libxml_use_internal_errors", area: Xml, direct: Xml, values: Xml }
