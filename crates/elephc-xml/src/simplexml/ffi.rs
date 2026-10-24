//! Purpose:
//! Opaque tree and XPath declarations for the managed libxml2 SimpleXML shim.
//!
//! Called from:
//! - The SimpleXML document and selection engine.
//!
//! Key details:
//! - Rust never declares or dereferences libxml2 structure layouts.

use std::ffi::{c_char, c_int, c_void};
pub(super) type Node = *mut c_void;
extern "C" {
    /// Sets scalar node content for DOM text, comment, and processing instruction nodes.
    #[link_name = "elephc_libxml2_v1_sx_content"]
    pub(super) fn content(node: Node, value: *const c_char);
    /// Returns all descendant text for DOM nodeValue/textContent.
    #[link_name = "elephc_libxml2_v1_sx_full_text"]
    pub(super) fn full_text(node: Node) -> *mut c_char;
    /// Serializes one node without a document declaration.
    #[link_name = "elephc_libxml2_v1_sx_dump_node"]
    pub(super) fn dump_node(doc: Node, node: Node) -> *mut c_char;
    /// Copies an entire libxml2 document for PHP clone semantics.
    #[link_name = "elephc_libxml2_v1_sx_copy"]
    pub(super) fn copy(doc: Node) -> Node;
    /// Maps a source node into its document copy without exposing C structure layouts.
    #[link_name = "elephc_libxml2_v1_sx_correspond"]
    pub(super) fn correspond(source: Node, destination: Node, node: Node) -> Node;
    /// Calls the opaque shim parse operation.
    #[link_name = "elephc_libxml2_v1_sx_parse"]
    pub(super) fn parse(data: *const c_char, len: c_int, options: c_int) -> Node;
    /// Calls the opaque shim doc_free operation.
    #[link_name = "elephc_libxml2_v1_sx_doc_free"]
    pub(super) fn doc_free(doc: Node) -> ();
    /// Calls the opaque shim root operation.
    #[link_name = "elephc_libxml2_v1_sx_root"]
    pub(super) fn root(doc: Node) -> Node;
    /// Calls the opaque shim child operation.
    #[link_name = "elephc_libxml2_v1_sx_child"]
    pub(super) fn child(node: Node) -> Node;
    /// Calls the opaque shim next operation.
    #[link_name = "elephc_libxml2_v1_sx_next"]
    pub(super) fn next(node: Node) -> Node;
    /// Calls the opaque shim parent operation.
    #[link_name = "elephc_libxml2_v1_sx_parent"]
    pub(super) fn parent(node: Node) -> Node;
    /// Calls the opaque shim attrs operation.
    #[link_name = "elephc_libxml2_v1_sx_attrs"]
    pub(super) fn attrs(node: Node) -> Node;
    /// Calls the opaque shim type operation.
    #[link_name = "elephc_libxml2_v1_sx_type"]
    pub(super) fn node_type(node: Node) -> c_int;
    /// Calls the opaque shim name operation.
    #[link_name = "elephc_libxml2_v1_sx_name"]
    pub(super) fn name(node: Node) -> *const c_char;
    /// Calls the opaque shim ns operation.
    #[link_name = "elephc_libxml2_v1_sx_ns"]
    pub(super) fn ns(node: Node, prefix: c_int) -> *const c_char;
    /// Calls the opaque shim text operation.
    #[link_name = "elephc_libxml2_v1_sx_text"]
    pub(super) fn text(node: Node) -> *mut c_char;
    /// Calls the opaque shim string_free operation.
    #[link_name = "elephc_libxml2_v1_sx_string_free"]
    pub(super) fn string_free(value: Node) -> ();
    /// Calls the opaque shim dump operation.
    #[link_name = "elephc_libxml2_v1_sx_dump"]
    pub(super) fn dump(doc: Node, node: Node) -> *mut c_char;
    /// Calls the opaque shim unlink operation.
    #[link_name = "elephc_libxml2_v1_sx_unlink"]
    pub(super) fn unlink(node: Node) -> ();
    /// Calls the opaque shim node_free operation.
    #[link_name = "elephc_libxml2_v1_sx_node_free"]
    pub(super) fn node_free(node: Node) -> ();
    /// Calls the opaque shim assign operation.
    #[link_name = "elephc_libxml2_v1_sx_assign"]
    pub(super) fn assign(node: Node, value: *const c_char) -> ();
    /// Calls the opaque shim add operation.
    #[link_name = "elephc_libxml2_v1_sx_add"]
    pub(super) fn add(node: Node, name: *const c_char, value: *const c_char, uri: *const c_char, has_uri: c_int, attribute: c_int, has_value: c_int) -> Node;
    /// Calls the opaque shim ns_defs operation.
    #[link_name = "elephc_libxml2_v1_sx_ns_defs"]
    pub(super) fn ns_defs(node: Node) -> Node;
    /// Calls the opaque shim ns_next operation.
    #[link_name = "elephc_libxml2_v1_sx_ns_next"]
    pub(super) fn ns_next(ns: Node) -> Node;
    /// Calls the opaque shim ns_value operation.
    #[link_name = "elephc_libxml2_v1_sx_ns_value"]
    pub(super) fn ns_value(ns: Node, prefix: c_int) -> *const c_char;
    /// Calls the opaque shim xpath_context operation.
    #[link_name = "elephc_libxml2_v1_sx_xpath_context"]
    pub(super) fn xpath_context(doc: Node, node: Node) -> Node;
    /// Calls the opaque shim xpath_register operation.
    #[link_name = "elephc_libxml2_v1_sx_xpath_register"]
    pub(super) fn xpath_register(ctx: Node, prefix: *const c_char, uri: *const c_char) -> c_int;
    /// Calls the opaque shim xpath_eval operation.
    #[link_name = "elephc_libxml2_v1_sx_xpath_eval"]
    pub(super) fn xpath_eval(ctx: Node, expression: *const c_char) -> Node;
    /// Calls the opaque shim xpath_count operation.
    #[link_name = "elephc_libxml2_v1_sx_xpath_count"]
    pub(super) fn xpath_count(result: Node) -> c_int;
    /// Calls the opaque shim xpath_node operation.
    #[link_name = "elephc_libxml2_v1_sx_xpath_node"]
    pub(super) fn xpath_node(result: Node, index: c_int) -> Node;
    /// Calls the opaque shim xpath_free operation.
    #[link_name = "elephc_libxml2_v1_sx_xpath_free"]
    pub(super) fn xpath_free(result: Node) -> ();
    /// Calls the opaque shim xpath_context_free operation.
    #[link_name = "elephc_libxml2_v1_sx_xpath_context_free"]
    pub(super) fn xpath_context_free(ctx: Node) -> ();
}
