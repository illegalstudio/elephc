//! Purpose:
//! Replays the basic-document, indentation, escaping and flush blocks of the PHP
//! `XMLWriter` probe corpus (`probe3.php`) against the writer port.
//!
//! Called from:
//! - `cargo test -p elephc-xml --lib writer` through Rust's test harness.
//!
//! Key details:
//! - Each test is one PHP call sequence with the exact `outputMemory()` bytes PHP printed.

use super::{indented, take, writer};

/// A document mixing every content kind without indentation.
#[test]
fn basic_document_matches_php() {
    let mut w = writer();
    assert!(w.start_document(Some(b"1.0"), Some(b"UTF-8"), None));
    assert!(w.start_element(b"root"));
    assert!(w.write_attribute(b"a", b"v<&>\"'"));
    assert!(w.start_element(b"child"));
    assert!(w.write_string(b"t<&>\"'"));
    assert!(w.end_element());
    assert!(w.write_element(b"e", Some(b"c")));
    assert!(w.write_element(b"empty", None));
    assert!(w.write_element(b"emptystr", Some(b"")));
    assert!(w.start_element(b"x"));
    assert!(w.end_element());
    assert!(w.start_element(b"y"));
    assert!(w.full_end_element());
    assert!(w.write_comment(b"cm"));
    assert!(w.write_pi(b"pi", b"data"));
    assert!(w.write_cdata(b"cd]]>x"));
    assert!(w.write_raw(b"<raw/>"));
    assert!(w.end_element());
    assert!(w.end_document());
    assert_eq!(
        take(&mut w),
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<root a=\"v&lt;&amp;&gt;&quot;'\"><child>t&lt;&amp;&gt;&quot;'</child><e>c</e><empty/><emptystr></emptystr><x/><y></y><!--cm--><?pi data?><![CDATA[cd]]>x]]><raw/></root>\n"
    );
}

/// Tab indentation across elements, comments, PIs, CDATA, raw and empty text.
#[test]
fn indented_document_matches_php() {
    let mut w = indented();
    assert!(w.set_indent_string(b"\t"));
    assert!(w.start_document(None, None, None));
    assert!(w.start_element(b"root"));
    assert!(w.start_element(b"a"));
    assert!(w.write_string(b"txt"));
    assert!(w.end_element());
    assert!(w.start_element(b"b"));
    assert!(w.start_element(b"c"));
    assert!(w.end_element());
    assert!(w.end_element());
    assert!(w.write_element(b"d", Some(b"x")));
    assert!(w.start_element(b"e"));
    assert!(w.write_comment(b"cm"));
    assert!(w.write_pi(b"p", b"d"));
    assert!(w.write_cdata(b"cd"));
    assert!(w.end_element());
    assert!(w.start_element(b"f"));
    assert!(w.write_raw(b"r"));
    assert!(w.end_element());
    assert!(w.start_element(b"g"));
    assert!(w.write_string(b""));
    assert!(w.end_element());
    assert!(w.start_element(b"h"));
    assert!(w.full_end_element());
    assert!(w.end_element());
    assert!(w.end_document());
    assert_eq!(
        take(&mut w),
        "<?xml version=\"1.0\"?>\n<root>\n\t<a>txt</a>\n\t<b>\n\t\t<c/>\n\t</b>\n\t<d>x</d>\n\t<e>\n\t\t<!--cm-->\n<?p d?>\n<![CDATA[cd]]></e>\n\t<f>r</f>\n\t<g></g>\n\t<h></h>\n</root>\n"
    );
}

/// Indentation without a document declaration.
#[test]
fn indent_without_declaration() {
    let mut w = indented();
    assert!(w.start_element(b"a"));
    assert!(w.start_element(b"b"));
    assert!(w.end_element());
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<a>\n <b/>\n</a>\n");
}

/// Attribute values assembled from several text calls, then a nested element.
#[test]
fn indent_with_attributes() {
    let mut w = indented();
    assert!(w.start_element(b"a"));
    assert!(w.start_attribute(b"x"));
    assert!(w.write_string(b"1"));
    assert!(w.write_string(b"2"));
    assert!(w.end_attribute());
    assert!(w.write_attribute(b"y", b"2"));
    assert!(w.start_element(b"b"));
    assert!(w.write_attribute(b"z", b"3"));
    assert!(w.end_element());
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<a x=\"12\" y=\"2\">\n <b z=\"3\"/>\n</a>\n");
}

/// Attributes cannot follow content.
#[test]
fn attribute_after_content_fails() {
    let mut w = writer();
    assert!(w.start_element(b"a"));
    assert!(w.write_string(b"t"));
    assert!(!w.write_attribute(b"x", b"1"));
    assert!(!w.start_attribute(b"y"));
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<a>t</a>");
}

/// The declaration variants PHP's `startDocument` arguments produce.
#[test]
fn start_document_variants() {
    let cases: [(Option<&[u8]>, Option<&[u8]>, Option<&[u8]>, &str); 6] = [
        (None, None, None, "<?xml version=\"1.0\"?>\n\n"),
        (Some(b"1.1"), None, None, "<?xml version=\"1.1\"?>\n\n"),
        (None, Some(b"ISO-8859-1"), None, "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>\n\n"),
        (Some(b"1.0"), None, Some(b"no"), "<?xml version=\"1.0\" standalone=\"no\"?>\n\n"),
        (
            Some(b"1.0"),
            Some(b"UTF-8"),
            Some(b"yes"),
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\n",
        ),
        (Some(b""), Some(b""), Some(b""), "<?xml version=\"\" encoding=\"\" standalone=\"\"?>\n\n"),
    ];
    for (version, encoding, standalone, expected) in cases {
        let mut w = writer();
        assert!(w.start_document(version, encoding, standalone));
        assert!(w.end_document());
        assert_eq!(take(&mut w), expected);
    }
}

/// `end_document` closes an open attribute and every open element.
#[test]
fn end_document_closes_everything() {
    let mut w = indented();
    assert!(w.start_document(None, None, None));
    assert!(w.start_element(b"a"));
    assert!(w.start_element(b"b"));
    assert!(w.start_attribute(b"x"));
    assert!(w.end_document());
    assert_eq!(take(&mut w), "<?xml version=\"1.0\"?>\n<a>\n <b x=\"\"/>\n</a>\n");
}

/// PI, CDATA and comment content is never escaped and text calls concatenate.
#[test]
fn pi_cdata_comment_states() {
    let mut w = writer();
    assert!(w.start_element(b"a"));
    assert!(w.start_pi(b"target"));
    assert!(w.write_string(b"pidata"));
    assert!(w.write_string(b"more"));
    assert!(w.end_pi());
    assert!(w.start_cdata());
    assert!(w.write_string(b"cd1"));
    assert!(w.write_string(b"cd2]]>"));
    assert!(w.end_cdata());
    assert!(w.start_comment());
    assert!(w.write_string(b"c1"));
    assert!(w.write_string(b"c2"));
    assert!(w.end_comment());
    assert!(w.write_comment(b"has -- inside"));
    assert!(w.write_pi(b"t", b"has ?> inside"));
    assert!(w.end_element());
    assert_eq!(
        take(&mut w),
        "<a><?target pidatamore?><![CDATA[cd1cd2]]>]]><!--c1c2--><!--has -- inside--><?t has ?> inside?></a>"
    );
}

/// Text and attribute escaping of the special and whitespace characters.
#[test]
fn text_and_attribute_escaping() {
    let mut w = writer();
    assert!(w.write_element(b"e", Some(b"a&b<c>d\"e'f\r\ng\th")));
    assert!(w.start_element(b"x"));
    assert!(w.write_attribute(b"at", b"a&b<c>d\"e'f\r\ng\th\n"));
    assert!(w.end_element());
    assert_eq!(
        take(&mut w),
        "<e>a&amp;b&lt;c&gt;d&quot;e'f&#13;\ng\th</e><x at=\"a&amp;b&lt;c&gt;d&quot;e'f&#13;&#10;g&#9;h&#10;\"/>"
    );
}

/// Control characters and invalid bytes: U+FFFD in attributes and text, DEL untouched,
/// invalid bytes raw in text.
#[test]
fn control_and_invalid_bytes() {
    let mut w = writer();
    assert!(w.start_element(b"r"));
    assert!(w.write_string(b"\xc3\xa9\x01\x7f\x1f"));
    assert!(w.end_element());
    assert_eq!(w.take_output(), b"<r>\xc3\xa9&#xFFFD;\x7f&#xFFFD;</r>".to_vec());

    let mut w = writer();
    assert!(w.start_element(b"r"));
    assert!(w.write_attribute(b"a", b"\xff"));
    assert!(w.write_string(b"\xff"));
    assert!(w.end_element());
    assert_eq!(w.take_output(), b"<r a=\"&#xFFFD;\">\xff</r>".to_vec());
}

/// Without a document encoding, attribute values hex-escape every non-ASCII character
/// while text keeps the UTF-8 bytes.
#[test]
fn attribute_non_ascii_without_document_encoding() {
    let mut w = writer();
    assert!(w.start_element(b"r"));
    assert!(w.write_attribute(b"a", "é€😀".as_bytes()));
    assert!(w.write_string("é".as_bytes()));
    assert!(w.end_element());
    assert_eq!(w.take_output(), "<r a=\"&#xE9;&#x20AC;&#x1F600;\">é</r>".as_bytes().to_vec());

    let mut w = writer();
    assert!(w.start_document(None, None, None));
    assert!(w.start_element(b"r"));
    assert!(w.write_attribute(b"a", "é".as_bytes()));
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<?xml version=\"1.0\"?>\n<r a=\"&#xE9;\"/>");

    let mut w = writer();
    assert!(w.start_element(b"r"));
    assert!(w.write_attribute(b"a", b"\xef\xbf\xbe\xef\xbf\xbf\xed\xa0\x80\xf4\x90\x80\x80\xc0\x80\xe2\x82\xac"));
    assert!(w.end_element());
    assert_eq!(
        take(&mut w),
        "<r a=\"&#xFFFD;&#xFFFD;&#xFFFD;&#xFFFD;&#xFFFD;&#xFFFD;&#xFFFD;&#xFFFD;&#xFFFD;&#xFFFD;&#xFFFD;&#x20AC;\"/>"
    );

    let mut w = writer();
    assert!(w.start_element(b"r"));
    assert!(w.write_string(b"\xef\xbf\xbe\xef\xbf\xbf\xed\xa0\x80\xf4\x90\x80\x80\xc0\x80"));
    assert!(w.end_element());
    assert_eq!(w.take_output(), b"<r>\xef\xbf\xbe\xef\xbf\xbf\xed\xa0\x80\xf4\x90\x80\x80\xc0\x80</r>".to_vec());
}

/// Comments, PIs and CDATA at the root with indentation, comments nested in elements.
#[test]
fn indent_around_comments_and_pis() {
    let mut w = indented();
    assert!(w.write_comment(b"c"));
    assert!(w.write_pi(b"p", b"d"));
    assert!(w.start_element(b"a"));
    assert!(w.write_comment(b"c2"));
    assert!(w.start_element(b"b"));
    assert!(w.write_comment(b"c3"));
    assert!(w.end_element());
    assert!(w.end_element());
    assert!(w.write_comment(b"c4"));
    assert_eq!(
        take(&mut w),
        "<!--c-->\n<?p d?>\n<a>\n <!--c2-->\n <b>\n  <!--c3-->\n</b>\n</a>\n<!--c4-->\n"
    );
}

/// Toggling indentation in the middle of a document.
#[test]
fn set_indent_mid_document() {
    let mut w = writer();
    assert!(w.start_element(b"a"));
    assert!(w.start_element(b"b"));
    assert!(w.set_indent(true));
    assert!(w.start_element(b"c"));
    assert!(w.end_element());
    assert!(w.end_element());
    assert!(w.set_indent(false));
    assert!(w.start_element(b"d"));
    assert!(w.end_element());
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<a><b>\n  <c/>\n </b>\n<d/></a>");
}

/// `set_indent` re-arms `doindent`, so an end tag after text still gets indented.
#[test]
fn set_indent_rearms_doindent() {
    let mut w = indented();
    assert!(w.start_element(b"a"));
    assert!(w.write_string(b"t"));
    assert!(w.set_indent(true));
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<a>t</a>\n");

    let mut w = indented();
    assert!(w.start_element(b"a"));
    assert!(w.write_string(b"t"));
    assert!(w.set_indent(false));
    assert!(w.set_indent(true));
    assert!(w.start_element(b"b"));
    assert!(w.end_element());
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<a>t <b/>\n</a>\n");
}

/// Text before, between and after child elements.
#[test]
fn text_around_children() {
    let mut w = writer();
    assert!(w.start_element(b"a"));
    assert!(w.write_string(b"x"));
    assert!(w.write_string(b"y"));
    assert!(w.start_element(b"b"));
    assert!(w.end_element());
    assert!(w.write_string(b"z"));
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<a>xy<b/>z</a>");
}

/// A child element after text is indented without a preceding newline.
#[test]
fn end_element_after_text_with_indent() {
    let mut w = indented();
    assert!(w.start_element(b"a"));
    assert!(w.start_element(b"b"));
    assert!(w.write_string(b"t"));
    assert!(w.start_element(b"c"));
    assert!(w.end_element());
    assert!(w.end_element());
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<a>\n <b>t  <c/>\n </b>\n</a>\n");
}

/// Empty, absent and present content through `write_element` with indentation.
#[test]
fn write_element_content_forms_with_indent() {
    let mut w = indented();
    assert!(w.start_element(b"r"));
    assert!(w.write_element(b"a", Some(b"")));
    assert!(w.write_element(b"b", None));
    assert!(w.write_element(b"c", Some(b"x")));
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<r>\n <a></a>\n <b/>\n <c>x</c>\n</r>\n");
}

/// PIs and CDATA inside an indented element: no indent, newline only after a PI.
#[test]
fn pi_and_cdata_inside_indented_element() {
    let mut w = indented();
    assert!(w.start_element(b"r"));
    assert!(w.write_pi(b"p", b"d"));
    assert!(w.write_pi(b"q", b""));
    assert!(w.start_element(b"a"));
    assert!(w.end_element());
    assert!(w.write_cdata(b"c"));
    assert!(w.start_element(b"b"));
    assert!(w.end_element());
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<r><?p d?>\n<?q ?>\n <a/>\n<![CDATA[c]]> <b/>\n</r>\n");

    let mut w = indented();
    assert!(w.write_pi(b"p", b"d"));
    assert!(w.start_element(b"r"));
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<?p d?>\n<r/>\n");
}

/// CDATA content is written verbatim, even a `]]>`.
#[test]
fn cdata_is_not_split() {
    let mut w = writer();
    assert!(w.write_cdata(b"a]]>b"));
    assert_eq!(take(&mut w), "<![CDATA[a]]>b]]>");
}

/// Text and raw content at the top level after the declaration.
#[test]
fn top_level_text_after_declaration() {
    let mut w = indented();
    assert!(w.start_document(None, None, None));
    assert!(w.write_string(b"t"));
    assert!(w.start_element(b"r"));
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<?xml version=\"1.0\"?>\nt<r/>\n");

    let mut w = indented();
    assert!(w.start_document(None, None, None));
    assert!(w.write_raw(b"t"));
    assert!(w.start_element(b"r"));
    assert!(w.end_element());
    assert!(w.end_document());
    assert_eq!(take(&mut w), "<?xml version=\"1.0\"?>\nt<r/>\n");
}

/// `output()` peeks after a flush, `take_output()` drains.
#[test]
fn flush_and_take_semantics() {
    let mut w = writer();
    assert!(w.start_element(b"a"));
    assert!(w.write_string(b"1"));
    assert!(w.flush());
    assert_eq!(w.output(), b"<a>1");
    assert_eq!(w.output(), b"<a>1");
    assert_eq!(w.take_output(), b"<a>1".to_vec());
    assert_eq!(w.take_output(), Vec::<u8>::new());
    assert!(w.write_string(b"2"));
    assert!(w.end_element());
    assert_eq!(w.take_output(), b"2</a>".to_vec());
    assert!(w.flush());
    assert_eq!(w.output(), b"");
}

/// A second declaration is allowed once the stack is empty again.
#[test]
fn start_document_twice_and_after_content() {
    let mut w = writer();
    assert!(w.start_document(None, None, None));
    assert!(w.start_document(None, None, None));
    assert_eq!(take(&mut w), "<?xml version=\"1.0\"?>\n<?xml version=\"1.0\"?>\n");

    let mut w = writer();
    assert!(w.write_element(b"a", None));
    assert!(w.start_document(None, None, None));
    assert_eq!(take(&mut w), "<a/><?xml version=\"1.0\"?>\n");
}

/// Elements and comments may follow the root element; `end_document` adds one newline.
#[test]
fn content_after_root_element() {
    let mut w = writer();
    assert!(w.start_document(None, None, None));
    assert!(w.write_element(b"a", None));
    assert!(w.write_element(b"b", None));
    assert!(w.write_comment(b"c"));
    assert!(w.end_document());
    assert_eq!(take(&mut w), "<?xml version=\"1.0\"?>\n<a/><b/><!--c-->\n");
}

/// `end_document` on an empty writer: a bare newline without indentation, nothing with.
#[test]
fn end_document_on_empty_writer() {
    let mut w = indented();
    assert!(w.end_document());
    assert_eq!(take(&mut w), "");

    let mut w = writer();
    assert!(w.write_element(b"a", None));
    assert!(w.end_document());
    assert_eq!(take(&mut w), "<a/>\n");
}

/// The indent string may be empty or arbitrary.
#[test]
fn indent_string_edge_cases() {
    let mut w = indented();
    assert!(w.set_indent_string(b""));
    assert!(w.start_element(b"a"));
    assert!(w.start_element(b"b"));
    assert!(w.end_element());
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<a>\n<b/>\n</a>\n");

    let mut w = writer();
    assert!(w.set_indent_string(b">>"));
    assert!(w.set_indent(true));
    assert!(w.start_element(b"a"));
    assert!(w.start_element(b"b"));
    assert!(w.end_element());
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<a>\n>><b/>\n</a>\n");
}

/// Strings cross to libxml2 as C strings: an interior NUL truncates a name or a value,
/// exactly like php-src's `(xmlChar *)` casts (PHP: `<a>x</a><e q="1">t</e>`, and an
/// attribute outside a start tag still fails).
#[test]
fn interior_nul_truncates_like_c_strings() {
    let mut w = writer();
    assert!(w.write_element(b"a\0b", Some(b"x\0y")));
    assert!(!w.write_attribute(b"q\0r", b"1\x002"));
    assert!(w.start_element(b"e"));
    assert!(w.write_attribute(b"q\0r", b"1\x002"));
    assert!(w.write_string(b"t\0u"));
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<a>x</a><e q=\"1\">t</e>");
}
