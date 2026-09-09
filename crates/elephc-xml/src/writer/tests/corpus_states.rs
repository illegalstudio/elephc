//! Purpose:
//! Replays the state-machine edge cases of the PHP `XMLWriter` probe corpus: return
//! values per call on an empty or mis-ordered writer, the empty-stack namespace-stack
//! destruction, `end_document` from every open state, `full_end_element`, and the PI /
//! CDATA / comment transitions from attribute and start-tag states.
//!
//! Called from:
//! - `cargo test -p elephc-xml --lib writer` through Rust's test harness.
//!
//! Key details:
//! - Every `assert!(!...)` below is a `bool(false)` PHP printed for the same call.

use super::{indented, take, writer};

/// Every `end_*` call on an empty writer, in PHP's order and with PHP's results.
#[test]
fn end_calls_on_empty_writer() {
    let mut w = writer();
    assert!(!w.end_element());
    assert!(!w.end_attribute());
    assert!(!w.end_cdata());
    assert!(!w.end_comment());
    assert!(w.end_pi());
    assert!(w.end_dtd());
    assert!(!w.end_dtd_element());
    assert!(!w.end_dtd_attlist());
    assert!(!w.end_dtd_entity());
    assert!(w.end_document());
    assert_eq!(take(&mut w), "\n");
}

/// A declaration is refused while an element is open; closing twice fails once.
#[test]
fn declaration_inside_element_and_double_end() {
    let mut w = writer();
    assert!(w.start_element(b"a"));
    assert!(!w.start_document(None, None, None));
    assert!(w.end_element());
    assert!(!w.end_element());
    assert!(w.end_document());
    assert_eq!(take(&mut w), "<a/>\n");
}

/// Elements may start inside CDATA sections and CDATA may open at the root.
#[test]
fn cdata_at_root_and_element_inside_cdata() {
    let mut w = writer();
    assert!(w.start_cdata());
    assert!(w.start_element(b"a"));
    assert!(w.start_cdata());
    assert!(w.end_cdata());
    assert_eq!(take(&mut w), "<![CDATA[<a><![CDATA[]]>");
}

/// Top-level text and raw content are written verbatim around elements.
#[test]
fn top_level_text_and_raw() {
    let mut w = writer();
    assert!(w.write_string(b"top level text"));
    assert!(w.write_raw(b"raw"));
    assert!(w.start_element(b"a"));
    assert!(w.end_element());
    assert!(w.write_string(b"after root"));
    assert!(w.start_element(b"b"));
    assert_eq!(take(&mut w), "top level textraw<a/>after root<b");
}

/// Elements open inside a comment; the comment cannot then be closed.
#[test]
fn element_inside_comment() {
    let mut w = writer();
    assert!(w.start_comment());
    assert!(w.start_element(b"a"));
    assert!(!w.end_comment());
    assert_eq!(take(&mut w), "<!--<a");
}

/// Elements cannot open inside a PI.
#[test]
fn element_inside_pi_fails() {
    let mut w = writer();
    assert!(w.start_pi(b"p"));
    assert!(!w.start_element(b"a"));
    assert!(w.end_pi());
    assert_eq!(take(&mut w), "<?p?>");
}

/// A child element closes the open attribute, after which `end_attribute` fails.
#[test]
fn child_element_closes_attribute() {
    let mut w = writer();
    assert!(w.start_element(b"a"));
    assert!(w.start_attribute(b"x"));
    assert!(w.start_element(b"b"));
    assert!(!w.end_attribute());
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<a x=\"\"><b/>");
}

/// Attributes started one after another close each other.
#[test]
fn attributes_close_each_other() {
    let mut w = writer();
    assert!(w.start_element(b"a"));
    assert!(w.start_attribute(b"x"));
    assert!(w.write_attribute(b"y", b"1"));
    assert!(w.start_attribute(b"z"));
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<a x=\"\" y=\"1\" z=\"\"/>");

    let mut w = writer();
    assert!(w.start_element(b"r"));
    assert!(w.start_attribute(b"a"));
    assert!(w.start_attribute(b"b"));
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<r a=\"\" b=\"\"/>");
}

/// An element may open inside a bare DOCTYPE; DTD declarations then fail.
#[test]
fn element_inside_doctype() {
    let mut w = writer();
    assert!(w.start_dtd(b"a", None, None));
    assert!(w.start_element(b"x"));
    assert!(!w.start_dtd_element(b"e"));
    assert!(!w.start_dtd_attlist(b"at"));
    assert!(!w.end_dtd_element());
    assert!(w.end_dtd());
    assert_eq!(take(&mut w), "<!DOCTYPE a<x");

    let mut w = writer();
    assert!(w.start_element(b"x"));
    assert!(!w.start_dtd(b"a", None, None));
    assert!(!w.write_dtd(b"b", None, None, None));
    assert_eq!(take(&mut w), "<x");
}

/// The engine accepts any non-empty element name (validation is PHP's job).
#[test]
fn engine_accepts_unvalidated_names() {
    let mut w = writer();
    assert!(!w.start_element(b""));
    assert!(w.start_element(b"a:b"));
    assert!(w.start_element(b"a-b"));
    assert!(w.start_element(b"ok"));
    assert_eq!(take(&mut w), "<a:b><a-b><ok");
}

/// The `xml` PI target is reserved; comments accept double hyphens.
#[test]
fn reserved_pi_target_and_comment_hyphens() {
    let mut w = writer();
    assert!(w.start_element(b"a"));
    assert!(w.write_attribute(b"ok", b"v"));
    assert!(w.start_pi(b"ok"));
    assert!(w.end_pi());
    assert!(!w.write_pi(b"xml", b"x"));
    assert!(!w.start_pi(b"XmL"));
    assert!(w.write_comment(b"a--b"));
    assert!(w.write_comment(b"a-"));
    assert!(!w.start_dtd(b"", None, None));
    assert!(w.write_element_ns(Some(b"p"), b"e", Some(b""), Some(b"c")));
    assert!(w.write_element_ns(Some(b""), b"e", Some(b"urn"), Some(b"c")));
    assert!(w.start_element_ns(Some(b"p"), b"e", Some(b"")));
    assert!(w.write_attribute_ns(Some(b"p"), b"e", Some(b""), b"v"));
    assert!(w.write_string(b"a"));
    assert_eq!(
        take(&mut w),
        "<a ok=\"v\"><?ok?><!--a--b--><!--a---><p:e xmlns:p=\"\">c</p:e><:e xmlns:=\"urn\">c</:e><p:e p:e=\"v\" xmlns:p=\"\">a"
    );
}

/// `end_document` from every kind of open node.
#[test]
fn end_document_from_open_states() {
    let mut w = writer();
    assert!(w.start_document(None, None, None));
    assert!(w.start_dtd(b"r", None, None));
    assert!(w.start_dtd_element(b"e"));
    assert!(w.end_document());
    assert_eq!(take(&mut w), "<?xml version=\"1.0\"?>\n<!DOCTYPE r [<!ELEMENT e>]>\n");

    let mut w = writer();
    assert!(w.start_document(None, None, None));
    assert!(w.start_element(b"r"));
    assert!(w.start_comment());
    assert!(w.end_document());
    assert_eq!(take(&mut w), "<?xml version=\"1.0\"?>\n<r><!----></r>\n");

    let mut w = writer();
    assert!(w.start_document(None, None, None));
    assert!(w.start_element(b"r"));
    assert!(w.start_pi(b"p"));
    assert!(w.end_document());
    assert_eq!(take(&mut w), "<?xml version=\"1.0\"?>\n<r><?p?></r>\n");

    let mut w = writer();
    assert!(w.start_document(None, None, None));
    assert!(w.start_element(b"r"));
    assert!(w.start_cdata());
    assert!(w.end_document());
    assert_eq!(take(&mut w), "<?xml version=\"1.0\"?>\n<r><![CDATA[]]></r>\n");

    let mut w = writer();
    assert!(w.start_document(None, None, None));
    assert!(w.start_dtd(b"r", None, None));
    assert!(w.start_dtd_entity(b"e", true));
    assert!(w.end_document());
    assert_eq!(take(&mut w), "<?xml version=\"1.0\"?>\n<!DOCTYPE r [<!ENTITY % e>]>\n");

    let mut w = writer();
    assert!(w.start_document(None, None, None));
    assert!(w.start_dtd(b"r", None, None));
    assert!(w.start_dtd_attlist(b"a"));
    assert!(w.end_document());
    assert_eq!(take(&mut w), "<?xml version=\"1.0\"?>\n<!DOCTYPE r [<!ATTLIST a>]>\n");

    let mut w = indented();
    assert!(w.start_document(None, None, None));
    assert!(w.start_dtd(b"r", None, None));
    assert!(w.start_dtd_element(b"e"));
    assert!(w.write_string(b"ANY"));
    assert!(!w.start_dtd_attlist(b"a"));
    assert!(w.end_document());
    assert_eq!(take(&mut w), "<?xml version=\"1.0\"?>\n<!DOCTYPE r [\n <!ELEMENT e ANY>\n]>\n");
}

/// `full_end_element` from attribute, start-tag and text states.
#[test]
fn full_end_element_states() {
    let mut w = writer();
    assert!(w.start_element(b"a"));
    assert!(w.start_attribute(b"x"));
    assert!(w.full_end_element());
    assert!(!w.full_end_element());
    assert_eq!(take(&mut w), "<a x=\"\"></a>");

    let mut w = indented();
    assert!(w.start_element(b"a"));
    assert!(w.start_element(b"b"));
    assert!(w.full_end_element());
    assert!(w.start_element(b"c"));
    assert!(w.write_string(b"t"));
    assert!(w.full_end_element());
    assert!(w.full_end_element());
    assert_eq!(take(&mut w), "<a>\n <b></b>\n <c>t</c>\n</a>\n");
}

/// Nothing nests inside a comment except text.
#[test]
fn nothing_nests_inside_comment() {
    let mut w = writer();
    assert!(w.start_comment());
    assert!(!w.start_comment());
    assert!(!w.write_comment(b"x"));
    assert!(!w.start_pi(b"p"));
    assert!(!w.start_cdata());
    assert!(w.write_string(b"t"));
    assert!(w.end_comment());
    assert_eq!(take(&mut w), "<!--t-->");
}

/// Inside an open attribute, CDATA/raw/comment/PI close the attribute and the tag.
#[test]
fn content_inside_attribute() {
    let mut w = writer();
    assert!(w.start_element(b"r"));
    assert!(w.start_attribute(b"a"));
    assert!(w.write_cdata(b"c"));
    assert!(w.write_raw(b"<r>"));
    assert!(w.write_comment(b"c"));
    assert!(w.start_pi(b"p"));
    assert!(!w.end_attribute());
    assert!(!w.end_element());
    assert_eq!(take(&mut w), "<r a=\"\"><![CDATA[c]]><r><!--c--><?p");

    let mut w = writer();
    assert!(w.start_element(b"a"));
    assert!(w.start_attribute(b"x"));
    assert!(w.write_raw(b"r&\""));
    assert!(w.end_attribute());
    assert!(w.write_raw(b"R"));
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<a x=\"r&\"\">R</a>");
}

/// PI, CDATA, text, raw, comment and element transitions from the attribute and
/// start-tag states with indentation.
#[test]
fn transitions_with_indent() {
    let mut w = indented();
    assert!(w.start_element(b"r"));
    assert!(w.start_pi(b"p"));
    assert!(w.end_pi());
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<r><?p?>\n</r>\n");

    let mut w = indented();
    assert!(w.start_element(b"r"));
    assert!(w.start_element(b"a"));
    assert!(w.start_pi(b"p"));
    assert!(w.end_pi());
    assert!(w.end_element());
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<r>\n <a><?p?>\n </a>\n</r>\n");

    let mut w = indented();
    assert!(w.start_element(b"r"));
    assert!(w.write_string(b"t"));
    assert!(w.start_pi(b"p"));
    assert!(w.end_pi());
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<r>t<?p?>\n</r>\n");

    let mut w = indented();
    assert!(w.start_element(b"r"));
    assert!(w.start_cdata());
    assert!(w.end_cdata());
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<r><![CDATA[]]></r>\n");

    for (kind, expected) in [
        ("pi", "<r a=\"\"><?p?>\n</r>\n"),
        ("cdata", "<r a=\"\"><![CDATA[]]></r>\n"),
        ("text", "<r a=\"\">t</r>\n"),
        ("raw", "<r a=\"\">t</r>\n"),
        ("comment", "<r a=\"\">\n <!---->\n</r>\n"),
    ] {
        let mut w = indented();
        assert!(w.start_element(b"r"));
        assert!(w.start_attribute(b"a"));
        assert!(w.end_attribute());
        match kind {
            "pi" => {
                assert!(w.start_pi(b"p"));
                assert!(w.end_pi());
            }
            "cdata" => {
                assert!(w.start_cdata());
                assert!(w.end_cdata());
            }
            "text" => assert!(w.write_string(b"t")),
            "raw" => assert!(w.write_raw(b"t")),
            _ => {
                assert!(w.start_comment());
                assert!(w.end_comment());
            }
        }
        assert!(w.end_element());
        assert_eq!(take(&mut w), expected, "{kind}");
    }

    let mut w = indented();
    assert!(w.start_element(b"r"));
    assert!(w.start_attribute(b"a"));
    assert!(w.start_pi(b"p"));
    assert!(w.end_pi());
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<r a=\"\"><?p?>\n</r>\n");

    let mut w = indented();
    assert!(w.start_element(b"r"));
    assert!(w.start_attribute(b"a"));
    assert!(w.start_cdata());
    assert!(w.end_cdata());
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<r a=\"\"><![CDATA[]]></r>\n");

    let mut w = indented();
    assert!(w.start_element(b"r"));
    assert!(w.start_attribute(b"a"));
    assert!(!w.start_comment());
    assert!(!w.end_comment());
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<r a=\"\"/>\n");

    let mut w = indented();
    assert!(w.start_element(b"r"));
    assert!(w.start_attribute(b"a"));
    assert!(w.start_element(b"c"));
    assert!(w.end_element());
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<r a=\"\">\n <c/>\n</r>\n");
}

/// Comment, PI and CDATA content is written verbatim in every state that accepts them.
#[test]
fn unescaped_content_kinds() {
    let mut w = writer();
    assert!(w.start_element(b"r"));
    assert!(w.write_comment(b"a&b<c>d\"e'\r\n\t"));
    assert!(w.write_pi(b"p", b"a&b<c>d\"e'\r\n\t"));
    assert!(w.write_cdata(b"a&b<c>d\"e'\r\n\t"));
    assert!(!w.start_dtd(b"x", None, None));
    assert!(w.end_element());
    assert_eq!(
        take(&mut w),
        "<r><!--a&b<c>d\"e'\r\n\t--><?p a&b<c>d\"e'\r\n\t?><![CDATA[a&b<c>d\"e'\r\n\t]]></r>"
    );

    let mut w = writer();
    assert!(w.write_string(b"a&b<c>"));
    assert_eq!(take(&mut w), "a&b<c>");

    let mut w = writer();
    assert!(w.start_pi(b"p"));
    assert!(w.write_string(b"a&b<c>\"'"));
    assert!(w.end_pi());
    assert_eq!(take(&mut w), "<?p a&b<c>\"'?>");
}
