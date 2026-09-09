//! Purpose:
//! Replays the recorded PHP probe corpus through the parser and compares the rendered
//! event streams with PHP 8.5.10's output, plus the hand-written cases that need a
//! different call shape (chunked feeding, positions after each chunk, stop requests).
//!
//! Called from:
//! - `cargo test -p elephc-xml --lib parser`.
//!
//! Key details:
//! - Each probe has its own handler set and line style; the tables in `corpus_data`
//!   drive the calls in the scripts' order.

use super::corpus_data::{Entry, PROBE1C, PROBE2MISC, PROBE4C, PROBE5C, PROBE6C, PROBE8C, PROBE9C};
use super::harness::{assert_corpus, summary_line, var_export, Handlers, Session, Style};
use crate::parser::{error_string, Event, Parser, Step};

/// Extra handlers a probe's `run()` registered on every parser.
#[derive(Clone, Copy)]
struct Extra {
    notation: bool,
    unparsed: bool,
    extref: bool,
}

/// How a probe's `run()` printed its summary line.
#[derive(Clone, Copy)]
struct Summary {
    /// Whether the handler list is printed.
    handlers: bool,
    /// Whether `,NS` follows the handler list for namespace parsers.
    ns_marker: bool,
    /// Whether the final position is printed.
    pos: bool,
}

/// Replays a `run()`-style probe table.
fn replay(table: &[Entry], style: Style, extra: Extra, summary: Summary, forced_handlers: Option<&str>) -> Vec<u8> {
    let mut out = Vec::new();
    for entry in table {
        match entry {
            Entry::Section(title) => {
                out.extend_from_slice(title.as_bytes());
                out.push(b'\n');
            }
            Entry::Case { doc, handlers, ns, .. } => {
                let mut h = Handlers::from_list(forced_handlers.unwrap_or(handlers));
                h.notation = extra.notation;
                h.unparsed = extra.unparsed;
                h.extref = extra.extref;
                let mut session = Session::new(*ns, h, style);
                let status = session.parse(doc, true);
                out.extend_from_slice(&session.out);
                let pos = summary.pos.then(|| session.pos());
                let listed = summary.handlers.then_some(*handlers);
                out.extend_from_slice(&summary_line(doc, listed, summary.ns_marker && *ns, status, session.parser.error_code(), pos));
            }
        }
    }
    out
}

/// probe1: `dump_events()` with every handler and the `RESULT` trailer.
#[test]
fn probe1_event_dump() {
    let mut out = Vec::new();
    for entry in PROBE1C {
        match entry {
            Entry::Section(title) => {
                out.extend_from_slice(title.as_bytes());
                out.push(b'\n');
            }
            Entry::Case { doc, handlers, ns, case_folding_off } => {
                let mut h = Handlers::from_list(handlers);
                h.notation = true;
                h.unparsed = true;
                h.extref = true;
                let mut session = Session::new(*ns, h, Style::PROBE1);
                session.case_folding = !case_folding_off;
                let status = session.parse(doc, true);
                session.result_line(status);
                out.extend_from_slice(&session.out);
            }
        }
    }
    assert_corpus("probe1", &out, include_bytes!("fixtures/probe1c.out"));
}

/// probe2 "misc errors": cdata handler only, summary with the error message.
#[test]
fn probe2_misc_errors() {
    let mut out = Vec::new();
    for entry in PROBE2MISC {
        match entry {
            Entry::Section(title) => {
                out.extend_from_slice(title.as_bytes());
                out.push(b'\n');
            }
            Entry::Case { doc, .. } => {
                let h = Handlers { cdata: true, ..Handlers::default() };
                let mut session = Session::new(false, h, Style::RUN_NOPOS);
                let status = session.parse(doc, true);
                out.extend_from_slice(&session.out);
                let code = session.parser.error_code();
                out.extend_from_slice(&var_export(doc));
                out.extend_from_slice(
                    format!(" => {status} code={code} '{}' {}\n", error_string(code), session.pos()).as_bytes(),
                );
            }
        }
    }
    assert_corpus("probe2 misc errors", &out, include_bytes!("fixtures/probe2misc.out"));
}

/// probe4: entities, encodings, character references, names, namespaces.
#[test]
fn probe4_entities_and_names() {
    let extra = Extra { notation: false, unparsed: false, extref: true };
    let out = replay(PROBE4C, Style::RUN_NOPOS, extra, Summary { handlers: true, ns_marker: false, pos: false }, None);
    assert_corpus("probe4", &out, include_bytes!("fixtures/probe4c.out"));
}

/// probe5: XML declaration variants and the internal subset.
#[test]
fn probe5_declarations() {
    let extra = Extra { notation: true, unparsed: true, extref: true };
    let out = replay(PROBE5C, Style::RUN_NOPOS, extra, Summary { handlers: false, ns_marker: false, pos: true }, Some("start,cdata,default"));
    assert_corpus("probe5", &out, include_bytes!("fixtures/probe5c.out"));
}

/// probe6: positions of every event kind and error positions.
#[test]
fn probe6_positions() {
    let extra = Extra { notation: true, unparsed: true, extref: true };
    let out = replay(PROBE6C, Style::RUN6, extra, Summary { handlers: true, ns_marker: true, pos: true }, None);
    assert_corpus("probe6", &out, include_bytes!("fixtures/probe6c.out"));
}

/// probe8: BOM, carriage returns, defaulted attributes, namespace edge cases.
#[test]
fn probe8_line_endings_and_namespaces() {
    let extra = Extra { notation: true, unparsed: true, extref: true };
    let out = replay(PROBE8C, Style::RUN8, extra, Summary { handlers: true, ns_marker: true, pos: true }, None);
    assert_corpus("probe8", &out, include_bytes!("fixtures/probe8c.out"));
}

/// probe9: declared encodings, xmlns URI validity, attribute entity handling, limits of
/// the slow character-data path.
#[test]
fn probe9_encodings_and_uris() {
    let extra = Extra { notation: false, unparsed: false, extref: true };
    let out = replay(PROBE9C, Style::RUN9, extra, Summary { handlers: true, ns_marker: true, pos: true }, None);
    assert_corpus("probe9", &out, include_bytes!("fixtures/probe9c.out"));
}

/// probe1 "error strings": the php-src message table for codes 0..=23.
#[test]
fn error_strings_match_php() {
    let expected = "0: 'No error'\n1: 'No memory'\n2: 'Invalid document start'\n3: 'Empty document'\n4: 'Not well-formed (invalid token)'\n5: 'Invalid document end'\n6: 'Invalid hexadecimal character reference'\n7: 'Invalid decimal character reference'\n8: 'Invalid character reference'\n9: 'Invalid character'\n10: 'XML_ERR_CHARREF_AT_EOF'\n11: 'XML_ERR_CHARREF_IN_PROLOG'\n12: 'XML_ERR_CHARREF_IN_EPILOG'\n13: 'XML_ERR_CHARREF_IN_DTD'\n14: 'XML_ERR_ENTITYREF_AT_EOF'\n15: 'XML_ERR_ENTITYREF_IN_PROLOG'\n16: 'XML_ERR_ENTITYREF_IN_EPILOG'\n17: 'XML_ERR_ENTITYREF_IN_DTD'\n18: 'PEReference at end of document'\n19: 'PEReference in prolog'\n20: 'PEReference in epilog'\n21: 'PEReference: forbidden within markup decl in internal subset'\n22: 'XML_ERR_ENTITYREF_NO_NAME'\n23: 'EntityRef: expecting \\';\\''\n";
    let mut out = String::new();
    for code in 0..=23 {
        out.push_str(&format!("{code}: {}\n", String::from_utf8_lossy(&var_export(error_string(code).as_bytes()))));
    }
    assert_eq!(out, expected);
}

/// `var_export` as text for ASCII-only fixtures.
fn ve(s: &[u8]) -> String {
    String::from_utf8_lossy(&var_export(s)).into_owned()
}

/// probe2 "chunks": 7-byte chunks with positions after each call, then parse-after-final.
#[test]
fn probe2_chunked_feeding() {
    let expected = "S ROOT [] @5\nchunk '<root><' => 1 line=1 col=7 byte=6\nchunk 'item a=' => 1 line=1 col=7 byte=6\nS ITEM {\"A\":\"12\"} @18\nchunk '\\'12\\'>he' => 1 line=1 col=20 byte=19\nchunk 'llo wor' => 1 line=1 col=20 byte=19\nC 'hello world' @30\nchunk 'ld</ite' => 1 line=1 col=31 byte=30\nE ITEM @37\nS X [] @39\nE X @41\nchunk 'm><x/><' => 1 line=1 col=42 byte=41\nE ROOT @48\nchunk '/root>' => 1 line=1 col=49 byte=48\nint(1)\n== parse after final\nint(0)\nint(5)\n";
    let mut parser = Parser::new(None);
    let mut out = String::new();
    let doc = b"<root><item a='12'>hello world</item><x/></root>";
    let render = |parser: &mut Parser, out: &mut String| loop {
        match parser.next() {
            Step::Event(Event::StartElement { name, attributes, .. }) => {
                let pairs: Vec<(Vec<u8>, Vec<u8>)> =
                    attributes.iter().map(|a| (a.name.to_ascii_uppercase(), a.value.clone())).collect();
                out.push_str(&format!(
                    "S {} {} @{}\n",
                    String::from_utf8_lossy(&name.to_ascii_uppercase()),
                    super::harness::json_assoc(&pairs),
                    parser.byte_index()
                ));
            }
            Step::Event(Event::EndElement { name, .. }) => {
                out.push_str(&format!("E {} @{}\n", String::from_utf8_lossy(&name.to_ascii_uppercase()), parser.byte_index()));
            }
            Step::Event(Event::Characters(text)) => {
                out.push_str(&format!("C {} @{}\n", ve(&text), parser.byte_index()));
            }
            Step::Event(_) => {}
            _ => break,
        }
    };
    for chunk in doc.chunks(7) {
        parser.feed(chunk, false);
        render(&mut parser, &mut out);
        let status = i32::from(parser.is_well_formed());
        out.push_str(&format!(
            "chunk {} => {status} line={} col={} byte={}\n",
            ve(chunk),
            parser.line(),
            parser.column(),
            parser.byte_index()
        ));
    }
    parser.feed(b"", true);
    render(&mut parser, &mut out);
    out.push_str(&format!("int({})\n", i32::from(parser.is_well_formed())));
    out.push_str("== parse after final\n");
    parser.feed(b"<a/>", true);
    render(&mut parser, &mut out);
    out.push_str(&format!("int({})\nint({})\n", i32::from(parser.is_well_formed()), parser.error_code()));
    assert_eq!(out, expected);
}

/// probe2 "is_final=false then error": a fatal error freezes the status for later chunks.
#[test]
fn probe2_error_in_partial_chunk_is_sticky() {
    let mut session = Session::new(false, Handlers::default(), Style::RUN_NOPOS);
    assert_eq!(session.parse(b"<a><b>", false), 1);
    assert_eq!(session.parse(b"</a>", false), 0);
    assert_eq!(session.parser.error_code(), 76);
    assert_eq!(session.parse(b"", true), 0);
    assert_eq!(session.parser.error_code(), 76);
}

/// probe2 "extref false": a handler answering false stops the parser with code 21. The
/// events libxml2 had already produced past the reference (`C 'b'`, `E R`) are never
/// dispatched, the position stays where the handler read it (PHP: `1:48/47`, the cursor
/// after `&x;`), and a later chunk keeps the status and code (PHP-verified).
#[test]
fn probe2_external_entity_handler_false_stops() {
    let h = Handlers { cdata: true, extref: true, ..Handlers::default() };
    let mut session = Session::new(false, h, Style::RUN_NOPOS);
    session.extref_returns_false = true;
    let status = session.parse(b"<!DOCTYPE r [<!ENTITY x SYSTEM \"xsys\">]><r>a&x;b</r>", true);
    assert_eq!(session.out, b"  C 'a'\n  EXTREF [\"x\",\"\",\"xsys\",false]\n");
    assert_eq!(status, 0);
    assert_eq!(session.parser.error_code(), 21);
    assert_eq!(session.pos(), "@1:48/47");
    assert_eq!(error_string(21), "PEReference: forbidden within markup decl in internal subset");
    assert_eq!(session.parse(b"<z/>", true), 0);
    assert_eq!(session.parser.error_code(), 21);
    assert_eq!(session.pos(), "@1:48/47");
    assert_eq!(session.out, b"  C 'a'\n  EXTREF [\"x\",\"\",\"xsys\",false]\n");
}

/// probe2 "undefined entity": the default handler sees the reference before the fatal error.
#[test]
fn probe2_undefined_entity_reaches_default_handler() {
    let h = Handlers { cdata: true, default: true, ..Handlers::default() };
    let mut session = Session::new(false, h, Style::RUN_NOPOS);
    let status = session.parse(b"<a>x&nope;y</a>", true);
    assert_eq!(session.out, b"  D '<a>'\n  C 'x'\n  D '&nope;'\n");
    assert_eq!((status, session.parser.error_code()), (0, 26));
    assert_eq!(error_string(26), "Undeclared entity error");
}

/// probe2 "ns errors": custom separator, undefined prefixes, redefined attributes.
#[test]
fn probe2_namespace_errors_with_pipe_separator() {
    let cases: [(&[u8], &str, i32); 4] = [
        (b"<a><p:b/></a>", "  S A []\n  S B []\n  E B\n  E A\n", 201),
        (b"<p:a xmlns:p='u'><p:b q:c='1'/></p:a>", "  NS 'p' 'u'\n  S U|A []\n  S U|B {\"C\":\"1\"}\n  E U|B\n  E U|A\n", 201),
        (b"<a xmlns='u1'><b xmlns='u2'/><c/></a>", "  NS false 'u1'\n  S U1|A []\n  NS false 'u2'\n  S U2|B []\n  E U2|B\n  S U1|C []\n  E U1|C\n  E U1|A\n", 0),
        (b"<a xmlns:p='u1' p:x='1' xmlns:q='u1' q:x='2'/>", "  NS 'p' 'u1'\n  NS 'q' 'u1'\n  S A {\"U1|X\":\"2\"}\n  E A\n", 203),
    ];
    for (doc, expected, code) in cases {
        let h = Handlers { start: true, ns: true, extref: true, ..Handlers::default() };
        let mut session = Session::with_separator(b"|", h, Style::RUN_NOPOS);
        let status = session.parse(doc, true);
        assert_eq!(String::from_utf8_lossy(&session.out), expected, "{}", String::from_utf8_lossy(doc));
        assert_eq!(session.parser.error_code(), code);
        assert_eq!(status, i32::from(code == 0));
    }
}

/// probe2 "utf8 col counting": columns count code points, byte index counts bytes.
#[test]
fn probe2_utf8_columns() {
    let h = Handlers { start: true, cdata: true, extref: true, ..Handlers::default() };
    let mut session = Session::new(false, h, Style::RUN6);
    session.parse("<a>ééé<b/>x€y\n<c/></a>".as_bytes(), true);
    assert_eq!(
        String::from_utf8_lossy(&session.out),
        "  S A [] @1:3/2\n  C 'ééé' @1:7/9\n  S B [] @1:9/11\n  E B @1:11/13\n  C 'x' @1:12/14\n  C '€y\n' @2:1/19\n  S C [] @2:3/21\n  E C @2:5/23\n  E A @2:9/27\n"
    );
}

/// probe2 "long non-ascii text": the slow path flushes every 300 bytes.
#[test]
fn probe2_slow_path_flushes_at_300_bytes() {
    for (doc, expected) in [
        (format!("<a>{}</a>", "é".repeat(400)), "C len=300\nC len=300\nC len=200\n"),
        (format!("<a>{}</a>", "abé".repeat(200)), "C len=2\nC len=300\nC len=300\nC len=198\n"),
    ] {
        let mut parser = Parser::new(None);
        parser.feed(doc.as_bytes(), true);
        let mut out = String::new();
        loop {
            match parser.next() {
                Step::Event(Event::Characters(text)) => out.push_str(&format!("C len={}\n", text.len())),
                Step::Event(_) => {}
                _ => break,
            }
        }
        assert_eq!(out, expected);
    }
}

/// probe4 "byte index with chunks and multibyte": an incomplete trailing sequence waits.
#[test]
fn probe4_incomplete_utf8_sequence_waits_for_the_next_chunk() {
    let h = Handlers { cdata: true, ..Handlers::default() };
    let mut session = Session::new(false, h, Style::RUN6);
    session.parse(b"<a>\xc3", false);
    assert_eq!(session.pos(), "@1:4/3");
    session.parse(b"\xa9x</a>", true);
    assert_eq!(String::from_utf8_lossy(&session.out), "  C 'éx' @1:6/6\n");
    assert_eq!(session.pos(), "@1:10/10");
}

/// probe4 "deep nesting" / probe5 limits: depth is unlimited, oversized chunks are
/// `XML_ERR_RESOURCE_LIMIT` unless huge mode is on.
#[test]
fn limits_follow_libxml() {
    let deep = format!("{}x{}", "<a>".repeat(3000), "</a>".repeat(3000));
    let h = Handlers { start: true, ..Handlers::default() };
    let mut session = Session::new(false, h, Style::RUN_NOPOS);
    assert_eq!(session.parse(deep.as_bytes(), true), 1);
    assert_eq!(session.parser.error_code(), 0);

    for (len, huge, expect_status, expect_code, expect_got) in [
        (9_999_999usize, false, 0, 114, 9_999_999usize),
        (10_000_010, false, 0, 114, 10_000_010),
        (10_000_010, true, 1, 0, 10_000_010),
    ] {
        let doc = format!("<a>{}</a>", "x".repeat(len));
        let mut parser = Parser::new(None);
        parser.set_parse_huge(huge);
        parser.feed(doc.as_bytes(), true);
        let mut got = 0;
        loop {
            match parser.next() {
                Step::Event(Event::Characters(text)) => got += text.len(),
                Step::Event(_) => {}
                _ => break,
            }
        }
        assert_eq!(got, expect_got, "len {len}");
        assert_eq!(i32::from(parser.is_well_formed()), expect_status, "len {len}");
        assert_eq!(parser.error_code(), expect_code, "len {len}");
    }
    let mut parser = Parser::new(None);
    parser.feed(format!("<a><!--{}--></a>", "x".repeat(10_000_010)).as_bytes(), true);
    while let Step::Event(_) = parser.next() {}
    assert_eq!(parser.error_code(), 45);
    let mut parser = Parser::new(None);
    parser.feed(format!("<a><![CDATA[{}]]></a>", "x".repeat(10_000_010)).as_bytes(), true);
    let mut got = 0;
    loop {
        match parser.next() {
            Step::Event(Event::Characters(text)) => got += text.len(),
            Step::Event(_) => {}
            _ => break,
        }
    }
    assert_eq!((got, parser.error_code()), (0, 114));
    let mut parser = Parser::new(None);
    parser.feed(format!("<{}/>", "x".repeat(60_000)).as_bytes(), true);
    while let Step::Event(_) = parser.next() {}
    assert_eq!(parser.error_code(), 68);
}

/// probe4 "ns separator variants": an empty separator joins URI and name directly, and
/// only the FIRST byte of a longer separator is used (compat.c `xmlStrncat(.., sep, 1)`),
/// so a multi-byte `é` contributes its lead byte alone and `ab` contributes `a`.
#[test]
fn probe4_namespace_separator_variants() {
    let h = Handlers { start: true, ..Handlers::default() };
    let mut session = Session::with_separator(b"", h, Style::RUN_NOPOS);
    assert_eq!(session.parse(b"<r xmlns='u'><a/></r>", true), 1);
    assert_eq!(session.out, b"  S UR []\n  S UA []\n  E UA\n  E UR\n");
    let mut session = Session::with_separator("é".as_bytes(), h, Style::RUN_NOPOS);
    assert_eq!(session.parse(b"<r xmlns='u'/>", true), 1);
    assert_eq!(session.out, b"  S U\xc3R []\n  E U\xc3R\n".to_vec());
    let mut session = Session::with_separator(b"ab", h, Style::RUN_NOPOS);
    assert_eq!(session.parse(b"<r xmlns='u'><a/></r>", true), 1);
    assert_eq!(session.out, b"  S UAR []\n  S UAA []\n  E UAA\n  E UAR\n");
}

/// A stopped parser ignores later chunks and keeps its code; a finished one reports
/// `XML_ERR_DOCUMENT_END` only for leftover data in a final chunk.
#[test]
fn stop_and_post_finish_semantics() {
    let mut parser = Parser::new(None);
    parser.feed(b"<ab>", false);
    assert!(matches!(parser.next(), Step::Event(_)));
    parser.stop(0);
    assert_eq!(parser.next(), Step::Failed);
    parser.feed(b"</ab>", true);
    assert_eq!(parser.next(), Step::Failed);
    assert_eq!(parser.error_code(), 0);

    let mut parser = Parser::new(None);
    parser.feed(b"<a/>", true);
    while let Step::Event(_) = parser.next() {}
    assert_eq!(parser.next(), Step::Finished);
    parser.feed(b"<z/>", false);
    assert_eq!(parser.next(), Step::NeedMoreData);
    assert_eq!(parser.error_code(), 0);
    parser.feed(b"", true);
    assert_eq!(parser.next(), Step::Failed);
    assert_eq!(parser.error_code(), 5);
}

/// A trailing `\r` in a non-final chunk is held back until the next chunk, so `\r\n`
/// split across chunks still counts as one line break.
#[test]
fn held_carriage_return_across_chunks() {
    let h = Handlers { cdata: true, ..Handlers::default() };
    let mut session = Session::new(false, h, Style::RUN6);
    session.parse(b"<a>x\r", false);
    session.parse(b"\ny</a>", true);
    assert_eq!(session.out, b"  C 'x' @1:5/4\n  C '\ny' @2:2/7\n");
}

/// probe2 "xml decl and doctype": the declaration and DOCTYPE never reach the default
/// handler, comments and PIs in the prolog, subset and epilog do.
#[test]
fn probe2_prolog_items_and_default_handler() {
    let h = Handlers { start: true, default: true, pi: true, extref: true, ..Handlers::default() };
    let mut session = Session::new(false, h, Style::RUN_NOPOS);
    session.parse(
        b"<?xml version='1.0' encoding='UTF-8' standalone='yes'?>\n<!DOCTYPE a SYSTEM 'a.dtd' [ <!ELEMENT a ANY> <!ATTLIST a x CDATA #IMPLIED> <!ENTITY e 'v'> ]>\n<?p1 d1?><a>&e;<!--x--><![CDATA[c]]></a>\n<!-- after -->\n<?pi2?>",
        true,
    );
    assert_eq!(
        session.out,
        b"  PI 'p1' 'd1'\n  S A []\n  D '&e;'\n  D '<!--x-->'\n  D 'c'\n  E A\n  D '<!-- after -->'\n  PI 'pi2' false\n"
    );
    assert_eq!((session.parser.error_code(), session.parser.is_well_formed()), (0, true));
}

/// Delivers every queued event, rendering `S NAME:code` / `E NAME:code` / `C 'text':code`
/// with the error code reported WHILE the event is being delivered (what a PHP handler
/// reads), and answers the non-event `Step` that ended the round.
fn drain_with_codes(parser: &mut Parser, out: &mut String) -> Step {
    loop {
        match parser.next() {
            Step::Event(Event::StartElement { name, .. }) => {
                out.push_str(&format!("S {}:{}\n", String::from_utf8_lossy(&name.to_ascii_uppercase()), parser.error_code()));
            }
            Step::Event(Event::EndElement { name, .. }) => {
                out.push_str(&format!("E {}:{}\n", String::from_utf8_lossy(&name.to_ascii_uppercase()), parser.error_code()));
            }
            Step::Event(Event::Characters(text)) => {
                out.push_str(&format!("C {}:{}\n", ve(&text), parser.error_code()));
            }
            Step::Event(_) => {}
            other => return other,
        }
    }
}

/// The error code a handler reads is libxml2's state at that event, not the chunk's
/// final state: `<root><ok/></wrong>` fed at once dispatches ROOT and OK with code 0
/// (PHP: `ROOT:0 OK:0`), and only the read after the round sees the mismatched tag (76).
#[test]
fn error_code_inside_events_is_the_state_at_that_event() {
    let mut parser = Parser::new(None);
    parser.feed(b"<root><ok/></wrong>", true);
    let mut out = String::new();
    assert_eq!(drain_with_codes(&mut parser, &mut out), Step::Failed);
    assert_eq!(out, "S ROOT:0\nS OK:0\nE OK:0\n");
    assert_eq!(parser.error_code(), 76);
    assert!(!parser.is_well_formed(), "the chunk's outcome is never snapshotted");
}

/// The same document split before the bad end tag: the first chunk's events and the read
/// after it answer 0, the final chunk dispatches nothing and answers 76.
#[test]
fn error_code_snapshot_across_chunks() {
    let mut parser = Parser::new(None);
    parser.feed(b"<root><ok/>", false);
    let mut out = String::new();
    assert_eq!(drain_with_codes(&mut parser, &mut out), Step::NeedMoreData);
    assert_eq!(out, "S ROOT:0\nS OK:0\nE OK:0\n");
    assert_eq!((parser.error_code(), parser.is_well_formed()), (0, true));
    parser.feed(b"</wrong>", true);
    out.clear();
    assert_eq!(drain_with_codes(&mut parser, &mut out), Step::Failed);
    assert_eq!(out, "");
    assert_eq!((parser.error_code(), parser.is_well_formed()), (76, false));
}

/// A non-fatal error (undefined namespace prefix, 201) is raised before the start
/// callback of the offending tag and stays set, so the events from that tag on see it
/// while the earlier ones still see 0 (PHP-verified: `S R:0 C 'a':0 S B:201 ... C 'b':201`).
#[test]
fn error_code_snapshot_after_a_non_fatal_error_in_the_same_chunk() {
    let mut parser = Parser::new(Some(b"|".to_vec()));
    parser.feed(b"<r>a<p:b/>b<c/>c</r>", true);
    let mut out = String::new();
    assert_eq!(drain_with_codes(&mut parser, &mut out), Step::Failed);
    assert_eq!(out, "S R:0\nC 'a':0\nS B:201\nE B:201\nC 'b':201\nS C:201\nE C:201\nC 'c':201\nE R:201\n");
    assert_eq!((parser.error_code(), parser.is_well_formed()), (201, false));
}

/// `stop()` wins over the delivered event's snapshot: the external reference is
/// delivered with code 0 although the chunk later fails with 76, and once the handler's
/// `false` stops the parser with 21 that code is reported immediately, after the round
/// and after a later chunk. `stop(0)` keeps the snapshot (0) rather than the chunk's later
/// error, since the parse ended at that event.
#[test]
fn stop_code_wins_over_the_event_snapshot() {
    let mut parser = Parser::new(None);
    parser.feed(b"<!DOCTYPE r [<!ENTITY x SYSTEM \"xsys\">]><r>a&x;b</wrong>", true);
    assert!(matches!(parser.next(), Step::Event(Event::StartElement { .. })));
    assert_eq!(parser.error_code(), 0);
    assert!(matches!(parser.next(), Step::Event(Event::Characters(_))));
    assert_eq!(parser.error_code(), 0);
    assert!(matches!(parser.next(), Step::Event(Event::EntityRef { .. })));
    assert_eq!(parser.error_code(), 0, "the handler sees the state at its event");
    parser.stop(21);
    assert_eq!(parser.error_code(), 21, "the stop code is reported while the event is still current");
    assert_eq!(parser.next(), Step::Failed);
    assert_eq!((parser.error_code(), parser.is_well_formed()), (21, false));
    parser.feed(b"<z/>", true);
    assert_eq!(parser.next(), Step::Failed);
    assert_eq!(parser.error_code(), 21);

    let mut parser = Parser::new(None);
    parser.feed(b"<root><ok/></wrong>", true);
    assert!(matches!(parser.next(), Step::Event(Event::StartElement { .. })));
    parser.stop(0);
    assert_eq!(parser.error_code(), 0);
    assert_eq!(parser.next(), Step::Failed);
    assert_eq!(parser.error_code(), 0, "stop(0) keeps the code the event saw, not the chunk's later 76");
}
