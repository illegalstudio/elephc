//! Purpose:
//! Declares the Magician binding for `xml_get_error_code`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Execution forwards to the compiled xml prelude through the shared xml dispatcher.

eval_builtin! { contract: "xml_get_error_code", area: Xml, direct: Xml, values: Xml }
