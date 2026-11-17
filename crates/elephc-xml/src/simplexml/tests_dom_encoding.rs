//! Purpose:
//! Distinguishes DOM whole-document encoding from node-argument document serialization.
//!
//! Called from:
//! - Managed-native SimpleXML bridge tests in CI.
//!
//! Key details:
//! - PHP's node dump retains the declared encoding label while writing UTF-8 bytes.

use super::*;

/// Copies serialized bytes through the same explicit-length shim used by the PHP hex ABI.
fn dump(view: &View, mode: i32) -> Vec<u8> {
    unsafe {
        let mut length = 0;
        let raw = ffi::dump_bytes(view.doc.raw, view.first().unwrap(), mode, &mut length);
        assert!(!raw.is_null());
        let bytes = std::slice::from_raw_parts(raw.cast::<u8>(), length as usize).to_vec();
        ffi::string_free(raw.cast());
        bytes
    }
}

/// A document without an explicit encoding gains UTF-8 only in the node-argument form.
#[test]
fn simplexml_dom_document_node_dump_has_distinct_default_encoding() {
    let root = parse(b"<!--c--><r/>", 0, "", false).unwrap();
    assert_eq!(dump(&root, 2).as_slice(), b"<?xml version=\"1.0\"?>\n<!--c-->\n<r/>\n");
    assert_eq!(dump(&root, 3).as_slice(), b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!--c-->\n<r/>\n");
}

/// An explicit label stays intact while node dumping emits UTF-8 instead of the document codec.
#[test]
fn simplexml_dom_document_node_dump_preserves_label_with_utf8_content() {
    let root = parse(b"<?xml version='1.0' encoding='ISO-8859-1'?><r>caf\xe9</r>", 0, "", false).unwrap();
    assert_eq!(dump(&root, 2).as_slice(), b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>\n<r>caf\xe9</r>\n");
    assert_eq!(dump(&root, 3).as_slice(), b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>\n<r>caf\xc3\xa9</r>\n");
}
