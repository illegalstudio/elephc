//! Purpose:
//! Declares the Magician binding for `xml_get_current_byte_index`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Execution forwards to the compiled xml prelude through the shared xml dispatcher.

eval_builtin! { contract: "xml_get_current_byte_index", area: Xml, direct: Xml, values: Xml }
