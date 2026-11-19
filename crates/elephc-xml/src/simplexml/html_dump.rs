//! Purpose:
//! The HTML serializer of PHP's libxml2 2.9.13 (`HTMLtree.c`: `htmlNodeDumpFormatOutput`,
//! `htmlAttrDumpOutput`, `htmlDocContentDumpFormatOutput`) over a native libxml2 tree.
//!
//! Called from:
//! - `dom_write.rs` for `saveHTML` (whole document and single node).
//!
//! Key details:
//! - The managed libxml2 is newer than the one PHP links, and its HTML writer differs in byte
//!   output (attribute quoting, valueless attributes, the end tag of an empty `li`, text escaping
//!   of a document that is not an HTML one). The tree is read through the shim accessors only and
//!   the 2.9.13 algorithm is reproduced, with the element table (void, inline, known) taken from
//!   `php -n`.
//! - Output is UTF-8. A whole document goes through the HTML character encoder (`&eacute;`,
//!   `&#26085;`) unless the document declares UTF-8 in a meta element, as `htmlDocDumpMemoryFormat`.

use super::ffi;
use super::ffi::Node;
use std::ffi::{c_char, c_int, c_uint, CStr};

extern "C" {
    #[link_name = "xmlNodeGetContent"]
    fn node_content(node: Node) -> *mut c_char;
    #[link_name = "htmlGetMetaEncoding"]
    fn meta_encoding(doc: Node) -> *const c_char;
    #[link_name = "htmlEntityValueLookup"]
    fn entity_by_value(value: c_uint) -> *const EntityDesc;
}

#[repr(C)]
struct EntityDesc {
    value: c_uint,
    name: *const c_char,
    desc: *const c_char,
}

const ELEMENT: c_int = 1;
const TEXT: c_int = 3;
const CDATA: c_int = 4;
const ENTITY_REF: c_int = 5;
const PI: c_int = 7;
const COMMENT: c_int = 8;
const DOCUMENT: c_int = 9;
const HTML_DOCUMENT: c_int = 13;
const DTD: c_int = 14;

/// Elements `htmlTagLookup` knows (compared case-insensitively).
const KNOWN: &[&str] = &[
    "a", "abbr", "acronym", "address", "applet", "area", "b", "base", "basefont", "bdo", "big", "blockquote", "body", "br",
    "button", "caption", "center", "cite", "code", "col", "colgroup", "dd", "del", "dfn", "dir", "div", "dl", "dt", "em",
    "embed", "fieldset", "font", "form", "frame", "frameset", "h1", "h2", "h3", "h4", "h5", "h6", "head", "hr", "html", "i",
    "iframe", "img", "input", "ins", "isindex", "kbd", "label", "legend", "li", "link", "map", "menu", "meta", "noframes",
    "noscript", "object", "ol", "optgroup", "option", "p", "param", "pre", "q", "s", "samp", "script", "select", "small",
    "span", "strike", "strong", "style", "sub", "sup", "table", "tbody", "td", "textarea", "tfoot", "th", "thead", "title",
    "tr", "tt", "u", "ul", "var",
];
/// Void elements: written as a start tag only, children ignored.
const VOID: &[&str] = &[
    "area", "base", "basefont", "br", "col", "frame", "hr", "img", "input", "isindex", "link", "meta", "param",
];
/// Known elements that are not inline: `formatOutput` breaks lines around them.
const BLOCK: &[&str] = &[
    "address", "area", "base", "blockquote", "body", "caption", "center", "col", "colgroup", "dd", "dir", "div", "dl", "dt",
    "fieldset", "form", "frame", "frameset", "h1", "h2", "h3", "h4", "h5", "h6", "head", "hr", "html", "isindex", "legend",
    "li", "link", "menu", "meta", "noframes", "noscript", "ol", "optgroup", "option", "p", "param", "pre", "style", "table",
    "tbody", "td", "tfoot", "th", "thead", "title", "tr", "ul",
];
/// Attributes written without a value, whatever they hold.
const BOOLEAN: &[&str] = &[
    "checked", "compact", "declare", "defer", "disabled", "ismap", "multiple", "nohref", "noresize", "noshade", "nowrap",
    "readonly", "selected",
];

fn in_list(list: &[&str], name: &[u8]) -> bool {
    list.iter().any(|candidate| name.eq_ignore_ascii_case(candidate.as_bytes()))
}

/// What `htmlTagLookup` returns, reduced to the fields the writer reads.
struct Info {
    empty: bool,
    save_end_tag: bool,
    inline: bool,
}

fn lookup(name: &[u8]) -> Option<Info> {
    if !in_list(KNOWN, name) { return None; }
    Some(Info {
        empty: in_list(VOID, name),
        // Only `li` has saveEndTag set among the elements that are not void.
        save_end_tag: name.eq_ignore_ascii_case(b"li"),
        inline: !in_list(BLOCK, name),
    })
}

unsafe fn c_bytes(raw: *const c_char) -> Vec<u8> {
    if raw.is_null() { Vec::new() } else { CStr::from_ptr(raw).to_bytes().to_vec() }
}

unsafe fn name_of(node: Node) -> Vec<u8> { c_bytes(ffi::name(node)) }

unsafe fn content_of(node: Node) -> Option<Vec<u8>> {
    let raw = node_content(node);
    if raw.is_null() { return None; }
    let value = c_bytes(raw);
    ffi::string_free(raw.cast());
    Some(value)
}

/// `xmlEncodeEntitiesReentrant`: `html` documents keep non-ASCII bytes, the others write `&#xHEX;`.
fn encode_entities(text: &[u8], html: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut index = 0;
    while index < text.len() {
        let byte = text[index];
        match byte {
            b'<' => out.extend_from_slice(b"&lt;"),
            b'>' => out.extend_from_slice(b"&gt;"),
            b'&' => out.extend_from_slice(b"&amp;"),
            b'\n' | b'\t' | 0x20..=0x7f => out.push(byte),
            b'\r' if html => out.push(byte),
            0x80..=0xff if html => out.push(byte),
            0x80..=0xff => {
                let width = match byte { 0xc0..=0xdf => 2, 0xe0..=0xef => 3, 0xf0..=0xf7 => 4, _ => 1 };
                let end = (index + width).min(text.len());
                match std::str::from_utf8(&text[index..end]).ok().and_then(|s| s.chars().next()) {
                    Some(c) => out.extend_from_slice(format!("&#x{:X};", c as u32).as_bytes()),
                    None => out.push(byte),
                }
                index = end;
                continue;
            }
            _ => out.extend_from_slice(format!("&#{};", byte).as_bytes()),
        }
        index += 1;
    }
    out
}

/// `xmlBufWriteQuotedString`.
fn quoted(out: &mut Vec<u8>, value: &[u8]) {
    if value.contains(&b'"') {
        if value.contains(&b'\'') {
            out.push(b'"');
            for byte in value {
                if *byte == b'"' { out.extend_from_slice(b"&quot;"); } else { out.push(*byte); }
            }
            out.push(b'"');
        } else {
            out.push(b'\'');
            out.extend_from_slice(value);
            out.push(b'\'');
        }
    } else {
        out.push(b'"');
        out.extend_from_slice(value);
        out.push(b'"');
    }
}

/// `xmlURIEscapeStr(value, "@/:=?;#%&,+")`.
fn uri_escape(value: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(value.len());
    for byte in value {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()@/:=?;#%&,+".contains(byte) {
            out.push(*byte);
        } else {
            out.extend_from_slice(format!("%{:02X}", byte).as_bytes());
        }
    }
    out
}

struct Dump<'a> {
    format: bool,
    html: bool,
    out: &'a mut Vec<u8>,
}

impl Dump<'_> {
    unsafe fn list(&mut self, first: Node) {
        let mut current = first;
        while !current.is_null() {
            self.node(current);
            current = ffi::next(current);
        }
    }

    unsafe fn prefix_of(node: Node) -> Vec<u8> { c_bytes(ffi::ns(node, 1)) }

    unsafe fn attribute(&mut self, attr: Node) {
        self.out.push(b' ');
        let prefix = Self::prefix_of(attr);
        if !prefix.is_empty() {
            self.out.extend_from_slice(&prefix);
            self.out.push(b':');
        }
        let name = name_of(attr);
        self.out.extend_from_slice(&name);
        let children = ffi::child(attr);
        if children.is_null() || in_list(BOOLEAN, &name) { return; }
        let mut value = Vec::new();
        let mut piece = children;
        while !piece.is_null() {
            match ffi::node_type(piece) {
                ENTITY_REF => {
                    value.push(b'&');
                    value.extend(name_of(piece));
                    value.push(b';');
                }
                _ => value.extend(encode_entities(&content_of(piece).unwrap_or_default(), self.html)),
            }
            piece = ffi::next(piece);
        }
        self.out.push(b'=');
        let parent = ffi::parent(attr);
        let uri = prefix.is_empty()
            && !parent.is_null()
            && ffi::ns(parent, 0).is_null()
            && (name.eq_ignore_ascii_case(b"href")
                || name.eq_ignore_ascii_case(b"action")
                || name.eq_ignore_ascii_case(b"src")
                || (name.eq_ignore_ascii_case(b"name") && name_of(parent).eq_ignore_ascii_case(b"a")));
        if uri {
            let start = value.iter().position(|b| !matches!(b, b' ' | b'\t' | b'\n' | b'\r')).unwrap_or(value.len());
            quoted(self.out, &uri_escape(&value[start..]));
        } else {
            quoted(self.out, &value);
        }
    }

    /// The line break written after a non-inline element that has a following sibling.
    unsafe fn newline_after(&mut self, cur: Node, info: &Option<Info>) {
        let next = ffi::next(cur);
        if !self.format || next.is_null() { return; }
        let Some(info) = info else { return; };
        if info.inline { return; }
        let next_type = ffi::node_type(next);
        let parent = ffi::parent(cur);
        if next_type != TEXT && next_type != ENTITY_REF && !parent.is_null() && !name_of(parent).starts_with(b"p") {
            self.out.push(b'\n');
        }
    }

    unsafe fn node(&mut self, cur: Node) {
        match ffi::node_type(cur) {
            DTD => {}
            DOCUMENT | HTML_DOCUMENT => self.document(cur),
            2 => self.attribute(cur),
            TEXT => {
                let Some(content) = content_of(cur) else { return; };
                let parent = ffi::parent(cur);
                let raw = !parent.is_null() && {
                    let name = name_of(parent);
                    name.eq_ignore_ascii_case(b"script") || name.eq_ignore_ascii_case(b"style")
                };
                if raw {
                    self.out.extend_from_slice(&content);
                } else {
                    let encoded = encode_entities(&content, self.html);
                    self.out.extend_from_slice(&encoded);
                }
            }
            COMMENT => {
                self.out.extend_from_slice(b"<!--");
                self.out.extend(content_of(cur).unwrap_or_default());
                self.out.extend_from_slice(b"-->");
            }
            PI => {
                self.out.extend_from_slice(b"<?");
                self.out.extend(name_of(cur));
                if let Some(content) = content_of(cur) {
                    self.out.push(b' ');
                    self.out.extend(content);
                }
                self.out.push(b'>');
            }
            ENTITY_REF => {
                self.out.push(b'&');
                self.out.extend(name_of(cur));
                self.out.push(b';');
            }
            CDATA => self.out.extend(content_of(cur).unwrap_or_default()),
            ELEMENT => self.element(cur),
            _ => {}
        }
    }

    unsafe fn element(&mut self, cur: Node) {
        let name = name_of(cur);
        let prefix = Self::prefix_of(cur);
        let info = lookup(&name);
        self.out.push(b'<');
        let qualified = |out: &mut Vec<u8>| {
            if !prefix.is_empty() {
                out.extend_from_slice(&prefix);
                out.push(b':');
            }
            out.extend_from_slice(&name);
        };
        qualified(self.out);
        let mut ns = ffi::ns_defs(cur);
        while !ns.is_null() {
            self.out.extend_from_slice(b" xmlns");
            let declared = c_bytes(ffi::ns_value(ns, 1));
            if !declared.is_empty() {
                self.out.push(b':');
                self.out.extend(declared);
            }
            self.out.push(b'=');
            quoted(self.out, &c_bytes(ffi::ns_value(ns, 0)));
            ns = ffi::ns_next(ns);
        }
        let mut attr = ffi::attrs(cur);
        while !attr.is_null() {
            self.attribute(attr);
            attr = ffi::next(attr);
        }
        if info.as_ref().is_some_and(|info| info.empty) {
            self.out.push(b'>');
            self.newline_after(cur, &info);
            return;
        }
        let first = ffi::child(cur);
        if first.is_null() {
            if info.as_ref().is_some_and(|info| info.save_end_tag) && !name.eq_ignore_ascii_case(b"html") && !name.eq_ignore_ascii_case(b"body") {
                self.out.push(b'>');
            } else {
                self.out.extend_from_slice(b"></");
                qualified(self.out);
                self.out.push(b'>');
            }
            self.newline_after(cur, &info);
            return;
        }
        self.out.push(b'>');
        let mut last = first;
        while !ffi::next(last).is_null() { last = ffi::next(last); }
        let block = info.as_ref().is_some_and(|info| !info.inline);
        let breaks = self.format && block && first != last && !name.starts_with(b"p");
        let kind = |node: Node| ffi::node_type(node);
        if breaks && kind(first) != TEXT && kind(first) != ENTITY_REF {
            self.out.push(b'\n');
        }
        self.list(first);
        if breaks && kind(last) != TEXT && kind(last) != ENTITY_REF {
            self.out.push(b'\n');
        }
        self.out.extend_from_slice(b"</");
        qualified(self.out);
        self.out.push(b'>');
        self.newline_after(cur, &info);
    }

    unsafe fn document(&mut self, doc: Node) {
        let mut child = ffi::child(doc);
        while !child.is_null() {
            if ffi::node_type(child) == DTD {
                if let Some(fields) = super::dom_write::doctype_of(child) { self.doctype(&fields); }
                break;
            }
            child = ffi::next(child);
        }
        self.list(ffi::child(doc));
        self.out.push(b'\n');
    }

    /// `htmlDtdDumpOutput` from "name \x1f public \x1f system".
    fn doctype(&mut self, fields: &[u8]) {
        let mut parts = fields.split(|byte| *byte == 0x1f);
        let name = parts.next().unwrap_or_default();
        let public = parts.next().unwrap_or_default();
        let system = parts.next().unwrap_or_default();
        self.out.extend_from_slice(b"<!DOCTYPE ");
        self.out.extend_from_slice(name);
        if !public.is_empty() {
            self.out.extend_from_slice(b" PUBLIC ");
            quoted(self.out, public);
            if !system.is_empty() {
                self.out.push(b' ');
                quoted(self.out, system);
            }
        } else if !system.is_empty() && system != b"about:legacy-compat" {
            self.out.extend_from_slice(b" SYSTEM ");
            quoted(self.out, system);
        }
        self.out.extend_from_slice(b">\n");
    }
}

/// UTF-8 to the `HTML` output encoding: non-ASCII characters become `&name;` or `&#N;`.
fn html_encode(text: &[u8]) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(text) else { return text.to_vec(); };
    let mut out = Vec::with_capacity(text.len());
    for c in text.chars() {
        if (c as u32) < 0x80 {
            out.push(c as u8);
            continue;
        }
        let entity = unsafe { entity_by_value(c as c_uint) };
        if entity.is_null() {
            out.extend_from_slice(format!("&#{};", c as u32).as_bytes());
        } else {
            out.push(b'&');
            out.extend(unsafe { c_bytes((*entity).name) });
            out.push(b';');
        }
    }
    out
}

/// Serializes a whole document (`node` null) or one node of it.
pub(super) fn dump(doc: Node, node: Node, format: bool, html_document: bool) -> Vec<u8> {
    let mut out = Vec::new();
    unsafe {
        let mut writer = Dump { format, html: html_document || node.is_null(), out: &mut out };
        if node.is_null() {
            writer.document(doc);
            let meta = c_bytes(meta_encoding(doc));
            if meta.eq_ignore_ascii_case(b"utf-8") || meta.eq_ignore_ascii_case(b"utf8") { return out; }
            return html_encode(&out);
        }
        writer.node(node);
    }
    out
}
