//! Purpose:
//! Qualified DOM attribute lookup, mutation and names on SimpleXML-owned trees.
//!
//! Called from:
//! - Managed-native XML bridge tests in CI.
//!
//! Key details:
//! - Attribute aliases and previously exposed text nodes survive replacement.

use super::*;

/// Hidden literal xmlns attributes still keep their old text alive through DOM writes.
#[test]
fn simplexml_dom_literal_namespace_attribute_preparation_preserves_child_owners() {
    crate::ffi::init_library();
    unsafe {
        let raw = ffi::html_document();
        let empty = cstring("");
        let node = ffi::html_append(raw, std::ptr::null_mut(), 1, cstring("p").as_ptr(), empty.as_ptr());
        let name = cstring("xmlns:p");
        let attr = ffi::html_append(raw, node, 2, name.as_ptr(), cstring("old").as_ptr());
        let root = from_document(raw, "", false).unwrap();
        let text = ffi::child(attr);
        let retained = root.with_scope(Scope::Node(text));
        let mut status = 0;
        let target = ffi::dom_attribute_target(node, name.as_ptr(), cstring("new").as_ptr(), &mut status);
        assert_eq!(status, 1);
        assert_eq!(target, attr);
        assert_eq!(ffi::child(target), text);
        root.assign_node(target, "new");
        assert_eq!(owned_bytes(ffi::full_text(retained.first().unwrap())), b"old");
        assert_eq!(owned_bytes(ffi::text(attr)), b"new");
        assert_eq!(owned_bytes(ffi::dom_attribute_text(node, name.as_ptr())), b"");
    }
}

/// Namespace aliases address one attribute while its previous text stays independently pinned.
#[test]
fn simplexml_dom_attribute_alias_replaces_without_duplicate_or_retiring_live_text() {
    let root = parse(b"<r xmlns:p='urn:p' xmlns:q='urn:p' p:a='old' xml:lang='it'/>", 0, "", false).unwrap();
    let selected = root.select(1, "", "urn:p", false, 0).select(3, "a", "", false, 0);
    let attr = selected.first().unwrap();
    let old_text = root.with_scope(Scope::Node(unsafe { ffi::child(attr) }));
    unsafe {
        assert_eq!(owned_bytes(ffi::dom_attribute_text(root.first().unwrap(), cstring("q:a").as_ptr())), b"old");
        let mut status = 0;
        let target = ffi::dom_attribute_target(root.first().unwrap(), cstring("q:a").as_ptr(), cstring("new&value").as_ptr(), &mut status);
        assert_eq!(status, 1);
        assert_eq!(target, attr);
        root.assign_node(target, "new&value");
        assert_eq!(owned_bytes(ffi::full_text(old_text.first().unwrap())), b"old");
        assert_eq!(owned_bytes(ffi::text(selected.first().unwrap())), b"new&value");
        let dumped = owned_string(ffi::dump_node(root.doc.raw, root.first().unwrap()));
        assert!(dumped.contains("q:a=\"new&amp;value\""));
        assert!(!dumped.contains(" p:a="));
        assert_eq!(root.select(1, "", "urn:p", false, 0).nodes().len(), 1);
    }
}

/// The handle ABI distinguishes qualified DOM names, local SimpleXML names and declarations.
#[test]
fn simplexml_dom_attribute_abi_keeps_namespaces_and_qualified_node_names() {
    let source = cstring("<p:r xmlns:p='urn:p' p:a='1' xml:lang='it'><p:c/></p:r>");
    let empty = cstring("");
    let root = unsafe { abi::elephc_xml_sx_load(source.as_ptr(), source.as_bytes().len() as i64, 0, empty.as_ptr(), 0, empty.as_ptr()) };
    assert_ne!(root, 0);
    unsafe {
        assert_eq!(bytes(abi::elephc_xml_sx_string(root, 6)), b"p:r");
        assert_eq!(bytes(abi::elephc_xml_sx_string(root, 1)), b"r");
        assert_eq!(bytes(abi::elephc_xml_sx_dom_attribute(root, cstring("xmlns:p").as_ptr())), b"urn:p");
        for (name, value) in [("p:a", "3"), ("p:new", "added"), ("xml:lang", "en")] {
            assert_eq!(abi::elephc_xml_sx_mutate(root, 25, cstring(name).as_ptr(), cstring(value).as_ptr(), empty.as_ptr(), 0, 0), 1);
            assert_eq!(bytes(abi::elephc_xml_sx_dom_attribute(root, cstring(name).as_ptr())), value.as_bytes());
        }
        assert_eq!(abi::elephc_xml_sx_mutate(root, 25, cstring("xmlns:p").as_ptr(), cstring("changed").as_ptr(), empty.as_ptr(), 0, 0), 0);
        assert_eq!(bytes(abi::elephc_xml_sx_dom_attribute(root, cstring("xmlns:p").as_ptr())), b"urn:p");
        let children = abi::elephc_xml_sx_select(root, 6, empty.as_ptr(), empty.as_ptr(), 0, 0);
        let child = abi::elephc_xml_sx_item(children, 0);
        assert_eq!(bytes(abi::elephc_xml_sx_string(child, 6)), b"p:c");
        for handle in [child, children, root] { abi::elephc_xml_sx_free(handle); }
    }
}
