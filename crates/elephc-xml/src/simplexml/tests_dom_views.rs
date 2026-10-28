//! Purpose:
//! Verifies native DOM identity, live links and HTML serializer metadata.
//!
//! Called from:
//! - Managed-native SimpleXML bridge tests in CI.
//!
//! Key details:
//! - DOM relation views use the same retained-node ownership as other SimpleXML aliases.
//! - HTML attributes include every namespace and raw-text policy survives wrapper recreation.

use super::*;

/// Retained nodes expose current native links after their siblings or parents are removed.
#[test]
fn simplexml_dom_relations_follow_mutation_and_pin_removed_nodes() {
    let root = parse(b"<r><a/><b/></r>", 0, "", false).unwrap();
    let a = root.select(2, "a", "", false, 0).select(4, "", "", false, 0);
    let b = root.select(2, "b", "", false, 0).select(4, "", "", false, 0);
    assert_eq!(a.select(12, "", "", false, 0).first(), root.first());
    assert_eq!(a.select(14, "", "", false, 0).first(), b.first());
    assert_eq!(b.select(13, "", "", false, 0).first(), a.first());
    root.remove(2, "b", 0);
    assert!(a.select(14, "", "", false, 0).first().is_none());
    for kind in [12, 13, 14] { assert!(b.select(kind, "", "", false, 0).first().is_none()); }
    let c = root.add("c", Some("new"), None, false).unwrap();
    assert_eq!(a.select(14, "", "", false, 0).first(), Some(c));
    assert_eq!(root.select(12, "", "", false, 0).first(), Some(root.doc.raw));
    root.remove(2, "a", 0);
    assert!(a.select(12, "", "", false, 0).first().is_none());
    assert_eq!(unsafe { bytes(ffi::name(b.first().unwrap())) }, b"b");
}

/// Serializer attribute selection includes prefixed attributes while ordinary attributes stay filtered.
#[test]
fn simplexml_dom_attribute_selection_keeps_qualified_names_and_identity() {
    let source = cstring("<r xmlns:p='urn:p' a='1' p:a='2'/>");
    let empty = cstring("");
    let root = unsafe { abi::elephc_xml_sx_load(source.as_ptr(), source.as_bytes().len() as i64, 0, empty.as_ptr(), 0, empty.as_ptr()) };
    assert_ne!(root, 0);
    unsafe {
        let ordinary = abi::elephc_xml_sx_select(root, 1, empty.as_ptr(), empty.as_ptr(), 0, 0);
        let attributes = abi::elephc_xml_sx_select(root, 15, empty.as_ptr(), empty.as_ptr(), 0, 0);
        assert_eq!(abi::elephc_xml_sx_int(ordinary, 2), 1);
        assert_eq!(abi::elephc_xml_sx_int(attributes, 2), 2);
        let first = abi::elephc_xml_sx_item(attributes, 0);
        let same = abi::elephc_xml_sx_item(attributes, 0);
        let second = abi::elephc_xml_sx_item(attributes, 1);
        assert_ne!(abi::elephc_xml_sx_int(first, 12), 0);
        assert_eq!(abi::elephc_xml_sx_int(first, 12), abi::elephc_xml_sx_int(same, 12));
        assert_ne!(abi::elephc_xml_sx_int(first, 12), abi::elephc_xml_sx_int(second, 12));
        assert_eq!(bytes(abi::elephc_xml_sx_string(second, 6)), b"p:a");
        for handle in [ordinary, attributes, first, same, second] { abi::elephc_xml_sx_free(handle); }
    }
    abi::elephc_xml_sx_free(root);
}

/// Native HTML metadata preserves script/style CDATA formatting independently of DOM wrapper lifetime.
#[test]
fn simplexml_dom_html_raw_text_policy_uses_native_parent_names() {
    let empty = cstring("");
    let root_name = cstring("html");
    let script_name = cstring("SCRIPT");
    let text = cstring("a & b");
    let document = abi::elephc_xml_sx_html_document();
    assert_ne!(document, 0);
    unsafe {
        let root = abi::elephc_xml_sx_html_append(document, 1, root_name.as_ptr(), empty.as_ptr());
        let script = abi::elephc_xml_sx_html_append(root, 1, script_name.as_ptr(), empty.as_ptr());
        let raw = abi::elephc_xml_sx_html_append(script, 3, empty.as_ptr(), text.as_ptr());
        let plain = abi::elephc_xml_sx_html_append(root, 3, empty.as_ptr(), text.as_ptr());
        assert_ne!(raw, 0);
        assert_ne!(plain, 0);
        assert_eq!(abi::elephc_xml_sx_int(document, 13), 1);
        assert_eq!(abi::elephc_xml_sx_int(raw, 14), 1);
        assert_eq!(abi::elephc_xml_sx_int(plain, 14), 0);
        for handle in [root, script, raw, plain] { abi::elephc_xml_sx_free(handle); }
    }
    abi::elephc_xml_sx_free(document);
}
