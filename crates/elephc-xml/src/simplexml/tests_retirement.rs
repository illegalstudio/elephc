//! Purpose:
//! Bounds native retained storage during repeated writes without invalidating live views.
//!
//! Called from:
//! - Managed-native SimpleXML bridge tests in CI.
//!
//! Key details:
//! - Checks weak owner counts, text views, XPath lists, cursors, and namespace ancestors.

use super::*;

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
