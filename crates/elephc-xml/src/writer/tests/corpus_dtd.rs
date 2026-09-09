//! Purpose:
//! Replays the DTD blocks of the PHP `XMLWriter` probe corpus: DOCTYPE identifiers with
//! and without indentation, the internal subset, ELEMENT/ATTLIST/ENTITY declarations in
//! every form, and the state errors around them.
//!
//! Called from:
//! - `cargo test -p elephc-xml --lib writer` through Rust's test harness.
//!
//! Key details:
//! - `write_dtd_entity` is exercised through the PHP-facing dispatcher so the
//!   internal/external selection matches `xmlTextWriterWriteDTDEntity`.

use super::{indented, take, writer};

/// A full indented DTD with every declaration kind.
#[test]
fn indented_dtd_matches_php() {
    let mut w = indented();
    assert!(w.start_document(Some(b"1.0"), Some(b"UTF-8"), Some(b"yes")));
    assert!(w.start_dtd(
        b"html",
        Some(b"-//W3C//DTD XHTML 1.0//EN"),
        Some(b"http://www.w3.org/TR/xhtml1/DTD/xhtml1.dtd")
    ));
    assert!(w.start_dtd_element(b"html"));
    assert!(w.write_string(b"(head, body)"));
    assert!(w.end_dtd_element());
    assert!(w.write_dtd_element(b"head", b"(title)"));
    assert!(w.start_dtd_attlist(b"a"));
    assert!(w.write_string(b"href CDATA #REQUIRED"));
    assert!(w.end_dtd_attlist());
    assert!(w.write_dtd_attlist(b"img", b"src CDATA #REQUIRED"));
    assert!(w.start_dtd_entity(b"e1", false));
    assert!(w.write_string(b"v1"));
    assert!(w.end_dtd_entity());
    assert!(w.write_dtd_entity(b"e2", Some(b"v2"), false, None, None, None));
    assert!(w.write_dtd_entity(b"e3", Some(b"v3"), true, None, None, None));
    assert!(w.write_dtd_entity(b"e4", Some(b""), false, Some(b"pub"), Some(b"sys"), None));
    assert!(w.write_dtd_entity(b"e5", Some(b""), false, None, Some(b"sys5"), Some(b"ndata")));
    assert!(!w.write_dtd_entity(b"e6", Some(b"ignored?"), false, Some(b"pub6"), None, None));
    assert!(w.end_dtd());
    assert!(w.start_element(b"html"));
    assert!(w.end_element());
    assert!(w.end_document());
    assert_eq!(
        take(&mut w),
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<!DOCTYPE html\nPUBLIC \"-//W3C//DTD XHTML 1.0//EN\"\n       \"http://www.w3.org/TR/xhtml1/DTD/xhtml1.dtd\" [\n <!ELEMENT html (head, body)>\n <!ELEMENT head (title)>\n <!ATTLIST a href CDATA #REQUIRED>\n <!ATTLIST img src CDATA #REQUIRED>\n <!ENTITY e1 \"v1\">\n <!ENTITY e2 \"v2\">\n <!ENTITY % e3 \"v3\">\n <!ENTITY e4 PUBLIC \"pub\" \"sys\">\n <!ENTITY e5 SYSTEM \"sys5\" NDATA ndata>\n <!ENTITY e6>\n]>\n<html/>\n"
    );
}

/// DOCTYPEs without indentation, including the stuck node a public id without a system
/// id leaves behind.
#[test]
fn dtd_without_indent() {
    let mut w = writer();
    assert!(w.start_dtd(b"a", None, None));
    assert!(w.write_dtd_element(b"a", b"EMPTY"));
    assert!(w.end_dtd());
    assert!(w.write_dtd(b"b", None, Some(b"sys"), None));
    assert!(w.write_dtd(b"c", Some(b"pub"), Some(b"sys"), Some(b"<!ELEMENT c ANY>")));
    assert!(!w.write_dtd(b"d", Some(b"pub"), None, None));
    assert!(!w.write_dtd(b"e", None, None, Some(b"")));
    assert!(!w.start_dtd(b"f", Some(b"pub2"), None));
    assert!(w.end_dtd());
    assert!(w.start_dtd(b"g", None, Some(b"sys2")));
    assert!(w.end_dtd());
    assert!(w.start_dtd(b"h", None, None));
    assert!(w.write_string(b"<!ELEMENT h ANY>"));
    assert!(w.end_dtd());
    assert_eq!(
        take(&mut w),
        "<!DOCTYPE a [<!ELEMENT a EMPTY>]><!DOCTYPE b SYSTEM \"sys\"><!DOCTYPE c PUBLIC \"pub\" \"sys\" [<!ELEMENT c ANY>]><!DOCTYPE d><!DOCTYPE g SYSTEM \"sys2\"><!DOCTYPE h [<!ELEMENT h ANY>]>"
    );
}

/// The engine only rejects empty DOCTYPE names.
#[test]
fn doctype_names_are_not_validated_by_the_engine() {
    let mut w = writer();
    for name in [&b""[..], b"a b", b"1a", b"a:b", b"ok"] {
        let started = w.start_dtd(name, None, None);
        assert_eq!(started, !name.is_empty());
        w.end_dtd();
    }
    assert!(!w.write_dtd(b"", None, None, None));
    assert!(w.write_dtd(b"a b", None, None, None));
    assert!(w.write_dtd(b"ok", None, None, None));
    assert_eq!(
        take(&mut w),
        "<!DOCTYPE a b><!DOCTYPE 1a><!DOCTYPE a:b><!DOCTYPE ok><!DOCTYPE a b><!DOCTYPE ok>"
    );
}

/// Declarations with empty content and the two-argument write forms.
#[test]
fn declarations_inside_subset() {
    let mut w = writer();
    assert!(w.start_dtd(b"r", None, None));
    assert!(w.start_dtd_element(b"ok"));
    assert!(w.end_dtd_element());
    assert!(w.write_dtd_element(b"ok", b"ANY"));
    assert!(w.start_dtd_attlist(b"ok"));
    assert!(w.end_dtd_attlist());
    assert!(w.write_dtd_attlist(b"ok", b"x CDATA #IMPLIED"));
    assert!(w.start_dtd_entity(b"ok", false));
    assert!(w.end_dtd_entity());
    assert!(w.write_dtd_entity(b"ok", Some(b"v"), false, None, None, None));
    assert!(w.end_dtd());
    assert_eq!(
        take(&mut w),
        "<!DOCTYPE r [<!ELEMENT ok><!ELEMENT ok ANY><!ATTLIST ok><!ATTLIST ok x CDATA #IMPLIED><!ENTITY ok><!ENTITY ok \"v\">]>"
    );
}

/// Indented DOCTYPE identifier layouts.
#[test]
fn indented_doctype_identifiers() {
    let mut w = indented();
    assert!(!w.start_dtd(b"r", Some(b"pub"), None));
    assert!(w.end_dtd());
    assert_eq!(take(&mut w), "<!DOCTYPE r>\n");

    let mut w = indented();
    assert!(w.start_dtd(b"r", None, Some(b"sys")));
    assert!(w.start_dtd_element(b"e"));
    assert!(w.end_dtd());
    assert_eq!(take(&mut w), "<!DOCTYPE r\nSYSTEM \"sys\" [\n <!ELEMENT e>\n]>\n");

    let mut w = indented();
    assert!(w.start_dtd(b"r", Some(b"p"), Some(b"s")));
    assert!(w.write_dtd_element(b"e", b"ANY"));
    assert!(w.end_dtd());
    assert_eq!(take(&mut w), "<!DOCTYPE r\nPUBLIC \"p\"\n       \"s\" [\n <!ELEMENT e ANY>\n]>\n");
}

/// Entity variants through the PHP-facing dispatcher with indentation.
#[test]
fn entity_dispatcher_variants() {
    let mut w = indented();
    assert!(!w.start_dtd(b"r", Some(b"pub"), None));
    assert!(w.start_dtd_entity(b"e", false));
    assert!(w.write_string(b"v"));
    assert!(w.end_dtd_entity());
    assert!(w.write_dtd_entity(b"x", Some(b"y"), false, Some(b"p"), Some(b"s"), Some(b"n")));
    assert!(!w.write_dtd_entity(b"z", Some(b"y"), true, Some(b"p"), Some(b"s"), Some(b"n")));
    assert!(w.write_dtd_entity(b"q", Some(b"y"), false, None, None, Some(b"n")));
    assert!(w.end_dtd());
    assert!(w.start_element(b"r"));
    assert!(w.end_element());
    assert_eq!(
        take(&mut w),
        "<!DOCTYPE r [\n <!ENTITY e \"v\">\n <!ENTITY x PUBLIC \"p\" \"s\" NDATA n>\n <!ENTITY q \"y\">\n]>\n<r/>\n"
    );
}

/// State errors around open declarations.
#[test]
fn declaration_state_errors() {
    let mut w = writer();
    assert!(w.start_dtd(b"r", None, None));
    assert!(w.start_dtd_entity(b"e", false));
    assert!(w.start_element(b"x"));
    assert!(!w.end_dtd_entity());
    assert!(w.end_dtd());
    assert_eq!(take(&mut w), "<!DOCTYPE r [<!ENTITY e<x");

    let mut w = writer();
    assert!(w.start_dtd(b"r", None, None));
    assert!(w.write_string(b"<!ELEMENT r ANY>"));
    assert!(w.start_dtd_element(b"e"));
    assert!(w.end_dtd_element());
    assert!(w.end_dtd());
    assert_eq!(take(&mut w), "<!DOCTYPE r [<!ELEMENT r ANY><!ELEMENT e>]>");

    let mut w = writer();
    assert!(w.start_dtd_element(b"e"));
    assert!(!w.start_dtd_attlist(b"a"));
    assert!(!w.start_dtd_entity(b"n", false));
    assert!(w.end_dtd());
    assert_eq!(take(&mut w), "<!ELEMENT e>");

    let mut w = writer();
    assert!(w.start_dtd(b"r", None, None));
    assert!(w.start_dtd_entity(b"e", true));
    assert!(w.write_string(b"v"));
    assert!(w.end_dtd_entity());
    assert!(w.start_dtd_entity(b"e2", false));
    assert!(!w.start_dtd_entity(b"e3", false));
    assert!(w.end_dtd());
    assert_eq!(take(&mut w), "<!DOCTYPE r [<!ENTITY % e \"v\"><!ENTITY e2>]>");

    let mut w = writer();
    assert!(w.start_dtd(b"r", None, None));
    assert!(w.start_dtd_attlist(b"a"));
    assert!(w.write_string(b"x CDATA #IMPLIED"));
    assert!(!w.start_dtd_element(b"e"));
    assert!(w.end_dtd());
    assert_eq!(take(&mut w), "<!DOCTYPE r [<!ATTLIST a x CDATA #IMPLIED>]>");

    let mut w = writer();
    assert!(w.start_dtd(b"r", None, None));
    assert!(w.start_dtd_element(b"e"));
    assert!(w.write_string(b"ANY"));
    assert!(!w.start_dtd_element(b"f"));
    assert!(w.start_element(b"x"));
    assert!(w.end_dtd());
    assert_eq!(take(&mut w), "<!DOCTYPE r [<!ELEMENT e ANY<x");
}

/// Comments and CDATA are refused inside a DOCTYPE; PIs are allowed.
#[test]
fn comment_and_pi_inside_doctype() {
    let mut w = indented();
    assert!(w.start_dtd(b"r", None, None));
    assert!(!w.write_comment(b"c"));
    assert!(!w.start_cdata());
    assert!(w.write_pi(b"p", b"d"));
    assert!(w.end_dtd());
    assert_eq!(take(&mut w), "<!DOCTYPE r<?p d?>\n>\n");

    let mut w = writer();
    assert!(w.start_dtd(b"r", None, None));
    assert!(!w.write_comment(b"c"));
    assert!(!w.start_cdata());
    assert!(w.write_pi(b"p", b"d"));
    assert!(w.end_dtd());
    assert_eq!(take(&mut w), "<!DOCTYPE r<?p d?>>");

    let mut w = writer();
    assert!(w.start_dtd(b"r", None, None));
    assert!(w.write_string(b"x"));
    assert!(w.start_element(b"e"));
    assert!(w.start_comment());
    assert!(w.end_dtd());
    assert_eq!(take(&mut w), "<!DOCTYPE r [x<e><!---->");
}

/// Declaration content is written verbatim.
#[test]
fn declaration_content_is_raw() {
    let mut w = writer();
    assert!(w.start_dtd(b"r", None, None));
    assert!(w.start_dtd_element(b"e"));
    assert!(w.write_string(b"a&b<c>d\"e'\r"));
    assert!(w.end_dtd_element());
    assert!(w.start_dtd_entity(b"n", false));
    assert!(w.write_string(b"a&b<c>d\"e'\r"));
    assert!(w.end_dtd_entity());
    assert!(w.start_dtd_attlist(b"at"));
    assert!(w.write_string(b"a&b<c>d\"e'\r"));
    assert!(w.end_dtd_attlist());
    assert!(w.write_string(b"<!-- raw & -->"));
    assert!(w.end_dtd());
    assert_eq!(
        take(&mut w),
        "<!DOCTYPE r [<!ELEMENT e a&b<c>d\"e'\r><!ENTITY n \"a&b<c>d\"e'\r\"><!ATTLIST at a&b<c>d\"e'\r><!-- raw & -->]>"
    );
}

/// The dispatcher's argument rules.
#[test]
fn entity_dispatcher_argument_rules() {
    let mut w = writer();
    assert!(w.start_dtd(b"r", None, None));
    assert!(!w.write_dtd_entity(b"a", None, false, None, None, None));
    assert!(!w.write_dtd_entity(b"b", Some(b"v"), true, None, None, Some(b"n")));
    assert!(w.write_dtd_entity(b"c", None, false, None, Some(b"s"), None));
    assert!(w.write_dtd_internal_entity(b"d", b"v", true));
    assert!(!w.write_dtd_internal_entity(b"", b"v", false));
    assert!(!w.write_dtd_external_entity(b"e", None, None, None, false));
    assert!(w.end_dtd());
    assert_eq!(take(&mut w), "<!DOCTYPE r [<!ENTITY c SYSTEM \"s\"><!ENTITY % d \"v\">]>");
}
