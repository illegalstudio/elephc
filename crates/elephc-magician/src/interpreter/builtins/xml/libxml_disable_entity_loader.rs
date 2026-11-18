//! Purpose:
//! Declares the shared native-prelude eval binding for `libxml_disable_entity_loader`.
//!
//! Called from:
//! - The Magician builtin registry.
//!
//! Key details:
//! - Execution forwards to the host's compiled libxml declarations (the DOM prelude's
//!   `Libxml` part) through the shared xml dispatcher.

eval_builtin! { contract: "libxml_disable_entity_loader", area: Xml, direct: Xml, values: Xml }
