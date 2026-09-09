//! Purpose:
//! Declares the Magician binding for `xmlwriter_set_indent_string`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Execution forwards to the compiled xml prelude through the shared xml dispatcher.

eval_builtin! { contract: "xmlwriter_set_indent_string", area: Xml, direct: Xml, values: Xml }
