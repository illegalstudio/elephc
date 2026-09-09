//! Purpose:
//! A php-src-shaped event dispatcher for the parser corpus tests: it applies the
//! handler-selection rules of `ext/xml/compat.c` and `xml.c` (default-handler fallbacks,
//! entity handling, ASCII case folding) and renders each callback exactly as the PHP
//! probe scripts printed it (`var_export` and `json_encode` spellings).
//!
//! Called from:
//! - `crate::parser::tests::probes`.
//!
//! Key details:
//! - `Session::parse` mirrors one `xml_parse()` call: feed, drain, report the status.
//! - Positions printed inside a callback come from the event snapshot, positions printed
//!   after `xml_parse()` from the live cursor, exactly like PHP reads them.

pub(crate) use super::style::Style;
use crate::parser::{error_string, EntityKind, Event, Parser, Step};

/// Which PHP handlers the probe registered.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Handlers {
    pub(crate) start: bool,
    pub(crate) cdata: bool,
    pub(crate) default: bool,
    pub(crate) pi: bool,
    pub(crate) ns: bool,
    pub(crate) notation: bool,
    pub(crate) unparsed: bool,
    pub(crate) extref: bool,
}

impl Handlers {
    /// Parses the probe scripts' handler list (`start,cdata,default,pi,ns`).
    pub(crate) fn from_list(list: &str) -> Self {
        let mut h = Handlers::default();
        for name in list.split(',') {
            match name {
                "start" => h.start = true,
                "cdata" => h.cdata = true,
                "default" => h.default = true,
                "pi" => h.pi = true,
                "ns" => h.ns = true,
                // probe1 lists `end` beside `start`; both are set by xml_set_element_handler.
                "end" | "" => {}
                other => panic!("unknown handler {other}"),
            }
        }
        h
    }
}

/// `var_export()` of a PHP string (a NUL splits the literal into `'' . "\0" . ''` parts).
pub(crate) fn var_export(s: &[u8]) -> Vec<u8> {
    let mut raw = Vec::new();
    for (i, part) in s.split(|&b| b == 0).enumerate() {
        if i > 0 {
            raw.extend_from_slice(b" . \"\\0\" . ");
        }
        raw.push(b'\'');
        for &b in part {
            match b {
                b'\\' => raw.extend_from_slice(b"\\\\"),
                b'\'' => raw.extend_from_slice(b"\\'"),
                _ => raw.push(b),
            }
        }
        raw.push(b'\'');
    }
    raw
}

/// Concatenates byte and string pieces into one line body.
pub(crate) fn cat(parts: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    for part in parts {
        out.extend_from_slice(part);
    }
    out
}

/// `json_encode()` of a PHP string, `None` when the bytes are not valid UTF-8.
pub(crate) fn json_string(s: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(s).ok()?;
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '/' => out.push_str("\\/"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c if (c as u32) < 0x80 => out.push(c),
            c => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{:04x}", unit));
                }
            }
        }
    }
    out.push('"');
    Some(out)
}

/// `json_encode()` of a PHP associative array built with `zend_symtable_update`.
pub(crate) fn json_assoc(pairs: &[(Vec<u8>, Vec<u8>)]) -> String {
    if pairs.is_empty() {
        return "[]".to_string();
    }
    let mut keys: Vec<Vec<u8>> = Vec::new();
    let mut values: Vec<Vec<u8>> = Vec::new();
    for (k, v) in pairs {
        if let Some(i) = keys.iter().position(|key| key == k) {
            values[i] = v.clone();
        } else {
            keys.push(k.clone());
            values.push(v.clone());
        }
    }
    let mut out = String::from("{");
    for (i, (k, v)) in keys.iter().zip(values.iter()).enumerate() {
        if i > 0 {
            out.push(',');
        }
        let (Some(k), Some(v)) = (json_string(k), json_string(v)) else {
            return String::new();
        };
        out.push_str(&k);
        out.push(':');
        out.push_str(&v);
    }
    out.push('}');
    out
}

/// `json_encode()` of a PHP list whose entries are strings or `false`.
pub(crate) fn json_list(items: &[Option<&[u8]>]) -> String {
    let mut out = String::from("[");
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        match item {
            None => out.push_str("false"),
            Some(s) => match json_string(s) {
                Some(j) => out.push_str(&j),
                None => return String::new(),
            },
        }
    }
    out.push(']');
    out
}

/// One PHP `XMLParser` with the probe's handlers.
pub(crate) struct Session {
    pub(crate) parser: Parser,
    pub(crate) handlers: Handlers,
    pub(crate) style: Style,
    pub(crate) case_folding: bool,
    pub(crate) out: Vec<u8>,
    /// A pending extref-handler answer of `false` stops the parser (probe2).
    pub(crate) extref_returns_false: bool,
}

impl Session {
    /// Creates a parser like `xml_parser_create()` / `xml_parser_create_ns()`.
    pub(crate) fn new(ns: bool, handlers: Handlers, style: Style) -> Self {
        Session {
            parser: Parser::new(if ns { Some(b":".to_vec()) } else { None }),
            handlers,
            style,
            case_folding: true,
            out: Vec::new(),
            extref_returns_false: false,
        }
    }

    /// Creates a namespace parser with a custom separator.
    pub(crate) fn with_separator(sep: &[u8], handlers: Handlers, style: Style) -> Self {
        let mut s = Session::new(false, handlers, style);
        s.parser = Parser::new(Some(sep.to_vec()));
        s
    }

    /// ASCII case folding as `zend_str_toupper` applies it.
    fn fold(&self, name: &[u8]) -> Vec<u8> {
        if self.case_folding {
            name.to_ascii_uppercase()
        } else {
            name.to_vec()
        }
    }

    /// `@line:col/byte` of the parser's current reported position.
    pub(crate) fn pos(&self) -> String {
        format!("@{}:{}/{}", self.parser.line(), self.parser.column(), self.parser.byte_index())
    }

    /// The position suffix for an event line, when the style prints one.
    fn event_pos(&self, enabled: bool) -> String {
        if enabled {
            format!(" {}", self.pos())
        } else {
            String::new()
        }
    }

    /// One `xml_parse($p, $data, $is_final)` call, returning PHP's 1/0 status.
    pub(crate) fn parse(&mut self, data: &[u8], is_final: bool) -> i32 {
        self.parser.feed(data, is_final);
        loop {
            match self.parser.next() {
                Step::Event(event) => self.dispatch(event),
                Step::NeedMoreData | Step::Finished | Step::Failed => break,
            }
        }
        if self.parser.is_well_formed() {
            1
        } else {
            0
        }
    }

    /// Appends one output line with the style's indentation.
    fn line(&mut self, text: Vec<u8>) {
        if !self.style.probe1 {
            self.out.extend_from_slice(b"  ");
        }
        self.out.extend_from_slice(&text);
        self.out.push(b'\n');
    }

    /// Applies the compat.c / xml.c handler rules to one event.
    fn dispatch(&mut self, event: Event) {
        let (s_tag, e_tag, c_tag, d_tag, ns_tag) = if self.style.probe1 {
            ("START", "END", "CDATA", "DEFAULT", "NSSTART")
        } else {
            ("S", "E", "C", "D", "NS")
        };
        match event {
            Event::StartElement { name, attributes, namespaces, raw } => {
                if self.handlers.ns {
                    for ns in &namespaces {
                        let prefix = ns.prefix.as_deref().map_or(b"false".to_vec(), var_export);
                        let suffix = self.event_pos(self.style.ns_pos);
                        self.line(cat(&[ns_tag.as_bytes(), b" ", &prefix, b" ", &var_export(&ns.uri), suffix.as_bytes()]));
                    }
                }
                if self.handlers.start {
                    let pairs: Vec<(Vec<u8>, Vec<u8>)> = attributes
                        .iter()
                        .map(|a| (self.fold(&a.name), a.value.clone()))
                        .collect();
                    let folded = self.fold(&name);
                    let suffix = self.event_pos(self.style.event_pos);
                    let name_text = if self.style.probe1 { var_export(&folded) } else { folded };
                    self.line(cat(&[s_tag.as_bytes(), b" ", &name_text, b" ", json_assoc(&pairs).as_bytes(), suffix.as_bytes()]));
                } else if self.handlers.default {
                    let suffix = self.event_pos(self.style.event_pos && !self.style.probe1);
                    self.line(cat(&[d_tag.as_bytes(), b" ", &var_export(&raw), suffix.as_bytes()]));
                }
            }
            Event::EndElement { name, raw } => {
                if self.handlers.start {
                    let folded = self.fold(&name);
                    let suffix = self.event_pos(self.style.event_pos);
                    let name_text = if self.style.probe1 { var_export(&folded) } else { folded };
                    self.line(cat(&[e_tag.as_bytes(), b" ", &name_text, suffix.as_bytes()]));
                } else if self.handlers.default {
                    let suffix = self.event_pos(self.style.event_pos && !self.style.probe1);
                    self.line(cat(&[d_tag.as_bytes(), b" ", &var_export(&raw), suffix.as_bytes()]));
                }
            }
            Event::Characters(text) => {
                if self.handlers.cdata {
                    let suffix = self.event_pos(self.style.event_pos);
                    self.line(cat(&[c_tag.as_bytes(), b" ", &var_export(&text), suffix.as_bytes()]));
                } else if self.handlers.default {
                    let suffix = self.event_pos(self.style.event_pos && !self.style.probe1);
                    self.line(cat(&[d_tag.as_bytes(), b" ", &var_export(&text), suffix.as_bytes()]));
                }
            }
            Event::ProcessingInstruction { target, data } => {
                if self.handlers.pi {
                    let data_text = data.as_deref().map_or(b"false".to_vec(), var_export);
                    let suffix = self.event_pos(self.style.pi_pos);
                    self.line(cat(&[b"PI ", &var_export(&target), b" ", &data_text, suffix.as_bytes()]));
                } else if self.handlers.default {
                    let mut full = b"<?".to_vec();
                    full.extend_from_slice(&target);
                    full.push(b' ');
                    match &data {
                        Some(d) => full.extend_from_slice(d),
                        None => full.extend_from_slice(b"(null)"),
                    }
                    full.extend_from_slice(b"?>");
                    let suffix = self.event_pos(self.style.event_pos && !self.style.probe1);
                    self.line(cat(&[d_tag.as_bytes(), b" ", &var_export(&full), suffix.as_bytes()]));
                }
            }
            Event::Comment(text) => {
                if self.handlers.default {
                    let mut full = b"<!--".to_vec();
                    full.extend_from_slice(&text);
                    full.extend_from_slice(b"-->");
                    let suffix = self.event_pos(self.style.event_pos && !self.style.probe1);
                    self.line(cat(&[d_tag.as_bytes(), b" ", &var_export(&full), suffix.as_bytes()]));
                }
            }
            Event::EntityRef { name, kind } => match kind {
                EntityKind::External { system_id, public_id } => {
                    if !self.handlers.extref {
                        return;
                    }
                    let suffix = self.event_pos(self.style.extref_pos);
                    let list = json_list(&[Some(&name), Some(b""), Some(&system_id), public_id.as_deref()]);
                    self.line(cat(&[b"EXTREF ", list.as_bytes(), suffix.as_bytes()]));
                    if self.extref_returns_false {
                        self.parser.stop(crate::parser::XML_ERROR_EXTERNAL_ENTITY_HANDLING);
                    }
                }
                EntityKind::Unparsed => {}
                other => {
                    let predefined = matches!(other, EntityKind::Predefined { .. });
                    let content: Option<Vec<u8>> = match other {
                        EntityKind::Predefined { expansion } => Some(expansion),
                        EntityKind::Internal { replacement } => Some(replacement),
                        _ => None,
                    };
                    if self.handlers.default && !(predefined && self.handlers.cdata) {
                        let mut full = b"&".to_vec();
                        full.extend_from_slice(&name);
                        full.push(b';');
                        let suffix = self.event_pos(self.style.event_pos && !self.style.probe1);
                        self.line(cat(&[d_tag.as_bytes(), b" ", &var_export(&full), suffix.as_bytes()]));
                    } else if self.handlers.cdata {
                        if let Some(content) = content {
                            let suffix = self.event_pos(self.style.event_pos);
                            self.line(cat(&[c_tag.as_bytes(), b" ", &var_export(&content), suffix.as_bytes()]));
                        }
                    }
                }
            },
            Event::NotationDecl { name, public_id, system_id } => {
                if !self.handlers.notation {
                    return;
                }
                let list = json_list(&[Some(&name), None, system_id.as_deref(), public_id.as_deref()]);
                let suffix = self.event_pos(self.style.event_pos && !self.style.probe1);
                self.line(cat(&[b"NOTATION ", list.as_bytes(), suffix.as_bytes()]));
            }
            Event::UnparsedEntityDecl { name, public_id, system_id, notation } => {
                if !self.handlers.unparsed {
                    return;
                }
                let list = json_list(&[Some(&name), None, Some(&system_id), public_id.as_deref(), Some(&notation)]);
                let suffix = self.event_pos(self.style.unparsed_pos);
                self.line(cat(&[b"UNPARSED ", list.as_bytes(), suffix.as_bytes()]));
            }
        }
    }

    /// probe1's `RESULT` line plus separator.
    pub(crate) fn result_line(&mut self, status: i32) {
        let code = self.parser.error_code();
        let text = cat(&[
            format!("RESULT {status} code={code} msg=").as_bytes(),
            &var_export(error_string(code).as_bytes()),
            b" ",
            self.pos().as_bytes(),
            b"\n-----\n",
        ]);
        self.out.extend_from_slice(&text);
    }
}

/// The `run()` summary line of probes 4-9.
pub(crate) fn summary_line(doc: &[u8], handlers: Option<&str>, ns_marker: bool, status: i32, code: i32, pos: Option<String>) -> Vec<u8> {
    let mut line = var_export(doc);
    if let Some(handlers) = handlers {
        let ns_text = if ns_marker { ",NS" } else { "" };
        line.extend_from_slice(format!(" [{handlers}{ns_text}]").as_bytes());
    }
    line.extend_from_slice(format!(" => {status} code={code}").as_bytes());
    if let Some(pos) = pos {
        line.push(b' ');
        line.extend_from_slice(pos.as_bytes());
    }
    line.push(b'\n');
    line
}

/// Panics with the first differing line when `actual` and `expected` differ.
pub(crate) fn assert_corpus(name: &str, actual: &[u8], expected: &[u8]) {
    if actual == expected {
        return;
    }
    let show = |lines: &[&[u8]]| -> String {
        lines.iter().map(|l| String::from_utf8_lossy(l).into_owned()).collect::<Vec<_>>().join("\n")
    };
    let a: Vec<&[u8]> = actual.split(|&b| b == b'\n').collect();
    let e: Vec<&[u8]> = expected.split(|&b| b == b'\n').collect();
    for (i, (la, le)) in a.iter().zip(e.iter()).enumerate() {
        if la != le {
            let from = i.saturating_sub(3);
            panic!(
                "{name}: line {} differs\n--- actual ---\n{}\n--- expected ---\n{}\n",
                i + 1,
                show(&a[from..(i + 3).min(a.len())]),
                show(&e[from..(i + 3).min(e.len())])
            );
        }
    }
    panic!("{name}: line counts differ ({} vs {} lines)\n--- actual tail ---\n{}", a.len(), e.len(), show(&a[a.len().saturating_sub(5)..]));
}
