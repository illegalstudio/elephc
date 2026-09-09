//! Purpose:
//! Declares the Magician binding for `xmlwriter_flush`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Execution forwards to the compiled xml prelude through the shared xml dispatcher.

eval_builtin! { contract: "xmlwriter_flush", area: Xml, direct: Xml, values: Xml }
