//! Purpose:
//! Covers literal QName suffixes, empty child namespaces and DOM text replacement.
//!
//! Called from:
//! - Managed-native SimpleXML bridge tests in CI.
//!
//! Key details:
//! - Name and namespace assertions distinguish native identity from serialized declarations.
//! - DOM entity parsing preserves previously exposed child storage.

use super::*;

/// A trailing colon stays in the local name and cannot satisfy a namespaced attribute prefix.
#[test]
fn simplexml_add_keeps_trailing_colon_names_literal() {
    let root = parse(b"<r xmlns:a='urn:a'/>", 0, "", false).unwrap();
    let child = root.add("a:", Some("v"), None, false).unwrap();
    let attr = root.add("b:", Some("1"), None, true).unwrap();
    let qualified = root.add("a:", Some("v"), Some("urn:a"), false).unwrap();
    unsafe {
        assert_eq!(bytes(ffi::name(child)), b"a:");
        assert_eq!(bytes(ffi::name(attr)), b"b:");
        assert_eq!(owned_string(ffi::dump_node(root.doc.raw, child)), "<a:>v</a:>");
        assert_eq!(owned_string(ffi::dump_node(root.doc.raw, qualified)), "<a:a:>v</a:a:>");
    }
    assert_eq!(root.add_checked(b"a:", Some(b"2"), Some(b"urn:a"), true), Err(mutation::AddError::AttributePrefix));
}

/// Explicitly empty declarations never give an element an actual namespace pointer.
#[test]
fn simplexml_add_empty_child_namespace_keeps_unqualified_xpath_identity() {
    let root = parse(b"<r xmlns='urn:default'/>", 0, "", false).unwrap();
    let prefixed = root.add("p:e", Some("w"), Some(""), false).unwrap();
    let child = root.add("c", Some("v"), Some(""), false).unwrap();
    unsafe {
        assert!(ffi::ns(prefixed, 0).is_null());
        assert!(ffi::ns(child, 0).is_null());
        assert_eq!(owned_string(ffi::dump_node(root.doc.raw, prefixed)), "<e xmlns:p=\"\">w</e>");
        assert_eq!(owned_string(ffi::dump_node(root.doc.raw, child)), "<c xmlns=\"\">v</c>");
    }
    assert_eq!(root.xpath("//e").unwrap(), vec![prefixed]);
    assert_eq!(root.xpath("//c").unwrap(), vec![child]);
    assert_eq!(root.select(2, "e", "", false, 0).first(), Some(prefixed));
}

/// Entity-aware and literal writes share the same pin-aware child retirement.
#[test]
fn simplexml_dom_node_value_decodes_entities_without_releasing_live_children() {
    let root = parse(b"<r><a>old</a></r>", 0, "", false).unwrap();
    let node = root.first().unwrap();
    let child = root.with_scope(Scope::Node(unsafe { ffi::child(node) }));
    let text = root.with_scope(Scope::Node(unsafe { ffi::child(child.first().unwrap()) }));
    root.assign_dom_node_value(node, "A&amp;B&#65;");
    unsafe {
        assert_eq!(owned_bytes(ffi::full_text(node)), b"A&BA");
        assert_eq!(owned_bytes(ffi::full_text(child.first().unwrap())), b"old");
        assert_eq!(owned_bytes(ffi::full_text(text.first().unwrap())), b"old");
    }
    root.assign_node(node, "A&amp;B&#65;");
    assert_eq!(unsafe { owned_bytes(ffi::full_text(node)) }, b"A&amp;B&#65;");
}

/// The same ABI exposes document text and preserves unknown entity references in nodeValue.
#[test]
fn simplexml_dom_text_abi_distinguishes_entities_from_literal_content() {
    let source = cstring("<!--c--><r>a<b>c</b><?work data?></r>");
    let empty = cstring("");
    let root = unsafe { abi::elephc_xml_sx_load(source.as_ptr(), source.as_bytes().len() as i64, 0, empty.as_ptr(), 0, empty.as_ptr()) };
    assert_ne!(root, 0);
    unsafe {
        assert_eq!(bytes(abi::elephc_xml_sx_string(root, 7)), b"ac");
        let value = cstring("A&unknown;B");
        assert_eq!(abi::elephc_xml_sx_mutate(root, 26, empty.as_ptr(), value.as_ptr(), empty.as_ptr(), 0, 0), 1);
        assert_eq!(bytes(abi::elephc_xml_sx_string(root, 7)), b"AB");
        assert_eq!(bytes(abi::elephc_xml_sx_string(root, 4)), b"<r>A&unknown;B</r>");
        assert_eq!(abi::elephc_xml_sx_mutate(root, 22, empty.as_ptr(), value.as_ptr(), empty.as_ptr(), 0, 0), 1);
        assert_eq!(bytes(abi::elephc_xml_sx_string(root, 7)), b"A&unknown;B");
    }
    abi::elephc_xml_sx_free(root);
}

/// Literal attribute dimensions reject non-elements while addAttribute retains its parent rule.
#[test]
fn simplexml_named_dimensions_do_not_redirect_non_element_receivers() {
    let root = parse(b"<r><mark a='3'>7<!--kept--><?p kept?></mark></r>", 0, "", false).unwrap();
    let mark = root.select(2, "mark", "", false, 0);
    let attribute = mark.with_scope(Scope::Node(mark.select(3, "a", "", false, 0).first().unwrap()));
    let children = mark.select(6, "", "", false, 0).nodes();
    let comment = mark.with_scope(Scope::Node(children[1]));
    let pi = mark.with_scope(Scope::Node(children[2]));
    for receiver in [&attribute, &comment, &pi] {
        assert_eq!(receiver.write(3, "x", 0, "wrong"), 0);
        assert!(mark.select(3, "x", "", false, 0).first().is_none());
    }
    assert_eq!(unsafe { owned_string(ffi::dump_node(root.doc.raw, root.first().unwrap())) },
        "<r><mark a=\"3\">7<!--kept--><?p kept?></mark></r>");
    let attributes = mark.select(1, "", "", false, 0);
    assert_eq!(attributes.write(3, "z", 0, "w"), 1);
    assert!(attribute.add_checked(b"n", Some(b"v"), None, true).is_ok());
    assert_eq!(unsafe { owned_string(ffi::dump_node(root.doc.raw, root.first().unwrap())) },
        "<r><mark a=\"3\" z=\"w\" n=\"v\">7<!--kept--><?p kept?></mark></r>");
}
