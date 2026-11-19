//! Purpose:
//! DOM serializers over the shared libxml2 tree: `saveXML`, `saveHTML` and `C14N`.
//!
//! Called from:
//! - The DOM prelude's `DOMDocument::saveXML` / `saveHTML` and `DOMNode::C14N`, through
//!   `elephc_xml_sx_dom_write` (the `SimpleXMLElement::__elephc_dom_write` bridge method).
//!
//! Key details:
//! - Only libxml2's public API is used, with the calls PHP makes: `xmlSaveToIO` for a whole XML
//!   document (so `LIBXML_NOEMPTYTAG`, `LIBXML_NOXMLDECL` and `formatOutput` are the library's own
//!   save options), `xmlSaveTree` (UTF-8) for one XML node, `htmlDocDumpMemoryFormat` /
//!   `htmlNodeDumpFormatOutput` for one HTML node and `xmlC14NExecute` for canonical XML.
//! - The document's encoding is never read here: libxml2 takes it from the document itself.
//! - Bytes are returned hex-encoded (like `elephc_xml_sx_xml_hex`) so legacy encodings and
//!   embedded NULs survive the NUL-terminated string boundary.

use super::*;
use super::html_dump;
use std::ffi::{c_int, c_long, c_void};

type Writer = extern "C" fn(*mut c_void, *const c_char, c_int) -> c_int;
type Closer = extern "C" fn(*mut c_void) -> c_int;
type Visible = extern "C" fn(*mut c_void, Node, Node) -> c_int;

extern "C" {
    #[link_name = "xmlSaveToIO"]
    fn save_to_io(write: Option<Writer>, close: Option<Closer>, ctx: *mut c_void, encoding: *const c_char, options: c_int) -> *mut c_void;
    #[link_name = "xmlSaveDoc"]
    fn save_doc(ctx: *mut c_void, doc: Node) -> c_long;
    #[link_name = "xmlSaveClose"]
    fn save_close(ctx: *mut c_void) -> c_int;
    #[link_name = "xmlCopyDtd"]
    fn copy_dtd(dtd: Node) -> Node;
    #[link_name = "xmlFreeDtd"]
    fn free_dtd(dtd: Node);
    #[link_name = "xmlSaveTree"]
    fn save_tree(ctx: *mut c_void, node: Node) -> c_long;
    #[link_name = "xmlOutputBufferCreateIO"]
    fn output_buffer_create_io(write: Option<Writer>, close: Option<Closer>, ctx: *mut c_void, encoder: *mut c_void) -> *mut c_void;
    #[link_name = "xmlOutputBufferClose"]
    fn output_buffer_close(buf: *mut c_void) -> c_int;
    #[link_name = "xmlC14NExecute"]
    fn c14n_execute(doc: Node, visible: Option<Visible>, user: *mut c_void, mode: c_int, prefixes: *mut *mut u8, comments: c_int, buf: *mut c_void) -> c_int;
}

/// Appends what libxml2 writes to the `Vec<u8>` behind `ctx`.
extern "C" fn collect(ctx: *mut c_void, data: *const c_char, length: c_int) -> c_int {
    if length > 0 && !data.is_null() {
        unsafe { (*ctx.cast::<Vec<u8>>()).extend_from_slice(std::slice::from_raw_parts(data.cast(), length as usize)); }
    }
    length
}

/// Canonical XML visibility of a subtree: the node, its descendants, their attributes and the
/// namespace declarations of those elements are visible (`parent` is the element for a namespace).
extern "C" fn in_subtree(root: *mut c_void, node: Node, parent: Node) -> c_int {
    let mut walk = if unsafe { ffi::node_type(node) } == 18 { parent } else { node };
    while !walk.is_null() {
        if walk == root { return 1; }
        walk = unsafe { ffi::parent(walk) };
    }
    0
}

extern "C" fn everything(_root: *mut c_void, _node: Node, _parent: Node) -> c_int { 1 }

/// Serializes the document (`document`) or the node a wrapper selects. Operations: 0 XML,
/// 3 XML of the document node as a tree (UTF-8, like a node), 1 HTML, 2 canonical XML (`options` bit 0 exclusive, bit 1 with comments, `prefixes` the
/// comma-separated inclusive namespace prefixes of the exclusive form). `format` is
/// `formatOutput`; for XML `options` also carries `LIBXML_NOXMLDECL` (2) and `LIBXML_NOEMPTYTAG`
/// (4).
fn write(view: &View, operation: i64, document: bool, options: i64, format: bool, prefixes: &[u8]) -> Option<Vec<u8>> {
    let doc = view.doc.raw;
    let node = if document { std::ptr::null_mut() } else { view.first()? };
    let mut out: Vec<u8> = Vec::new();
    let sink = (&mut out as *mut Vec<u8>).cast::<c_void>();
    unsafe {
        match operation {
            0 if document => {
                let flags = c_int::from(format) | (options as c_int & 6);
                let ctx = save_to_io(Some(collect), None, sink, std::ptr::null(), flags);
                if ctx.is_null() { return None; }
                let written = save_doc(ctx, doc);
                save_close(ctx);
                if written < 0 { return None; }
            }
            // One node (or, for operation 3, the document node as a tree): UTF-8 output with the
            // text left unescaped, as PHP's `saveXML($node)` writes it.
            0 | 3 => {
                let target = if operation == 3 { doc } else { node };
                let flags = c_int::from(format) | (options as c_int & 4);
                let ctx = save_to_io(Some(collect), None, sink, b"UTF-8\0".as_ptr().cast(), flags);
                if ctx.is_null() { return None; }
                let written = save_tree(ctx, target);
                save_close(ctx);
                if written < 0 { return None; }
            }
            1 if document => out = html_dump::dump(doc, std::ptr::null_mut(), format, options & 8 == 0),
            // One node: UTF-8 output; PHP writes the text of a document that is not an HTML one
            // (`createElement`) with character references (`options` bit 3).
            1 => out = html_dump::dump(doc, node, format, options & 8 == 0),
            // The DOCTYPE node of a document: "name \x1f publicId \x1f systemId [\x1f internalSubset]"
            // read back from libxml2's own serialization, which has no opaque accessor.
            4 if !document => return doctype_of(node),
            2 => {
                let buf = output_buffer_create_io(Some(collect), None, sink, std::ptr::null_mut());
                if buf.is_null() { return None; }
                let names: Vec<_> = prefixes.split(|byte| *byte == b',').filter(|name| !name.is_empty()).map(cstring).collect();
                let mut list: Vec<*mut u8> = names.iter().map(|name| name.as_ptr() as *mut u8).collect();
                list.push(std::ptr::null_mut());
                let list_pointer = if names.is_empty() { std::ptr::null_mut() } else { list.as_mut_ptr() };
                let (visible, user): (Visible, *mut c_void) = if document { (everything, std::ptr::null_mut()) } else { (in_subtree, node) };
                let status = c14n_execute(doc, Some(visible), user, c_int::from(options & 1 != 0), list_pointer, c_int::from(options & 2 != 0), buf);
                output_buffer_close(buf);
                if status < 0 { return None; }
            }
            _ => return None,
        }
    }
    Some(out)
}

/// The fields of a DTD node, "name \x1f publicId \x1f systemId [\x1f internalSubset]".
pub(super) fn doctype_of(node: Node) -> Option<Vec<u8>> {
    let mut out: Vec<u8> = Vec::new();
    let sink = (&mut out as *mut Vec<u8>).cast::<c_void>();
    unsafe {
        // Copy the DTD out of its document first: an HTML document would serialize it
        // through the HTML writer, which has no DTD form for a bare node.
        let copy = copy_dtd(node);
        if copy.is_null() { return None; }
        let ctx = save_to_io(Some(collect), None, sink, b"UTF-8\0".as_ptr().cast(), 0);
        if ctx.is_null() { free_dtd(copy); return None; }
        let written = save_tree(ctx, copy);
        save_close(ctx);
        free_dtd(copy);
        if written < 0 { return None; }
    }
    Some(doctype_fields(&out))
}

/// Splits `<!DOCTYPE name [PUBLIC "p" "s" | SYSTEM "s"] [ [subset] ]>` into its fields.
fn doctype_fields(dump: &[u8]) -> Vec<u8> {
    let text = dump.strip_prefix(b"<!DOCTYPE ").unwrap_or(dump);
    let name_end = text.iter().position(|b| matches!(b, b' ' | b'>' | b'[' | b'\n')).unwrap_or(text.len());
    let (name, mut rest) = text.split_at(name_end);
    let quoted = |rest: &mut &[u8]| -> Vec<u8> {
        while rest.first() == Some(&b' ') { *rest = &rest[1..]; }
        let Some(&quote) = rest.first().filter(|q| matches!(q, b'"' | b'\'')) else { return Vec::new(); };
        let body = &rest[1..];
        let end = body.iter().position(|b| *b == quote).unwrap_or(body.len());
        *rest = &body[(end + 1).min(body.len())..];
        body[..end].to_vec()
    };
    while rest.first() == Some(&b' ') { rest = &rest[1..]; }
    let (mut public, mut system) = (Vec::new(), Vec::new());
    if rest.starts_with(b"PUBLIC") {
        rest = &rest[6..];
        public = quoted(&mut rest);
        system = quoted(&mut rest);
    } else if rest.starts_with(b"SYSTEM") {
        rest = &rest[6..];
        system = quoted(&mut rest);
    }
    let subset = rest.iter().position(|b| *b == b'[').map(|start| {
        let body = &rest[start + 1..];
        let body = body.strip_prefix(b"\n").unwrap_or(body);
        let end = body.iter().rposition(|b| *b == b']').unwrap_or(body.len());
        let body = &body[..end];
        body.strip_suffix(b"\n").unwrap_or(body).to_vec()
    });
    let mut out = name.to_vec();
    for field in [public, system] { out.push(0x1f); out.extend(field); }
    if let Some(subset) = subset { out.push(0x1f); out.extend(subset); }
    out
}

/// Serializes for the DOM prelude. `target` 0 selects the document of the wrapper, 1 the node it
/// wraps; see [`write`] for the operations. Returns the bytes hex-encoded, "" on failure.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_sx_dom_write(handle: i64, operation: i64, target: i64, options: i64, format: i64, prefixes: *const c_char) -> *const c_char {
    let prefixes = bytes(prefixes);
    super::abi::guard(|| super::abi::output(super::abi::view(handle).and_then(|view| {
        write(&view, operation, target == 0, options, format != 0, &prefixes).map(|raw| super::abi::encode_hex(&raw))
    }).unwrap_or_default()))
}
