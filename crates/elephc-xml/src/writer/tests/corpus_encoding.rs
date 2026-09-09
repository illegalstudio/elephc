//! Purpose:
//! Replays the output-encoding blocks of the PHP `XMLWriter` probe corpus: the ISO-8859-1
//! and US-ASCII transcoders with their decimal-reference fallback, the accepted and
//! rejected encoding names, the `doc->encoding` effect on attribute escaping, the
//! conversion thresholds and the sticky error an undecodable sequence leaves behind.
//!
//! Called from:
//! - `cargo test -p elephc-xml --lib writer` through Rust's test harness.
//!
//! Key details:
//! - Expected bytes are the hex dumps PHP printed, decoded here as byte strings.

use super::{take, take_bytes, writer};

/// Every content kind is transcoded to ISO-8859-1, with references for the euro sign.
#[test]
fn latin1_transcodes_every_content_kind() {
    let mut w = writer();
    assert!(w.start_document(Some(b"1.0"), Some(b"ISO-8859-1"), None));
    assert!(w.start_element(b"r"));
    assert!(w.write_comment("caf€".as_bytes()));
    assert!(w.write_cdata("x€".as_bytes()));
    assert!(w.write_raw("y€".as_bytes()));
    assert!(w.write_pi(b"p", "z€".as_bytes()));
    assert!(w.write_string("t€".as_bytes()));
    assert!(w.end_element());
    assert_eq!(
        take(&mut w),
        "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>\n<r><!--caf&#8364;--><![CDATA[x&#8364;]]>y&#8364;<?p z&#8364;?>t&#8364;</r>"
    );

    let mut w = writer();
    assert!(w.start_document(Some(b"1.0"), Some(b"ISO-8859-1"), None));
    assert!(w.write_element(b"e", Some("café €".as_bytes())));
    assert!(w.end_document());
    assert_eq!(
        take_bytes(&mut w),
        b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>\n<e>caf\xe9 &#8364;</e>\n".to_vec()
    );

    let mut w = writer();
    assert!(w.start_document(Some(b"1.0"), Some(b"ISO-8859-1"), None));
    assert!(w.start_element(b"e"));
    assert!(w.write_attribute(b"a", "café".as_bytes()));
    assert!(w.write_comment("café".as_bytes()));
    assert!(w.write_cdata("café".as_bytes()));
    assert!(w.write_raw("café".as_bytes()));
    assert!(w.end_document());
    assert_eq!(
        take_bytes(&mut w),
        b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>\n<e a=\"caf\xe9\"><!--caf\xe9--><![CDATA[caf\xe9]]>caf\xe9</e>\n".to_vec()
    );
}

/// US-ASCII references every non-ASCII character, names included.
#[test]
fn ascii_transcodes_with_decimal_references() {
    let mut w = writer();
    assert!(w.start_document(Some(b"1.0"), Some(b"US-ASCII"), None));
    assert!(w.write_element(b"e", Some("café".as_bytes())));
    assert!(w.end_document());
    assert_eq!(
        take(&mut w),
        "<?xml version=\"1.0\" encoding=\"US-ASCII\"?>\n<e>caf&#233;</e>\n"
    );

    let mut w = writer();
    assert!(w.start_document(Some(b"1.0"), Some(b"US-ASCII"), None));
    assert!(w.start_element("é".as_bytes()));
    assert!(w.write_attribute("à".as_bytes(), "é".as_bytes()));
    assert!(w.write_string("é\x01".as_bytes()));
    assert!(w.end_element());
    assert_eq!(
        take(&mut w),
        "<?xml version=\"1.0\" encoding=\"US-ASCII\"?>\n<&#233; &#224;=\"&#233;\">&#233;&#xFFFD;</&#233;>"
    );

    let mut w = writer();
    assert!(w.start_document(Some(b"1.0"), Some(b"US-ASCII"), None));
    assert!(w.write_raw("aé€😀b".as_bytes()));
    assert_eq!(
        take(&mut w),
        "<?xml version=\"1.0\" encoding=\"US-ASCII\"?>\na&#233;&#8364;&#128512;b"
    );
}

/// The escape layer runs before the transcoder: control characters become U+FFFD
/// references, then the euro sign becomes a decimal reference.
#[test]
fn escape_then_transcode_order() {
    let mut w = writer();
    assert!(w.start_document(Some(b"1.0"), Some(b"ISO-8859-1"), None));
    assert!(w.start_element(b"r"));
    assert!(w.write_attribute(b"a", "é€\x01".as_bytes()));
    assert!(w.end_element());
    assert_eq!(
        take_bytes(&mut w),
        b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>\n<r a=\"\xe9&#8364;&#xFFFD;\"/>".to_vec()
    );

    let mut w = writer();
    assert!(w.start_document(Some(b"1.0"), Some(b"ISO-8859-1"), None));
    assert!(w.start_element(b"r"));
    assert!(w.write_string(b"\x01\x7f\x80"));
    assert!(w.end_element());
    assert!(!w.flush());
    assert_eq!(take_bytes(&mut w), Vec::<u8>::new());
}

/// Encoding names: UTF-8 aliases print `UTF-8`, others print as given, the empty name is
/// accepted, and names libxml2 has no handler for (neither built in nor through iconv)
/// fail without touching the writer.
#[test]
fn encoding_names() {
    let accepted: [(&[u8], &str); 12] = [
        (b"utf-8", "UTF-8"),
        (b"UTF8", "UTF-8"),
        (b"latin1", "latin1"),
        (b"LATIN1", "LATIN1"),
        (b"iso-8859-1", "iso-8859-1"),
        (b"ISO_8859-1", "ISO_8859-1"),
        (b"ISO-LATIN-1", "ISO-LATIN-1"),
        (b"ascii", "ascii"),
        (b"ASCII", "ASCII"),
        (b"us-ascii", "us-ascii"),
        (b"ISO-8859-1", "ISO-8859-1"),
        (b"", ""),
    ];
    for (name, printed) in accepted {
        let mut w = writer();
        assert!(w.start_document(Some(b"1.0"), Some(name), None), "{printed}");
        assert_eq!(take(&mut w), format!("<?xml version=\"1.0\" encoding=\"{printed}\"?>\n"));
    }
    for name in [&b"html"[..], b"ebcdic", b"x-unknown"] {
        let mut w = writer();
        assert!(!w.start_document(Some(b"1.0"), Some(name), None));
        assert!(w.start_element(b"r"));
        assert!(w.write_attribute(b"a", "é".as_bytes()));
        assert!(w.end_element());
        assert_eq!(take(&mut w), "<r a=\"&#xE9;\"/>");
    }
}

/// Encodings libxml2 serves through the platform iconv (the catalog builds it
/// `--with-iconv`): PHP prints the name as given and transcodes the content.
#[test]
fn iconv_backed_encodings() {
    for (name, expected) in [
        (&b"windows-1252"[..], &b"<?xml version=\"1.0\" encoding=\"windows-1252\"?>\n<r a=\"\xe9\"/>"[..]),
        (b"ISO-8859-2", b"<?xml version=\"1.0\" encoding=\"ISO-8859-2\"?>\n<r a=\"\xe9\"/>"),
    ] {
        let mut w = writer();
        assert!(w.start_document(Some(b"1.0"), Some(name), None), "{}", String::from_utf8_lossy(name));
        assert!(w.start_element(b"r"));
        assert!(w.write_attribute(b"a", "é".as_bytes()));
        assert!(w.end_element());
        assert_eq!(take_bytes(&mut w), expected.to_vec(), "{}", String::from_utf8_lossy(name));
    }
    let mut w = writer();
    assert!(w.start_document(Some(b"1.0"), Some(b"utf-16"), None));
    assert!(w.write_element(b"r", None));
    let out = take_bytes(&mut w);
    assert!(out.starts_with(b"\xff\xfe<\x00?\x00x\x00m\x00l\x00"), "{out:?}");
    assert!(out.ends_with(b"<\x00r\x00/\x00>\x00"), "{out:?}");
}

/// A declared encoding (any, even UTF-8 or empty) stops attribute hex escaping.
#[test]
fn declared_encoding_disables_attribute_hex_escaping() {
    let mut w = writer();
    assert!(w.start_document(Some(b"1.0"), Some(b"UTF-8"), None));
    assert!(w.start_element(b"r"));
    assert!(w.write_attribute(b"a", b"\xc3\xa9\xff\x01"));
    assert!(w.write_string(b"\xc3\xa9\xff\x01"));
    assert!(w.end_element());
    assert_eq!(
        take_bytes(&mut w),
        b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<r a=\"\xc3\xa9\xff&#xFFFD;\">\xc3\xa9\xff&#xFFFD;</r>".to_vec()
    );

    let mut w = writer();
    assert!(w.start_document(Some(b"1.0"), Some(b"utf8"), None));
    assert!(w.start_element(b"r"));
    assert!(w.write_attribute(b"a", "é".as_bytes()));
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<r a=\"é\"/>");

    // The empty name is served by iconv as the process locale's codeset (UTF-8 under PHP's
    // forced C.UTF-8, US-ASCII in a plain C-locale process), so only ASCII content is
    // asserted here; the declaration prints the name as given either way.
    let mut w = writer();
    assert!(w.start_document(Some(b"1.0"), Some(b""), None));
    assert!(w.start_element(b"r"));
    assert!(w.write_attribute(b"a", b"x"));
    assert!(w.write_string(b"y"));
    assert!(w.end_element());
    assert_eq!(take(&mut w), "<?xml version=\"1.0\" encoding=\"\"?>\n<r a=\"x\">y</r>");

    let mut w = writer();
    assert!(w.start_document(Some(b"1.0"), Some(b"ISO-8859-1"), None));
    assert!(w.write_element(b"a", Some("é".as_bytes())));
    assert!(w.end_document());
    assert!(w.start_document(None, None, None));
    assert!(w.start_element(b"r"));
    assert!(w.write_attribute(b"a", "é".as_bytes()));
    assert!(w.write_string("é".as_bytes()));
    assert!(w.end_element());
    assert_eq!(
        take_bytes(&mut w),
        b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>\n<a>\xe9</a>\n<?xml version=\"1.0\"?>\n<r a=\"\xc3\xa9\">\xc3\xa9</r>".to_vec()
    );
}

/// An undecodable sequence: writes keep succeeding until a conversion is attempted,
/// the flush then fails, nothing is delivered, and every later write fails.
#[test]
fn undecodable_sequence_sets_sticky_error() {
    let mut w = writer();
    assert!(w.start_document(Some(b"1.0"), Some(b"ISO-8859-1"), None));
    assert!(w.start_element(b"r"));
    assert!(w.write_string(b"a\xffb"));
    assert!(w.write_string(b"c"));
    assert!(w.end_element());
    assert!(!w.flush());
    assert_eq!(w.output(), b"");
    assert_eq!(take_bytes(&mut w), Vec::<u8>::new());
    assert!(!w.write_element(b"z", None));
    assert_eq!(take_bytes(&mut w), Vec::<u8>::new());

    for raw in [
        &b"\xff<z/>"[..],
        b"ab\xff<z/>",
        b"ab\xff\xfe<z/>",
        b"ab\xe2\x82<z/>",
        b"a\xc0\x80b",
        b"a\xed\xa0\x80b",
        b"a\xf4\x90\x80\x80b",
    ] {
        let mut w = writer();
        assert!(w.start_document(Some(b"1.0"), Some(b"ISO-8859-1"), None));
        assert!(w.write_raw(raw));
        assert_eq!(take_bytes(&mut w), Vec::<u8>::new(), "{raw:?}");
    }

    let mut w = writer();
    assert!(w.start_document(Some(b"1.0"), Some(b"US-ASCII"), None));
    assert!(w.write_raw(b"ab\xff<z/>"));
    assert_eq!(take_bytes(&mut w), Vec::<u8>::new());

    let mut w = writer();
    assert!(w.start_document(Some(b"1.0"), Some(b"US-ASCII"), None));
    assert!(w.write_raw(b"a\x80b"));
    assert_eq!(take_bytes(&mut w), Vec::<u8>::new());
}

/// U+FFFE is decodable and becomes a decimal reference; a truncated sequence waits for
/// its continuation, and the Latin-1 transcoder never validates the byte after 0xC3.
#[test]
fn latin1_decoder_quirks() {
    let mut w = writer();
    assert!(w.start_document(Some(b"1.0"), Some(b"ISO-8859-1"), None));
    assert!(w.write_raw(b"a\xef\xbf\xbeb"));
    assert_eq!(
        take(&mut w),
        "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>\na&#65534;b"
    );

    let mut w = writer();
    assert!(w.start_document(Some(b"1.0"), Some(b"ISO-8859-1"), None));
    assert!(w.write_raw(b"a\xc3"));
    assert!(w.write_raw(b"\xa9b"));
    assert_eq!(
        take_bytes(&mut w),
        b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>\na\xe9b".to_vec()
    );

    let mut w = writer();
    assert!(w.start_document(Some(b"1.0"), Some(b"ISO-8859-1"), None));
    assert!(w.write_raw(b"a\xc3"));
    assert_eq!(
        take_bytes(&mut w),
        b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>\na".to_vec()
    );
    assert!(w.write_raw(b"b"));
    assert_eq!(take_bytes(&mut w), b"\xe2".to_vec());

    let mut w = writer();
    assert!(w.start_document(Some(b"1.0"), Some(b"latin1"), None));
    assert!(w.write_raw(b"ab\xc3b<z/>"));
    assert_eq!(
        take_bytes(&mut w),
        b"<?xml version=\"1.0\" encoding=\"latin1\"?>\nab\xe2<z/>".to_vec()
    );

    let mut w = writer();
    assert!(w.start_document(Some(b"1.0"), Some(b"latin1"), None));
    assert!(w.write_raw("ab€é<z/>".as_bytes()));
    assert_eq!(
        take_bytes(&mut w),
        b"<?xml version=\"1.0\" encoding=\"latin1\"?>\nab&#8364;\xe9<z/>".to_vec()
    );
}

/// The write-time conversion threshold: a large write converts and delivers what precedes
/// the bad byte, the following small write still succeeds but is never delivered, and the
/// flush fails.
#[test]
fn conversion_thresholds() {
    let mut w = writer();
    assert!(w.start_document(Some(b"1.0"), Some(b"ISO-8859-1"), None));
    let mut big = vec![b'x'; 300];
    big.push(0xff);
    assert!(w.write_raw(&big));
    assert!(w.write_raw(b"<z/>"));
    assert_eq!(take_bytes(&mut w), Vec::<u8>::new());

    let mut w = writer();
    assert!(w.start_document(Some(b"1.0"), Some(b"ISO-8859-1"), None));
    assert!(w.write_raw(b"a\xffb"));
    assert!(w.write_raw(&vec![b'y'; 300]));
    assert!(!w.write_element(b"z", None));
    assert_eq!(take_bytes(&mut w), Vec::<u8>::new());

    let mut w = writer();
    assert!(w.start_document(Some(b"1.0"), Some(b"ISO-8859-1"), None));
    let mut big = vec![b'x'; 5000];
    big.push(0xff);
    assert!(w.write_raw(&big));
    assert!(w.write_raw(b"<z/>"));
    let delivered = take_bytes(&mut w);
    assert_eq!(delivered.len(), 44 + 5000);
    assert!(delivered.ends_with(b"xxxx"));
    assert!(!w.write_raw(b"q"));
    assert_eq!(take_bytes(&mut w), Vec::<u8>::new());
}

/// `end_document` fails when its flush fails.
#[test]
fn end_document_reports_flush_failure() {
    let mut w = writer();
    assert!(w.start_document(Some(b"1.0"), Some(b"ISO-8859-1"), None));
    assert!(w.write_element(b"a", Some(b"\xff")));
    assert!(!w.end_document());
    assert_eq!(take_bytes(&mut w), Vec::<u8>::new());
}
