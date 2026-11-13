//! Purpose:
//! Declares the shared native-prelude eval binding for `simplexml_import_dom`.
//!
//! Called from:
//! - The Magician builtin registry.
//!
//! Key details:
//! - Evaluation forwards to the host's compiled SimpleXML declarations.

eval_builtin! { contract: "simplexml_import_dom", area: Xml, direct: Xml, values: Xml }
