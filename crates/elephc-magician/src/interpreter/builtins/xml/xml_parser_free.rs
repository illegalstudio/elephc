//! Purpose:
//! Declares the Magician binding for `xml_parser_free`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Execution forwards to the compiled xml prelude through the shared xml dispatcher.

eval_builtin! { contract: "xml_parser_free", area: Xml, direct: Xml, values: Xml }
