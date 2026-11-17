//! Purpose:
//! Declares the shared native-prelude eval binding for `simplexml_load_file`.
//!
//! Called from:
//! - The Magician builtin registry.
//!
//! Key details:
//! - Evaluation forwards to the host's compiled SimpleXML declarations.

eval_builtin! { contract: "simplexml_load_file", area: Xml, direct: Xml, values: Xml }
