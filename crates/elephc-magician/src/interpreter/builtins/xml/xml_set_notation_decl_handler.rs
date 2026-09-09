//! Purpose:
//! Declares the Magician binding for `xml_set_notation_decl_handler`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Execution forwards to the compiled xml prelude through the shared xml dispatcher.

eval_builtin! { contract: "xml_set_notation_decl_handler", area: Xml, direct: Xml, values: Xml }
