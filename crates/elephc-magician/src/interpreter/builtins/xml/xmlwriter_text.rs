//! Purpose:
//! Declares the Magician binding for `xmlwriter_text`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Execution forwards to the compiled xml prelude through the shared xml dispatcher.

eval_builtin! { contract: "xmlwriter_text", area: Xml, direct: Xml, values: Xml }
