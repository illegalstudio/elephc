//! Purpose:
//! The libxml2 2.15.3 public tree API and the Elephc shim readers the legacy DOM engine calls.
//!
//! Called from:
//! - `crate::dom` and `crate::dom::abi`.
//!
//! Key details:
//! - Rust never declares or dereferences a libxml2 structure layout: struct fields are read
//!   through `elephc_libxml2_v1_sx_*` / `elephc_libxml2_v1_dom_*` shim functions
//!   (`src/native_deps/recipes/libxml2_simplexml*.c`, `libxml2_dom.c`).
//! - Node pointers are opaque `*mut c_void`.

use std::ffi::{c_char, c_int, c_void};

/// An opaque libxml2 node, document or attribute pointer.
pub(crate) type Node = *mut c_void;

extern "C" {
    // -- shim readers (struct fields) --
    #[link_name = "elephc_libxml2_v1_sx_type"]
    pub(crate) fn node_type(node: Node) -> c_int;
    #[link_name = "elephc_libxml2_v1_sx_name"]
    pub(crate) fn name(node: Node) -> *const c_char;
    #[link_name = "elephc_libxml2_v1_sx_ns"]
    pub(crate) fn ns(node: Node, prefix: c_int) -> *const c_char;
    #[link_name = "elephc_libxml2_v1_sx_child"]
    pub(crate) fn child(node: Node) -> Node;
    #[link_name = "elephc_libxml2_v1_sx_next"]
    pub(crate) fn next(node: Node) -> Node;
    #[link_name = "elephc_libxml2_v1_sx_prev"]
    pub(crate) fn previous(node: Node) -> Node;
    #[link_name = "elephc_libxml2_v1_sx_parent"]
    pub(crate) fn parent(node: Node) -> Node;
    #[link_name = "elephc_libxml2_v1_sx_attrs"]
    pub(crate) fn attrs(node: Node) -> Node;
    #[link_name = "elephc_libxml2_v1_sx_string_free"]
    pub(crate) fn string_free(value: Node);
    #[link_name = "elephc_libxml2_v1_sx_parse"]
    pub(crate) fn parse(data: *const c_char, len: c_int, options: c_int, url: *const c_char,
        diagnostic_style: c_int, diagnostics: *mut *mut c_char) -> Node;
    #[link_name = "elephc_libxml2_v1_sx_parse_file"]
    pub(crate) fn parse_file(filename: *const c_char, options: c_int, diagnostics: *mut *mut c_char) -> Node;
    #[link_name = "elephc_libxml2_v1_dom_save"]
    pub(crate) fn save(doc: Node, node: Node, format: c_int, no_decl: c_int, no_empty_tags: c_int,
        length: *mut c_int) -> *mut c_char;
    #[link_name = "elephc_libxml2_v1_dom_node_document"]
    pub(crate) fn node_document(node: Node) -> Node;
    #[link_name = "elephc_libxml2_v1_dom_doc_string"]
    pub(crate) fn doc_string(doc: Node, which: c_int) -> *const c_char;
    #[link_name = "elephc_libxml2_v1_dom_parse_html"]
    pub(crate) fn parse_html(data: *const c_char, len: c_int, filename: *const c_char, options: c_int,
        diagnostics: *mut *mut c_char) -> Node;
    #[link_name = "elephc_libxml2_v1_dom_save_html"]
    pub(crate) fn save_html(doc: Node, node: Node, format: c_int, length: *mut c_int) -> *mut c_char;
    #[link_name = "elephc_libxml2_v1_dom_last"]
    pub(crate) fn last(node: Node) -> Node;
    #[link_name = "elephc_libxml2_v1_dom_link_text_last"]
    pub(crate) fn link_text_last(parent: Node, child: Node);
    #[link_name = "elephc_libxml2_v1_dom_link_text_before"]
    pub(crate) fn link_text_before(reference: Node, child: Node);

    // -- libxml2 public tree API --
    #[link_name = "xmlNewDoc"]
    pub(crate) fn new_doc(version: *const c_char) -> Node;
    #[link_name = "xmlFreeDoc"]
    pub(crate) fn free_doc(doc: Node);
    #[link_name = "xmlFreeNode"]
    pub(crate) fn free_node(node: Node);
    #[link_name = "xmlFreeProp"]
    pub(crate) fn free_prop(attribute: Node);
    #[link_name = "xmlDocGetRootElement"]
    pub(crate) fn root(doc: Node) -> Node;
    #[link_name = "xmlNewDocNode"]
    pub(crate) fn new_element(doc: Node, ns: Node, name: *const c_char, content: *const c_char) -> Node;
    #[link_name = "xmlNewDocText"]
    pub(crate) fn new_text(doc: Node, content: *const c_char) -> Node;
    #[link_name = "xmlNewDocComment"]
    pub(crate) fn new_comment(doc: Node, content: *const c_char) -> Node;
    #[link_name = "xmlNewCDataBlock"]
    pub(crate) fn new_cdata(doc: Node, content: *const c_char, length: c_int) -> Node;
    #[link_name = "xmlNewDocPI"]
    pub(crate) fn new_pi(doc: Node, target: *const c_char, content: *const c_char) -> Node;
    #[link_name = "xmlNewDocProp"]
    pub(crate) fn new_attribute(doc: Node, name: *const c_char, value: *const c_char) -> Node;
    #[link_name = "xmlNewDocFragment"]
    pub(crate) fn new_fragment(doc: Node) -> Node;
    #[link_name = "xmlAddChild"]
    pub(crate) fn add_child(parent: Node, node: Node) -> Node;
    #[link_name = "xmlAddPrevSibling"]
    pub(crate) fn add_prev_sibling(sibling: Node, node: Node) -> Node;
    #[link_name = "xmlUnlinkNode"]
    pub(crate) fn unlink(node: Node);
    #[link_name = "xmlDocCopyNode"]
    pub(crate) fn copy_node(node: Node, doc: Node, extended: c_int) -> Node;
    #[link_name = "xmlNodeGetContent"]
    pub(crate) fn content(node: Node) -> *mut c_char;
    #[link_name = "xmlNodeSetContent"]
    pub(crate) fn set_content(node: Node, content: *const c_char) -> c_int;
    #[link_name = "xmlGetProp"]
    pub(crate) fn get_prop(node: Node, name: *const c_char) -> *mut c_char;
    #[link_name = "xmlHasProp"]
    pub(crate) fn has_prop(node: Node, name: *const c_char) -> Node;
    #[link_name = "xmlSetProp"]
    pub(crate) fn set_prop(node: Node, name: *const c_char, value: *const c_char) -> Node;
}
