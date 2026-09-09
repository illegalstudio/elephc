//! Purpose:
//! Regenerates the self-contained PHP probe scripts `fixtures/<name>.php` from the replay
//! tables in `corpus_data` and re-runs them with `php` against the committed
//! `fixtures/<name>.out`, so an independent reviewer can reproduce every parser corpus
//! fixture with PHP 8.5.10 / libxml2 2.15.3 even though the original probe scripts are
//! not in the tree.
//!
//! Called from:
//! - `cargo test -p elephc-xml --lib -- regen --ignored`: `write_scripts` writes the seven
//!   scripts; `php_reproduces_fixtures` (re)writes them and runs `php` on each one,
//!   asserting the output equals the `.out` file byte for byte (skipped with a message
//!   when no `php` is on `PATH`).
//! - `cargo test -p elephc-xml --lib` for `committed_scripts_are_current`, which fails when
//!   a committed script no longer matches the tables or this generator.
//!
//! Key details:
//! - Needs no libxml2 (`cfg(test)` only, never `cfg(elephc_xml_native)`): it reads the
//!   tables and shells out to `php`.
//! - The handlers a script registers print exactly what `harness::Session` renders: the
//!   shared `style::Style` decides which lines carry `@line:col/byte`, and each `Corpus`
//!   mirrors the `Extra` / `Summary` / forced-handler arguments of the `probes` tests.
//! - Documents and section titles are emitted as double-quoted PHP literals with `\xHH`
//!   for every byte outside printable ASCII, so invalid UTF-8, NUL and CR bytes reach
//!   `xml_parse()` unchanged.
//! - Every table case is one `xml_parse($p, $doc, true)` call: the tables record no
//!   chunked feeds (those probe cases live as hand-written tests in `probes`).
//! - Scripts are replaced through a rename, so a `php` started by the other ignored test
//!   never reads a half-written file.

use std::io::ErrorKind;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

use super::corpus_data::{Entry, PROBE1C, PROBE2MISC, PROBE4C, PROBE5C, PROBE6C, PROBE8C, PROBE9C};
use super::style::Style;

/// The command a reviewer runs to rewrite the scripts.
const REGEN_COMMAND: &str = "cargo test -p elephc-xml --lib -- regen --ignored";

/// The PHP / libxml2 releases the committed fixtures were recorded with.
const RECORDED_WITH: &str = "PHP 8.5.10 / libxml2 2.15.3";

/// How a script's per-case function ends each case.
#[derive(Clone, Copy)]
enum Summary {
    /// probe1: `RESULT <status> code=<code> msg='<message>' @pos` then a `-----` rule.
    Result,
    /// probe2 "misc errors": `'<doc>' => <status> code=<code> '<message>' @pos`.
    Message,
    /// probes 4-9 (`harness::summary_line`): `'<doc>' [<handlers>,NS] => <status> code=<code> @pos`,
    /// the handler list, its `,NS` marker and the position each optional.
    Run { handlers: bool, ns_marker: bool, pos: bool },
}

/// One corpus: its replay table, the probe's printing style and its `run()` shape.
struct Corpus {
    /// Fixture stem (`probe6c` is `fixtures/probe6c.php` / `fixtures/probe6c.out`).
    name: &'static str,
    /// Name of the replay table in `corpus_data.rs`, quoted in the script header.
    table_name: &'static str,
    /// The replay table.
    table: &'static [Entry],
    /// Which lines carry positions and which words spell the events.
    style: Style,
    /// Handlers the probe registered on every parser besides the per-case list
    /// (`probes::Extra`).
    notation: bool,
    unparsed: bool,
    extref: bool,
    /// A handler list registered instead of the table's (`probes::replay`'s
    /// `forced_handlers`, and the fixed `cdata` handler of the probe2 test).
    forced_handlers: Option<&'static str>,
    /// The summary line shape.
    summary: Summary,
    /// The recorded PHP 8.5.10 output.
    expected: &'static [u8],
}

/// The seven corpora, each mirroring the `probes` test that replays its table.
static CORPORA: &[Corpus] = &[
    Corpus {
        name: "probe1c",
        table_name: "PROBE1C",
        table: PROBE1C,
        style: Style::PROBE1,
        notation: true,
        unparsed: true,
        extref: true,
        forced_handlers: None,
        summary: Summary::Result,
        expected: include_bytes!("fixtures/probe1c.out"),
    },
    Corpus {
        name: "probe2misc",
        table_name: "PROBE2MISC",
        table: PROBE2MISC,
        style: Style::RUN_NOPOS,
        notation: false,
        unparsed: false,
        extref: false,
        forced_handlers: Some("cdata"),
        summary: Summary::Message,
        expected: include_bytes!("fixtures/probe2misc.out"),
    },
    Corpus {
        name: "probe4c",
        table_name: "PROBE4C",
        table: PROBE4C,
        style: Style::RUN_NOPOS,
        notation: false,
        unparsed: false,
        extref: true,
        forced_handlers: None,
        summary: Summary::Run { handlers: true, ns_marker: false, pos: false },
        expected: include_bytes!("fixtures/probe4c.out"),
    },
    Corpus {
        name: "probe5c",
        table_name: "PROBE5C",
        table: PROBE5C,
        style: Style::RUN_NOPOS,
        notation: true,
        unparsed: true,
        extref: true,
        forced_handlers: Some("start,cdata,default"),
        summary: Summary::Run { handlers: false, ns_marker: false, pos: true },
        expected: include_bytes!("fixtures/probe5c.out"),
    },
    Corpus {
        name: "probe6c",
        table_name: "PROBE6C",
        table: PROBE6C,
        style: Style::RUN6,
        notation: true,
        unparsed: true,
        extref: true,
        forced_handlers: None,
        summary: Summary::Run { handlers: true, ns_marker: true, pos: true },
        expected: include_bytes!("fixtures/probe6c.out"),
    },
    Corpus {
        name: "probe8c",
        table_name: "PROBE8C",
        table: PROBE8C,
        style: Style::RUN8,
        notation: true,
        unparsed: true,
        extref: true,
        forced_handlers: None,
        summary: Summary::Run { handlers: true, ns_marker: true, pos: true },
        expected: include_bytes!("fixtures/probe8c.out"),
    },
    Corpus {
        name: "probe9c",
        table_name: "PROBE9C",
        table: PROBE9C,
        style: Style::RUN9,
        notation: false,
        unparsed: false,
        extref: true,
        forced_handlers: None,
        summary: Summary::Run { handlers: true, ns_marker: true, pos: true },
        expected: include_bytes!("fixtures/probe9c.out"),
    },
];

/// A double-quoted PHP string literal holding exactly `bytes`.
fn php_literal(bytes: &[u8]) -> String {
    let mut out = String::from("\"");
    for &b in bytes {
        match b {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            b'$' => out.push_str("\\$"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0x20..=0x7e => out.push(b as char),
            _ => out.push_str(&format!("\\x{b:02x}")),
        }
    }
    out.push('"');
    out
}

/// Rejects a handler list the probes never spelled (`harness::Handlers::from_list`).
fn check_handler_list(list: &str) {
    for name in list.split(',') {
        assert!(
            matches!(name, "start" | "end" | "cdata" | "default" | "pi" | "ns" | ""),
            "unknown handler {name:?} in {list:?}"
        );
    }
}

impl Corpus {
    /// The per-case PHP function's name.
    fn case_function(&self) -> &'static str {
        if self.style.probe1 {
            "dump_events"
        } else {
            "run"
        }
    }

    /// The full PHP script text.
    fn script(&self) -> String {
        let mut php = String::new();
        self.push_header(&mut php);
        self.push_position_function(&mut php);
        self.push_case_function(&mut php);
        self.push_calls(&mut php);
        php
    }

    /// The `<?php` line and the provenance comment.
    fn push_header(&self, php: &mut String) {
        php.push_str("<?php\n");
        php.push_str(&format!("// {}.php -- generated from the {} replay table in\n", self.name, self.table_name));
        php.push_str("// crates/elephc-xml/src/parser/tests/corpus_data.rs by\n");
        php.push_str(&format!("//   {REGEN_COMMAND}\n"));
        php.push_str(&format!("// Do not edit by hand. Re-records its fixture under {RECORDED_WITH}:\n"));
        php.push_str(&format!("//   php fixtures/{0}.php > fixtures/{0}.out\n\n", self.name));
    }

    /// `position()`: the `@line:col/byte` spelling of the parser's current position.
    fn push_position_function(&self, php: &mut String) {
        php.push_str("/** `@line:col/byte` of the parser's current position. */\n");
        php.push_str("function position(XMLParser $parser): string\n{\n");
        php.push_str("    return '@' . xml_get_current_line_number($parser) . ':' . xml_get_current_column_number($parser)\n");
        php.push_str("        . '/' . xml_get_current_byte_index($parser);\n}\n\n");
    }

    /// The per-case function: creates the parser, registers the handlers that print the
    /// probe's lines, runs one `xml_parse()` and prints the summary line.
    fn push_case_function(&self, php: &mut String) {
        let style = self.style;
        let indent = if style.probe1 { "" } else { "  " };
        let (start, end, cdata, default, ns) = if style.probe1 {
            ("START", "END", "CDATA", "DEFAULT", "NSSTART")
        } else {
            ("S", "E", "C", "D", "NS")
        };
        let tag = |word: &str| format!("'{indent}{word} '");
        let name = if style.probe1 { "var_export($name, true)" } else { "$name" };
        // The position suffix of an event line, when the style prints one.
        let pos = |enabled: bool| if enabled { ", ' ', position($parser)" } else { "" };
        // probe1's DEFAULT and NOTATION lines never carried a position (`harness::Session::dispatch`).
        let default_pos = pos(style.event_pos && !style.probe1);

        php.push_str(&format!("/** One xml_parse() of $doc with the listed handlers, then the {} line. */\n", match self.summary {
            Summary::Result => "RESULT",
            Summary::Message | Summary::Run { .. } => "summary",
        }));
        let case_folding_param = if style.probe1 { ", bool $caseFoldingOff" } else { "" };
        php.push_str(&format!("function {}(string $doc, string $handlers, bool $ns{case_folding_param}): void\n{{\n", self.case_function()));
        php.push_str("    $p = $ns ? xml_parser_create_ns() : xml_parser_create();\n");
        if style.probe1 {
            php.push_str("    if ($caseFoldingOff) {\n        xml_parser_set_option($p, XML_OPTION_CASE_FOLDING, 0);\n    }\n");
        }
        php.push_str("    // 'end' rides along with 'start': xml_set_element_handler() sets both callbacks.\n");
        php.push_str("    $wanted = array_flip(explode(',', $handlers));\n");
        php.push_str("    if (isset($wanted['start'])) {\n");
        php.push_str("        xml_set_element_handler(\n            $p,\n");
        php.push_str(&format!(
            "            function ($parser, $name, $attrs) {{\n                echo {}, {name}, ' ', json_encode($attrs){}, \"\\n\";\n            }},\n",
            tag(start),
            pos(style.event_pos)
        ));
        php.push_str(&format!(
            "            function ($parser, $name) {{\n                echo {}, {name}{}, \"\\n\";\n            }}\n        );\n    }}\n",
            tag(end),
            pos(style.event_pos)
        ));
        php.push_str(&format!(
            "    if (isset($wanted['cdata'])) {{\n        xml_set_character_data_handler($p, function ($parser, $data) {{\n            echo {}, var_export($data, true){}, \"\\n\";\n        }});\n    }}\n",
            tag(cdata),
            pos(style.event_pos)
        ));
        php.push_str(&format!(
            "    if (isset($wanted['default'])) {{\n        xml_set_default_handler($p, function ($parser, $data) {{\n            echo {}, var_export($data, true){default_pos}, \"\\n\";\n        }});\n    }}\n",
            tag(default)
        ));
        php.push_str(&format!(
            "    if (isset($wanted['pi'])) {{\n        xml_set_processing_instruction_handler($p, function ($parser, $target, $data) {{\n            echo {}, var_export($target, true), ' ', var_export($data, true){}, \"\\n\";\n        }});\n    }}\n",
            tag("PI"),
            pos(style.pi_pos)
        ));
        php.push_str(&format!(
            "    if (isset($wanted['ns'])) {{\n        xml_set_start_namespace_decl_handler($p, function ($parser, $prefix, $uri) {{\n            echo {}, var_export($prefix, true), ' ', var_export($uri, true){}, \"\\n\";\n        }});\n    }}\n",
            tag(ns),
            pos(style.ns_pos)
        ));
        if self.notation {
            php.push_str(&format!(
                "    xml_set_notation_decl_handler($p, function ($parser, $name, $base, $systemId, $publicId) {{\n        echo {}, json_encode([$name, $base, $systemId, $publicId]){default_pos}, \"\\n\";\n    }});\n",
                tag("NOTATION")
            ));
        }
        if self.unparsed {
            php.push_str(&format!(
                "    xml_set_unparsed_entity_decl_handler($p, function ($parser, $name, $base, $systemId, $publicId, $notation) {{\n        echo {}, json_encode([$name, $base, $systemId, $publicId, $notation]){}, \"\\n\";\n    }});\n",
                tag("UNPARSED"),
                pos(style.unparsed_pos)
            ));
        }
        if self.extref {
            php.push_str(&format!(
                "    xml_set_external_entity_ref_handler($p, function ($parser, $names, $base, $systemId, $publicId) {{\n        echo {}, json_encode([$names, $base, $systemId, $publicId]){}, \"\\n\";\n        return true;\n    }});\n",
                tag("EXTREF"),
                pos(style.extref_pos)
            ));
        }
        php.push_str("    $status = xml_parse($p, $doc, true);\n");
        php.push_str("    $code = xml_get_error_code($p);\n");
        php.push_str("    echo ");
        match self.summary {
            Summary::Result => php.push_str(
                "'RESULT ', $status, ' code=', $code, ' msg=', var_export(xml_error_string($code), true), ' ', position($p), \"\\n-----\\n\"",
            ),
            Summary::Message => php.push_str(
                "var_export($doc, true), ' => ', $status, ' code=', $code, \" '\", xml_error_string($code), \"' \", position($p), \"\\n\"",
            ),
            Summary::Run { handlers, ns_marker, pos } => {
                php.push_str("var_export($doc, true)");
                if handlers {
                    php.push_str(", ' [', $handlers");
                    if ns_marker {
                        php.push_str(", $ns ? ',NS' : ''");
                    }
                    php.push_str(", ']'");
                }
                php.push_str(", ' => ', $status, ' code=', $code");
                if pos {
                    php.push_str(", ' ', position($p)");
                }
                php.push_str(", \"\\n\"");
            }
        }
        php.push_str(";\n}\n\n");
    }

    /// The section echoes and per-case calls, in table order.
    fn push_calls(&self, php: &mut String) {
        for entry in self.table {
            match entry {
                Entry::Section(title) => {
                    let mut line = title.as_bytes().to_vec();
                    line.push(b'\n');
                    php.push_str(&format!("echo {};\n", php_literal(&line)));
                }
                Entry::Case { doc, handlers, ns, case_folding_off } => {
                    let list = self.forced_handlers.unwrap_or(handlers);
                    check_handler_list(list);
                    assert!(
                        self.style.probe1 || !case_folding_off,
                        "{}: XML_OPTION_CASE_FOLDING is only modelled for probe1's dump_events()",
                        self.name
                    );
                    let case_folding_arg = if self.style.probe1 { format!(", {case_folding_off}") } else { String::new() };
                    php.push_str(&format!("{}({}, '{list}', {ns}{case_folding_arg});\n", self.case_function(), php_literal(doc)));
                }
            }
        }
    }
}

/// `fixtures/<name>.php` inside this crate's source tree.
fn script_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/parser/tests/fixtures").join(format!("{name}.php"))
}

/// Serializes script writes between the two ignored tests of one test binary.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// Writes the corpus script into `fixtures/`, replacing any previous one atomically, and
/// returns its path.
fn write_script(corpus: &Corpus) -> PathBuf {
    let path = script_path(corpus.name);
    let tmp = path.with_extension("php.tmp");
    let _guard = WRITE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    std::fs::write(&tmp, corpus.script()).unwrap_or_else(|err| panic!("cannot write {}: {err}", tmp.display()));
    std::fs::rename(&tmp, &path).unwrap_or_else(|err| panic!("cannot replace {}: {err}", path.display()));
    path
}

/// Panics with the first differing line when `actual` and `expected` differ.
fn assert_same_output(name: &str, context: &str, actual: &[u8], expected: &[u8]) {
    if actual == expected {
        return;
    }
    let actual_lines: Vec<&[u8]> = actual.split(|&b| b == b'\n').collect();
    let expected_lines: Vec<&[u8]> = expected.split(|&b| b == b'\n').collect();
    let first_diff = actual_lines
        .iter()
        .zip(expected_lines.iter())
        .position(|(a, e)| a != e)
        .unwrap_or(actual_lines.len().min(expected_lines.len()));
    let show = |lines: &[&[u8]]| -> String { lines.get(first_diff).map_or_else(|| "<end of output>".to_string(), |l| String::from_utf8_lossy(l).into_owned()) };
    panic!(
        "{name}: php output differs from fixtures/{name}.out at line {} ({context}; {} vs {} lines)\n--- php ---\n{}\n--- fixture ---\n{}\n",
        first_diff + 1,
        actual_lines.len(),
        expected_lines.len(),
        show(&actual_lines),
        show(&expected_lines)
    );
}

/// Writes `fixtures/<name>.php` for every corpus.
#[test]
#[ignore = "rewrites fixtures/*.php; run with --ignored"]
fn write_scripts() {
    for corpus in CORPORA {
        let path = write_script(corpus);
        eprintln!("wrote {}", path.display());
    }
}

/// Runs `php` on every generated script and checks it reproduces the committed `.out`
/// byte for byte; skips when no `php` is on `PATH`.
#[test]
#[ignore = "runs php on fixtures/*.php; run with --ignored"]
fn php_reproduces_fixtures() {
    let versions = match Command::new("php").args(["-r", "echo 'PHP ', PHP_VERSION, ' / libxml2 ', LIBXML_DOTTED_VERSION;"]).output() {
        Ok(output) => String::from_utf8_lossy(&output.stdout).into_owned(),
        Err(err) if err.kind() == ErrorKind::NotFound => {
            eprintln!("SKIP: no `php` on PATH; install {RECORDED_WITH} to re-verify the parser corpus fixtures");
            return;
        }
        Err(err) => panic!("cannot run php: {err}"),
    };
    for corpus in CORPORA {
        let path = write_script(corpus);
        let output = Command::new("php").arg(&path).output().unwrap_or_else(|err| panic!("cannot run php {}: {err}", path.display()));
        assert!(
            output.status.success(),
            "php {} failed with {}:\n{}",
            path.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        let context = format!("{versions}; fixtures recorded with {RECORDED_WITH}");
        assert_same_output(corpus.name, &context, &output.stdout, corpus.expected);
    }
}

/// The committed scripts are what the tables and this generator produce today.
#[test]
fn committed_scripts_are_current() {
    for corpus in CORPORA {
        let path = script_path(corpus.name);
        let on_disk = std::fs::read(&path).unwrap_or_else(|err| panic!("{}: {err}; run `{REGEN_COMMAND}`", path.display()));
        assert!(on_disk == corpus.script().as_bytes(), "{} is stale; run `{REGEN_COMMAND}`", path.display());
    }
}

/// The literal spelling escapes the bytes PHP would otherwise interpret or mangle.
#[test]
fn php_literal_escapes_special_bytes() {
    assert_eq!(php_literal(b"a\"b\\c$d\n\r\t"), "\"a\\\"b\\\\c\\$d\\n\\r\\t\"");
    assert_eq!(php_literal(b"\x00\x01\x7f\xc3\xa9"), "\"\\x00\\x01\\x7f\\xc3\\xa9\"");
    // A hex escape never swallows a following hex digit: PHP reads at most two.
    assert_eq!(php_literal(b"\xe9a"), "\"\\xe9a\"");
}
