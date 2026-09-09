//! Purpose:
//! Replays the namespace blocks of the PHP `XMLWriter` probe corpus: declaration
//! ordering, re-declaration on children, prefix conflicts, empty prefixes and URIs, and
//! the destroyed-namespace-stack state.
//!
//! Called from:
//! - `cargo test -p elephc-xml --lib writer` through Rust's test harness.
//!
//! Key details:
//! - Declarations queue on a LIFO stack and are emitted when the start tag closes, so
//!   they trail explicit attributes in reverse registration order.

use super::{indented, take, writer};

/// The namespace document of probe3.
#[test]
fn namespace_document_matches_php() {
    let mut w = indented();
    assert!(w.start_element_ns(Some(b"p"), b"root", Some(b"urn:p")));
    assert!(w.write_attribute_ns(Some(b"p"), b"a", Some(b"urn:p"), b"v"));
    assert!(w.write_attribute_ns(None, b"b", Some(b"urn:b"), b"v2"));
    assert!(w.start_attribute_ns(Some(b"q"), b"c", Some(b"urn:q")));
    assert!(w.write_string(b"v3"));
    assert!(w.end_attribute());
    assert!(w.start_element_ns(None, b"child", Some(b"urn:c")));
    assert!(w.end_element());
    assert!(w.start_element_ns(Some(b"p"), b"child2", None));
    assert!(w.end_element());
    assert!(w.write_element_ns(Some(b"p"), b"e", Some(b"urn:p"), Some(b"content")));
    assert!(w.write_element_ns(None, b"e2", None, None));
    assert!(w.write_element_ns(Some(b"p"), b"e3", Some(b"urn:p"), None));
    assert!(w.end_element());
    assert_eq!(
        take(&mut w),
        "<p:root p:a=\"v\" b=\"v2\" q:c=\"v3\" xmlns:q=\"urn:q\" xmlns=\"urn:b\" xmlns:p=\"urn:p\">\n <child xmlns=\"urn:c\"/>\n <p:child2/>\n <p:e xmlns:p=\"urn:p\">content</p:e>\n <e2/>\n <p:e3 xmlns:p=\"urn:p\"/>\n</p:root>\n"
    );
}

/// The engine accepts odd prefixes and empty URIs; only empty local names fail.
#[test]
fn unvalidated_namespace_names() {
    let mut w = writer();
    assert!(w.start_element_ns(Some(b"p q"), b"a", Some(b"u")));
    assert!(w.end_element());
    assert!(!w.start_element_ns(Some(b"p"), b"", Some(b"u")));
    assert!(w.start_element_ns(Some(b"1p"), b"a", Some(b"u")));
    assert!(w.end_element());
    assert!(w.start_element_ns(None, b"a", Some(b"u")));
    assert!(w.end_element());
    assert!(w.start_element(b"r"));
    assert!(w.start_attribute_ns(Some(b"p q"), b"a", Some(b"u")));
    assert!(w.end_attribute());
    assert!(!w.start_attribute_ns(Some(b"p"), b"", Some(b"u")));
    assert!(w.start_attribute_ns(Some(b"p"), b"a", None));
    assert!(w.end_attribute());
    assert!(w.start_attribute_ns(None, b"a", None));
    assert!(w.end_attribute());
    assert!(w.write_attribute_ns(Some(b"p q"), b"a", Some(b"u"), b"v"));
    assert!(w.write_attribute_ns(Some(b"p"), b"a", None, b"v"));
    assert!(w.write_attribute_ns(None, b"a", None, b"v"));
    assert!(w.write_element_ns(Some(b"p q"), b"a", Some(b"u"), Some(b"v")));
    assert!(w.write_element_ns(Some(b"p"), b"a", None, Some(b"v")));
    assert!(w.write_element_ns(None, b"a", None, Some(b"v")));
    assert!(w.write_element_ns(None, b"a", None, None));
    assert!(w.end_element());
    assert_eq!(
        take(&mut w),
        "<p q:a xmlns:p q=\"u\"/><1p:a xmlns:1p=\"u\"/><a xmlns=\"u\"/><r p q:a=\"\" p:a=\"\" a=\"\" p q:a=\"v\" p:a=\"v\" a=\"v\" xmlns:p q=\"u\"><p q:a xmlns:p q=\"u\">v</p q:a><p:a>v</p:a><a>v</a><a/></r>"
    );
}

/// A child re-declares the prefix its parent declared; a null URI declares nothing;
/// an attribute started after content fails (its queued declaration is never written).
#[test]
fn child_redeclares_parent_prefix() {
    let mut w = writer();
    assert!(w.start_element_ns(Some(b"p"), b"r", Some(b"u")));
    assert!(w.start_element_ns(Some(b"p"), b"c", Some(b"u")));
    assert!(w.end_element());
    assert!(w.start_element_ns(Some(b"p"), b"d", None));
    assert!(w.end_element());
    assert!(!w.start_attribute_ns(Some(b"p"), b"late", Some(b"u")));
    assert!(!w.end_attribute());
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<p:r xmlns:p=\"u\"><p:c xmlns:p=\"u\"/><p:d/></p:r>");

    let mut w = writer();
    assert!(w.start_element_ns(None, b"r", Some(b"u")));
    assert!(w.start_element_ns(None, b"c", Some(b"u")));
    assert!(w.end_element());
    assert!(w.start_element_ns(None, b"d", Some(b"v")));
    assert!(w.end_element());
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<r xmlns=\"u\"><c xmlns=\"u\"/><d xmlns=\"v\"/></r>");
}

/// Attributes sharing the element's prefix are not re-declared; another URI under the
/// same prefix is refused; `xml:` and `xmlns:` attributes are plain attributes.
#[test]
fn attribute_prefix_rules() {
    let mut w = writer();
    assert!(w.start_element_ns(Some(b"p"), b"r", Some(b"u")));
    assert!(w.write_attribute_ns(Some(b"p"), b"a", Some(b"u"), b"1"));
    assert!(w.write_attribute_ns(Some(b"p"), b"b", Some(b"u"), b"2"));
    assert!(!w.write_attribute_ns(Some(b"p"), b"c", Some(b"w"), b"3"));
    assert!(w.write_attribute_ns(Some(b"xml"), b"lang", None, b"en"));
    assert!(w.write_attribute_ns(Some(b"xmlns"), b"q", None, b"urn:q"));
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<p:r p:a=\"1\" p:b=\"2\" xml:lang=\"en\" xmlns:q=\"urn:q\" xmlns:p=\"u\"/>");

    let mut w = writer();
    assert!(w.start_element(b"r"));
    assert!(w.write_attribute_ns(Some(b"xmlns"), b"q", Some(b"ignored"), b"urn:q"));
    assert!(w.write_attribute_ns(None, b"xmlns", Some(b"ignored2"), b"urn:d"));
    assert!(w.end_element());
    assert_eq!(
        take(&mut w),
        "<r xmlns:q=\"urn:q\" xmlns=\"urn:d\" xmlns=\"ignored2\" xmlns:xmlns=\"ignored\"/>"
    );
}

/// Prefix conflicts on one element and re-declaration on a child.
#[test]
fn prefix_conflicts() {
    let mut w = writer();
    assert!(w.start_element(b"r"));
    assert!(w.write_attribute_ns(Some(b"p"), b"a", Some(b"u"), b"1"));
    assert!(!w.write_attribute_ns(Some(b"p"), b"b", Some(b"v"), b"2"));
    assert!(w.write_attribute_ns(Some(b"p"), b"c", Some(b"u"), b"3"));
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<r p:a=\"1\" p:c=\"3\" xmlns:p=\"u\"/>");

    let mut w = writer();
    assert!(w.start_element_ns(Some(b"p"), b"r", Some(b"u")));
    assert!(!w.write_attribute_ns(Some(b"p"), b"b", Some(b"v"), b"2"));
    assert!(w.start_attribute_ns(Some(b"p"), b"c", Some(b"u")));
    assert!(w.end_attribute());
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<p:r p:c=\"\" xmlns:p=\"u\"/>");

    let mut w = writer();
    assert!(w.start_element_ns(Some(b"p"), b"r", Some(b"u")));
    assert!(w.start_element_ns(Some(b"q"), b"c", Some(b"w")));
    assert!(w.write_attribute_ns(Some(b"p"), b"b", Some(b"u"), b"2"));
    assert!(w.end_element());
    assert!(w.end_element());
    assert_eq!(
        take(&mut w),
        "<p:r xmlns:p=\"u\"><q:c p:b=\"2\" xmlns:p=\"u\" xmlns:q=\"w\"/></p:r>"
    );
}

/// Ending an element on an empty stack destroys the namespace stack for good.
#[test]
fn end_element_on_empty_stack_destroys_namespaces() {
    let mut w = writer();
    assert!(!w.end_element());
    assert!(!w.start_element_ns(Some(b"p"), b"r", Some(b"u")));
    assert!(!w.write_attribute_ns(Some(b"p"), b"a", Some(b"u"), b"1"));
    assert!(!w.end_element());
    assert!(w.start_element(b"z"));
    assert_eq!(take(&mut w), "<z");

    let mut w = writer();
    assert!(!w.end_element());
    assert!(w.start_element(b"r"));
    assert!(!w.end_element());
    assert!(!w.start_element_ns(None, b"r", Some(b"u")));
    assert_eq!(take(&mut w), "<r");
}
