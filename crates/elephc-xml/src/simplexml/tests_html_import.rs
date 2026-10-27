//! Purpose:
//! Literal HTML tree construction and document lease lifetime regressions.
//!
//! Called from:
//! - Managed-native SimpleXML bridge tests in CI.
//!
//! Key details:
//! - No serialization or XML parser participates in DOM import.

use super::*;

/// Appends one node through the same integer-handle ABI used by AOT and eval.
fn append(parent: i64, kind: i64, name: &str, value: &str) -> i64 {
    let name = cstring(name);
    let value = cstring(value);
    let handle = unsafe { abi::elephc_xml_sx_html_append(parent, kind, name.as_ptr(), value.as_ptr()) };
    assert_ne!(handle, 0);
    handle
}

/// Root and descendant leases retain a literal HTML document after builder handles die.
#[test]
fn simplexml_html_import_keeps_literal_attributes_comments_and_independent_leases() {
    let builder = abi::elephc_xml_sx_html_document();
    assert_ne!(builder, 0);
    assert_eq!(abi::elephc_xml_sx_html_root(builder), 0);
    let html = append(builder, 1, "html", "");
    let xmlns = append(html, 2, "xmlns", "http://www.w3.org/1999/xhtml");
    let p = append(html, 1, "p", "");
    let attribute = append(p, 2, "xlink:href", "&literal;");
    let colon = append(p, 2, ":class", "value");
    let comment = append(p, 5, "#comment", " ---- ");
    let text = append(p, 3, "#text", "A&");
    let second_text = append(p, 3, "#text", "B");
    let cdata = append(p, 4, "#cdata-section", "<raw>");
    let pi = append(p, 6, "work", "data");
    let root = abi::elephc_xml_sx_html_root(builder);
    for handle in [builder, html, xmlns, attribute, colon, comment, text, second_text, cdata, pi] {
        abi::elephc_xml_sx_free(handle);
    }
    let dumped = unsafe { string(abi::elephc_xml_sx_string(root, 2)) };
    assert_eq!(dumped, "<html xmlns=\"http://www.w3.org/1999/xhtml\"><p xlink:href=\"&amp;literal;\" :class=\"value\"><!-- ---- -->A&amp;B<![CDATA[<raw>]]><?work data?></p></html>");
    // The child remains valid after the final root lease disappears.
    abi::elephc_xml_sx_free(root);
    assert_eq!(unsafe { string(abi::elephc_xml_sx_string(p, 0)) }, "A&B<raw>");
    abi::elephc_xml_sx_free(p);
    abi::elephc_xml_sx_free(builder);
}

/// Imported names never declare namespaces, and adjacent text nodes keep their identity.
#[test]
fn simplexml_html_import_preserves_namespace_and_node_correspondence() {
    crate::ffi::init_library();
    let raw = unsafe { ffi::html_document() };
    assert!(!raw.is_null());
    let empty = cstring("");
    let html = unsafe { ffi::html_append(raw, std::ptr::null_mut(), 1, cstring("html").as_ptr(), empty.as_ptr()) };
    unsafe { ffi::html_append(raw, html, 2, cstring("xmlns").as_ptr(), cstring("urn:literal").as_ptr()); }
    let a = unsafe { ffi::html_append(raw, html, 3, empty.as_ptr(), cstring("a").as_ptr()) };
    let b = unsafe { ffi::html_append(raw, html, 3, empty.as_ptr(), cstring("b").as_ptr()) };
    let root = from_document(raw, "", false).unwrap();
    assert!(root.namespaces(true, true, true).is_empty());
    assert_eq!(root.xpath("//html").unwrap().len(), 1);
    assert_eq!(unsafe { ffi::next(a) }, b);
    assert_eq!(unsafe { owned_string(ffi::text(a)) }, "a");
}

/// A failed or abandoned import releases a rootless document through its internal lease.
#[test]
fn simplexml_html_import_abandoned_builder_is_safe_to_release() {
    let builder = abi::elephc_xml_sx_html_document();
    let comment = append(builder, 5, "#comment", "before-root");
    assert_eq!(abi::elephc_xml_sx_html_root(builder), 0);
    abi::elephc_xml_sx_free(builder);
    assert_eq!(unsafe { string(abi::elephc_xml_sx_string(comment, 0)) }, "before-root");
    abi::elephc_xml_sx_free(comment);
}
