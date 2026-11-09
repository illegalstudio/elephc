//! Purpose:
//! Native checks of the legacy DOM engine against libxml2: loading, navigation, stable
//! handles, non-merging text insertion, detached-node ownership and serialization.
//!
//! Called from:
//! - `cargo test -p elephc-xml` with `ELEPHC_XML_LIBXML2_LIB_DIR` pointing at the managed
//!   libxml2 artifact.
//!
//! Key details:
//! - Expected serializations are PHP 8.5.8's (`php -n`) for the same operations.

use super::*;

fn load(xml: &str) -> i64 {
    let document = new_document(b"1.0", b"");
    assert!(load_xml(document, xml.as_bytes(), 0));
    document
}

fn save(document: i64) -> String {
    String::from_utf8(save_xml(&entry(document).unwrap(), None, false, false, false).unwrap()).unwrap()
}

fn node(handle: i64) -> Node {
    entry(handle).unwrap().node
}

/// The same native node always yields the same handle, so PHP keeps one object per node.
#[test]
fn handles_are_stable_per_node() {
    let document = load("<r><a/>text</r>");
    let root = entry(document).map(|doc| handle_for(&doc.doc, unsafe { ffi::root(doc.doc.raw) })).unwrap();
    let first = handle_for(&entry(root).unwrap().doc, child_at(node(root), 0));
    assert_eq!(first, handle_for(&entry(root).unwrap().doc, child_at(node(root), 0)));
    assert_eq!(node_name(node(first)), b"a");
    assert_eq!(node_name(node(document)), b"#document");
    assert_eq!(node_value(node(document)), None);
    assert_eq!(text_content(node(root)), Some(b"text".to_vec()));
}

/// Appending text after text keeps two nodes (php-src links them by hand, no merge), and a
/// removed node survives to be appended again.
#[test]
fn text_insertion_does_not_merge_and_removed_nodes_survive() {
    let document = load("<r>a</r>");
    let doc = entry(document).unwrap();
    let root = Entry { doc: doc.doc.clone(), node: unsafe { ffi::root(doc.doc.raw) } };
    let text = create(&doc, kind::TEXT, b"", b"b").unwrap();
    let text = Entry { doc: doc.doc.clone(), node: text };
    assert_eq!(insert(&root, &text, None).unwrap(), text.node);
    assert_eq!(child_at(root.node, 1), text.node);
    let first = Entry { doc: doc.doc.clone(), node: child_at(root.node, 0) };
    assert_eq!(remove_child(&root, &first).unwrap(), first.node);
    assert_eq!(insert(&root, &first, None).unwrap(), first.node);
    assert_eq!(save(document), "<?xml version=\"1.0\"?>\n<r>ba</r>\n");
}

/// Inserting an ancestor, or a node of another document, fails with php-src's codes.
#[test]
fn insertion_errors_follow_php() {
    let document = load("<r><c/></r>");
    let doc = entry(document).unwrap();
    let root = Entry { doc: doc.doc.clone(), node: unsafe { ffi::root(doc.doc.raw) } };
    let child = Entry { doc: doc.doc.clone(), node: child_at(root.node, 0) };
    assert_eq!(insert(&child, &root, None), Err(error::HIERARCHY_REQUEST));
    let other = load("<o/>");
    let other = entry(other).unwrap();
    let foreign = Entry { doc: other.doc.clone(), node: unsafe { ffi::root(other.doc.raw) } };
    assert_eq!(insert(&root, &foreign, None), Err(error::WRONG_DOCUMENT));
}

/// Attributes keep their node when their value changes, and removal detaches them.
#[test]
fn attributes_round_trip() {
    let document = load("<r a=\"1\"/>");
    let doc = entry(document).unwrap();
    let root = Entry { doc: doc.doc.clone(), node: unsafe { ffi::root(doc.doc.raw) } };
    let before = attribute_at(root.node, 0);
    assert_eq!(set_attribute(&root, b"a", b"2"), before);
    assert_eq!(get_attribute(root.node, b"a"), Some(b"2".to_vec()));
    assert!(remove_attribute(&root, b"a"));
    assert_eq!(get_attribute(root.node, b"a"), None);
    assert!(!remove_attribute(&root, b"a"));
    assert_eq!(save(document), "<?xml version=\"1.0\"?>\n<r/>\n");
}
