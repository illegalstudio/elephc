//! Purpose:
//! Native SimpleXML ownership, selection, and query regressions.
//!
//! Called from:
//! - Managed-native CI with `elephc_xml_native` enabled.
//!
//! Key details:
//! - Uses real managed libxml2 and checks document isolation after clone.

use super::*;

/// Child selectors observe later appends and indexed writes escape raw text.
#[test]
fn simplexml_live_selection_and_mutation() {
    let root = parse(b"<r><a>one</a><a>two</a></r>", 0, "", false).unwrap();
    let items = root.select(2, "a", "", false, 0);
    assert_eq!(items.items().len(), 2);
    assert_eq!(items.write(4, "", 0, "A&B"), 1);
    assert!(root.add("a", Some("three"), None, false).is_some());
    assert_eq!(items.items().len(), 3);
    let first = items.select(4, "", "", false, 0);
    assert_eq!(unsafe { owned_string(ffi::text(first.first().unwrap())) }, "A&B");
    assert_eq!(first.items().len(), 0);
}

/// Clone preserves repeated-node selection while isolating both document and attributes.
#[test]
fn simplexml_clone_owns_an_independent_document() {
    let root = parse(b"<r><a>one</a><a>two</a></r>", 0, "", false).unwrap();
    let items = root.select(2, "a", "", false, 0);
    let clone = items.deep_clone().unwrap();
    assert_eq!(clone.items().len(), 2);
    assert_eq!(clone.write(4, "", 0, "changed"), 1);
    assert_eq!(unsafe { owned_string(ffi::text(items.first().unwrap())) }, "one");
    assert_eq!(unsafe { owned_string(ffi::text(clone.first().unwrap())) }, "changed");
}

/// A wrapper survives parent release, node removal, and replacement of its parent's text.
#[test]
fn simplexml_detached_selection_keeps_storage_alive() {
    let root = parse(b"<r><a><b>kept</b></a></r>", 0, "", false).unwrap();
    let named = root.select(2, "a", "", false, 0);
    let selected = named.with_scope(Scope::Node(named.first().unwrap()));
    root.remove(2, "a", 0);
    drop(root);
    assert_eq!(selected.select(2, "b", "", false, 0).items().len(), 1);
    drop(named);
    assert_eq!(unsafe { string(ffi::name(selected.first().unwrap())) }, "a");
}

/// XPath binds in-scope namespaces and text results map to their containing element.
#[test]
fn simplexml_xpath_namespace_and_text_nodes() {
    let root = parse(b"<r xmlns:p='urn:p'><p:a>one</p:a><a>two</a></r>", 0, "", false).unwrap();
    assert_eq!(root.xpath("//p:a").unwrap().len(), 1);
    root.xpath_ns.borrow_mut().insert("q".into(), "urn:p".into());
    assert_eq!(root.xpath("//q:a/text()").unwrap().len(), 1);
    assert_eq!(root.namespaces(true, true, true), vec![("p".into(), "urn:p".into())]);
    assert!(root.xpath("count(//a)").is_none());
}

/// Missing named ancestors are materialized once for a chained assignment.
#[test]
fn simplexml_materializes_missing_ancestors() {
    let root = parse(b"<r/>", 0, "", false).unwrap();
    let missing = root.select(2, "outer", "", false, 0);
    assert_eq!(missing.write(2, "inner", 0, "value"), 1);
    assert_eq!(missing.nodes().len(), 1);
    assert_eq!(unsafe { owned_string(ffi::text(missing.select(2, "inner", "", false, 0).first().unwrap())) }, "value");
}

/// Indexed aliases keep their node after sibling removal and tolerate repeated unsets.
#[test]
fn simplexml_indexed_alias_survives_removal() {
    let root = parse(b"<r><a>one</a><a>two</a></r>", 0, "", false).unwrap();
    let items = root.select(2, "a", "", false, 0);
    let first = items.select(4, "", "", false, 0);
    items.remove(4, "", -1);
    assert_eq!(items.nodes().len(), 2);
    items.remove(4, "", 0);
    assert_eq!(unsafe { owned_string(ffi::text(first.first().unwrap())) }, "one");
    assert_eq!(unsafe { owned_string(ffi::text(items.first().unwrap())) }, "two");
    first.remove(4, "", 0);
    first.remove(4, "", 0);
    assert_eq!(first.doc.detached.borrow().len(), 1);
}
