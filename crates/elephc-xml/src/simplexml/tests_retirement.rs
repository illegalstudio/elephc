//! Purpose:
//! Bounds native retained storage during repeated writes without invalidating live views.
//!
//! Called from:
//! - Managed-native SimpleXML bridge tests in CI.
//!
//! Key details:
//! - Checks weak owner counts, text views, XPath lists, cursors, and namespace ancestors.

use super::*;

/// A retained child and its attribute survive when a namespace-declaring ancestor dies later.
#[test]
fn simplexml_descendant_first_retirement_preserves_namespace_declarations() {
    let root = parse(b"<r><a><b xmlns:p='urn:p'><p:c p:k='v'/></b></a></r>", 0, "", false).unwrap();
    let b = root.select(2, "a", "", false, 0).select(2, "b", "", false, 0);
    let c = b.select(0, "", "urn:p", false, 0).select(4, "", "", false, 0);
    drop(b);
    assert_eq!(root.select(2, "a", "", false, 0).write(2, "b", 0, "t"), 1);
    assert_eq!(root.write(2, "a", 0, "t2"), 1);
    assert_eq!(root.doc.detached.borrow().len(), 1);
    assert_eq!(unsafe { bytes(ffi::ns(c.first().unwrap(), 0)) }.as_slice(), b"urn:p");
    assert_eq!(unsafe { owned_string(ffi::dump_node(root.doc.raw, c.first().unwrap())) }, "<p:c p:k=\"v\"/>");
    assert_eq!(c.namespaces(false, true, false), vec![(b"p".to_vec(), b"urn:p".to_vec())]);
    drop(c);
    assert!(root.doc.detached.borrow().is_empty());
}

/// An attached appended sibling keeps its source sibling's namespace after that source dies.
#[test]
fn simplexml_sibling_namespace_borrow_survives_provider_removal() {
    let root = parse(b"<r><p:i xmlns:p='urn:p'>one</p:i></r>", 0, "", false).unwrap();
    let first = root.select(0, "", "urn:p", false, 0).select(4, "", "", false, 0);
    let second = first.append_sibling().unwrap();
    drop(first);
    root.select(0, "", "urn:p", false, 0).remove(4, "", 0);
    assert!(root.doc.detached.borrow().is_empty());
    assert_eq!(unsafe { bytes(ffi::ns(second.first().unwrap(), 0)) }.as_slice(), b"urn:p");
    assert_eq!(unsafe { owned_string(ffi::dump_node(root.doc.raw, second.first().unwrap())) }, "<p:i/>");
}

/// Transferred clone roots preserve namespace storage for an already retired child.
#[test]
fn simplexml_clone_child_outlives_removed_clone_namespace_provider() {
    let root = parse(b"<r><a xmlns:p='urn:p'><p:c p:k='v'/></a></r>", 0, "", false).unwrap();
    let copied = root.select(2, "a", "", false, 0).select(4, "", "", false, 0).deep_clone().unwrap();
    let child = copied.select(0, "", "urn:p", false, 0).select(4, "", "", false, 0);
    copied.assign_node(copied.first().unwrap(), "replacement");
    copied.remove(4, "", 0);
    drop(copied);
    assert_eq!(unsafe { owned_string(ffi::dump_node(root.doc.raw, child.first().unwrap())) }, "<p:c p:k=\"v\"/>");
    assert_eq!(child.namespaces(false, true, false), vec![(b"p".to_vec(), b"urn:p".to_vec())]);
    drop(child);
    assert!(root.doc.detached.borrow().is_empty());
}

/// Replacing unobserved text and attribute payloads does not retain one tree per write.
#[test]
fn simplexml_repeated_writes_keep_native_retirement_bounded() {
    let root = parse(b"<r tick='0'><counter>0</counter></r>", 0, "", false).unwrap();
    for index in 0..4096 {
        let value = index.to_string();
        assert_eq!(root.write(2, "counter", 0, &value), 1);
        assert_eq!(root.write(3, "tick", 0, &value), 1);
        assert!(root.doc.detached.borrow().is_empty());
        assert!(root.doc.pins.borrow().len() <= 2);
    }
}

/// One exposed DOM text node survives later writes and releases as soon as its view dies.
#[test]
fn simplexml_old_text_view_retains_only_its_own_replaced_storage() {
    let root = parse(b"<r><counter>old</counter></r>", 0, "", false).unwrap();
    let counter = root.select(2, "counter", "", false, 0).select(4, "", "", false, 0);
    let text = counter.with_scope(Scope::Node(unsafe { ffi::child(counter.first().unwrap()) }));
    for index in 0..1024 { assert_eq!(root.write(2, "counter", 0, index.to_string()), 1); }
    assert_eq!(root.doc.detached.borrow().len(), 1);
    assert_eq!(unsafe { owned_bytes(ffi::full_text(text.first().unwrap())) }.as_slice(), b"old");
    drop(text);
    assert!(root.doc.detached.borrow().is_empty());
}

/// XPath lists and a current cursor retain removed elements without accumulating dead owners.
#[test]
fn simplexml_xpath_and_cursor_pins_outlive_parent_replacement() {
    let root = parse(b"<r><a><n/></a><b/></r>", 0, "", false).unwrap();
    let xpath = root.with_scope(Scope::XPath(Rc::new(root.xpath("//a | //n").unwrap())));
    let mut cursor = iteration::Cursor::new(&root);
    root.assign_node(root.first().unwrap(), "new");
    assert_eq!(root.doc.detached.borrow().len(), 1);
    assert_eq!(unsafe { string(ffi::name(xpath.nodes()[1])) }, "n");
    drop(xpath);
    assert_eq!(unsafe { string(ffi::name(cursor.current().unwrap())) }, "a");
    cursor.advance();
    assert!(cursor.current().is_none());
    assert!(root.doc.detached.borrow().is_empty());
}

/// A twice-detached descendant retains its namespace ancestor, while its clone is independent.
#[test]
fn simplexml_retired_descendants_keep_namespace_owners_until_their_last_pin() {
    let root = parse(b"<r><a xmlns:p='urn:p'><p:b><p:c/></p:b></a></r>", 0, "", false).unwrap();
    let a = root.select(2, "a", "", false, 0).select(4, "", "", false, 0);
    let b = a.select(0, "", "urn:p", false, 0).select(4, "", "", false, 0);
    root.remove(2, "a", 0);
    drop(a);
    b.remove(4, "", 0);
    assert_eq!(root.doc.detached.borrow().len(), 2);
    assert_eq!(unsafe { bytes(ffi::ns(b.first().unwrap(), 0)) }.as_slice(), b"urn:p");
    let copied = b.deep_clone().unwrap();
    drop(b);
    assert!(root.doc.detached.borrow().is_empty());
    assert_eq!(unsafe { bytes(ffi::ns(copied.first().unwrap(), 0)) }.as_slice(), b"urn:p");
    assert_eq!(copied.select(0, "", "urn:p", false, 0).item_count(), 1);
}
