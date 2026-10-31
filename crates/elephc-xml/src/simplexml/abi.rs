//! Purpose:
//! Panic-contained, thread-local handle ABI for SimpleXML wrappers.
//!
//! Called from:
//! - `simplexml_prelude` extern declarations on every supported target.
//!
//! Key details:
//! - Returned strings stay valid until the next string call on this thread.
//! - Missing handles fail closed; freeing a handle is idempotent.

use super::*;
use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};

#[derive(Default)]
struct Registry { next: i64, views: HashMap<i64, View>, cursors: HashMap<i64, iteration::Cursor> }
thread_local! {
    static REGISTRY: RefCell<Registry> = RefCell::new(Registry::default());
    static OUTPUT: RefCell<CString> = RefCell::new(CString::default());
}
/// Runs an ABI operation without allowing Rust panics to unwind into generated code.
fn guard<T: Default>(body: impl FnOnce() -> T) -> T { catch_unwind(AssertUnwindSafe(body)).unwrap_or_default() }
/// Clones a wrapper so registry borrows never span native operations or new handles.
fn view(handle: i64) -> Option<View> { REGISTRY.with(|r| r.borrow().views.get(&handle).cloned()) }
/// Stores one independent PHP wrapper and allocates a nonzero monotonically increasing handle.
fn insert(value: View) -> i64 {
    REGISTRY.with(|registry| {
        let mut r = registry.borrow_mut();
        let Some(next) = r.next.checked_add(1) else { return 0; };
        r.next = next; r.views.insert(next, value); next
    })
}
/// Stashes an owned string until the next string-returning call on this thread.
fn output(value: impl AsRef<[u8]>) -> *const c_char {
    OUTPUT.with(|cell| { let mut out = cell.borrow_mut(); *out = cstring(value.as_ref()); out.as_ptr() })
}
/// Encodes arbitrary bytes for the NUL-terminated string boundary without changing their contents.
fn encode_hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len().saturating_mul(2));
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    for byte in bytes { hex.push(DIGITS[(byte >> 4) as usize] as char); hex.push(DIGITS[(byte & 15) as usize] as char); }
    hex
}
/// Parses the full byte length, including embedded NULs, into a document-owned root.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_sx_load(data: *const c_char, length: i64, options: i64, ns: *const c_char, prefix: i64, url: *const c_char) -> i64 {
    guard(|| {
        diagnostics::reset();
        if data.is_null() || length < 0 || length > i32::MAX as i64 { return 0; }
        parse_with_url(std::slice::from_raw_parts(data.cast(), length as usize), options,
            &bytes(ns), prefix != 0, &bytes(url)).map(insert).unwrap_or(0)
    })
}
/// Parses DOM loadXML bytes while retaining its single structured warning or notice.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_sx_load_dom(data: *const c_char, length: i64, options: i64) -> i64 {
    guard(|| {
        diagnostics::reset();
        if data.is_null() || length < 0 || length > i32::MAX as i64 { return 0; }
        parse_with_diagnostics(std::slice::from_raw_parts(data.cast(), length as usize), options,
            "", false, "", 2).map(insert).unwrap_or(0)
    })
}
/// Creates an internal document lease before any HTML element has been appended.
#[no_mangle]
pub extern "C" fn elephc_xml_sx_html_document() -> i64 {
    guard(|| {
        crate::ffi::init_library();
        let raw = unsafe { ffi::html_document() };
        if raw.is_null() { return 0; }
        let doc = Document::new(raw);
        insert(View::new(doc, Scope::Node(std::ptr::null_mut()), std::ptr::null_mut(), Namespace::default(),
            None, Rc::new(RefCell::new(BTreeMap::new()))))
    })
}
/// Retains each appended literal HTML node independently of its temporary parent handle.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_sx_html_append(handle: i64, kind: i64, name: *const c_char, value: *const c_char) -> i64 {
    guard(|| {
        let Some(parent) = view(handle) else { return 0; };
        let Ok(kind) = i32::try_from(kind) else { return 0; };
        let node = ffi::html_append(parent.doc.raw, parent.first().unwrap_or(std::ptr::null_mut()), kind, name, value);
        if node.is_null() { return 0; }
        insert(parent.with_scope(Scope::Node(node)))
    })
}
/// Publishes an imported root while keeping rootless builder leases internal.
#[no_mangle]
pub extern "C" fn elephc_xml_sx_html_root(handle: i64) -> i64 {
    guard(|| {
        let Some(document) = view(handle) else { return 0; };
        let root = unsafe { ffi::root(document.doc.raw) };
        if root.is_null() { return 0; }
        insert(document.with_scope(Scope::Node(root)))
    })
}
/// Opens a file or URL through libxml so read failures retain the SimpleXML caller's warnings.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_sx_load_file(filename: *const c_char, options: i64, ns: *const c_char, prefix: i64) -> i64 {
    guard(|| parse_file(filename, options, &bytes(ns), prefix != 0).map(insert).unwrap_or(0))
}
/// Adopts PHP stream bytes without losing the original URL or native I/O diagnostics.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_sx_load_stream(filename: *const c_char, data: *const c_char,
    length: i64, options: i64, ns: *const c_char, prefix: i64) -> i64 {
    guard(|| {
        diagnostics::reset();
        crate::ffi::init_library();
        if filename.is_null() || data.is_null() || length < -1 || length > i32::MAX as i64 { return 0; }
        let Ok(options) = i32::try_from(options) else { return 0; };
        let mut messages = std::ptr::null_mut();
        let raw = ffi::parse_stream(filename, data, length as i32, options, &mut messages);
        diagnostics::capture(messages);
        from_document(raw, &bytes(ns), prefix != 0).map(insert).unwrap_or(0)
    })
}
/// Captures errno before allocating or invoking the PHP-labelled write-failure dispatcher.
#[no_mangle]
pub extern "C" fn elephc_xml_sx_file_error() -> *const c_char {
    let code = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
    guard(|| output(if code == 0 { b"operation failed".to_vec() } else { unsafe { bytes(ffi::strerror(code)) } }))
}
/// Consumes formatted diagnostics before user warning handlers can run nested native operations.
#[no_mangle]
pub extern "C" fn elephc_xml_sx_diagnostics() -> *const c_char { guard(|| output(diagnostics::take())) }
/// Releases one wrapper; the document dies only after its final selection is released.
#[no_mangle]
pub extern "C" fn elephc_xml_sx_free(handle: i64) {
    guard(|| REGISTRY.with(|r| {
        let mut registry = r.borrow_mut();
        registry.cursors.remove(&handle);
        registry.views.remove(&handle);
    }));
}
/// Returns a fresh handle over an independent document and any detached aliased nodes.
#[no_mangle]
pub extern "C" fn elephc_xml_sx_clone(handle: i64) -> i64 { guard(|| view(handle).and_then(|v| v.deep_clone()).map(insert).unwrap_or(0)) }
/// Selects children (0), attributes (1), a property (2), an attribute (3), or offset (4).
/// Internal modes 7/8 preserve filters; 9/10 project children or a sibling tail, 11 reads document children.
/// DOM modes 12/13/14 select parent/previous/next; 15 includes attributes from every namespace.
/// Import mode 16 selects the first node with no namespace or XPath registration filter.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_sx_select(handle: i64, kind: i64, name: *const c_char, ns: *const c_char, prefix: i64, index: i64) -> i64 {
    guard(|| view(handle).map(|v| {
        if kind == 4 && index < 0 { return insert(v.with_scope(Scope::Nodes(Rc::new(vec![])))); }
        let mut selected = v.select(kind, &bytes(name), &bytes(ns), prefix != 0, index.max(0) as usize);
        if kind == 3 || (kind == 2 && matches!(v.scope, Scope::Attributes(..))) {
            if let Some(node) = selected.first() { selected.scope = Scope::Node(node); selected.identity = node; selected.refresh_pins(); }
        }
        insert(selected)
    }).unwrap_or(0))
}
/// Reads count (0), presence (1), iteration count (2), truthiness (3), or node kind (4).
/// DOM modes 12/13/14 read stable node identity, HTML document kind, and raw-text policy.
#[no_mangle]
pub extern "C" fn elephc_xml_sx_int(handle: i64, operation: i64) -> i64 {
    guard(|| view(handle).map(|v| match operation {
        0 | 2 => v.item_count() as i64,
        1 => i64::from(v.first().is_some()),
        3 => if matches!(v.scope, Scope::Node(_) | Scope::Index(..)) {
            i64::from(v.first().is_some_and(|node| node_is_truthy(&v, node)))
        } else if let Scope::Children(owner, None) = &v.scope {
            i64::from(v.first().is_some() || owner.first().is_some_and(|node| has_selected_attributes(&v, node)))
        } else { i64::from(v.first().is_some()) },
        4 => v.first().map(|n| unsafe { ffi::node_type(n) as i64 }).unwrap_or(0),
        5 => match v.scope { Scope::Children(_, None) => 1, Scope::Children(_, Some(_)) => 2,
            Scope::Attributes(_, None) => 3, Scope::Attributes(_, Some(_)) | Scope::AttributeMiss(..) => 4, _ => 0 },
        6 => v.first().map(|n| i64::from(projection_is_string(n))).unwrap_or(0),
        7 => v.first().map(|n| unsafe { i64::from(ffi::empty(n) != 0) }).unwrap_or(1),
        8 => v.first().map(|node| unsafe {
            let mut child = ffi::child(node);
            while !child.is_null() && ffi::node_type(child) != 1 { child = ffi::next(child); }
            i64::from(!child.is_null())
        }).unwrap_or(0),
        9 => i64::from(v.first().is_some_and(|node| node == unsafe { ffi::root(v.doc.raw) })),
        10 => v.first().map(|n| i64::from(projection_uses_items(n))).unwrap_or(0),
        11 => v.first().map(|n| unsafe {
            i64::from(projection_is_string(n) && ffi::next(ffi::child(n)).is_null())
        }).unwrap_or(0),
        12 => v.first().map(|node| node as usize as i64).unwrap_or(0),
        13 => unsafe { i64::from(ffi::node_type(v.doc.raw) == 13) },
        14 => v.first().map(|node| unsafe { i64::from(ffi::node_type(node) == 4) }).unwrap_or(0),
        _ => 0,
    }).unwrap_or(0))
}
/// Tests native property presence without treating CDATA or comment content as element text.
fn node_is_truthy(view: &View, node: Node) -> bool {
    unsafe {
        if ffi::node_type(node) == 2 { return true; }
        if has_selected_attributes(view, node) { return true; }
        let first = ffi::child(node);
        let mut child = first;
        while !child.is_null() {
            let kind = ffi::node_type(child);
            if kind == 3 && child == first && ffi::next(child).is_null() {
                return !owned_bytes(ffi::full_text(child)).is_empty();
            }
            if !matches!(kind, 3 | 4) && (kind != 1 || view.ns.matches(child))
                && !ffi::name(child).is_null() { return true; }
            if kind == 17 { break; }
            child = ffi::next(child);
        }
        false
    }
}
/// Tests parent attributes using the selection's namespace, including attribute-only children views.
fn has_selected_attributes(view: &View, node: Node) -> bool {
    unsafe {
        if ffi::node_type(node) != 1 { return false; }
        let mut attr = ffi::attrs(node);
        while !attr.is_null() {
            if view.ns.matches(attr) { return true; }
            attr = ffi::next(attr);
        }
        false
    }
}
/// PHP projects an element as a string only when its first child is nonblank text.
fn projection_is_string(node: Node) -> bool {
    unsafe {
        let child = ffi::child(node);
        !child.is_null() && ffi::node_type(child) == 3 && ffi::is_blank(child) == 0
    }
}
/// PHP projects a named list as items only for a first node with one childless child.
fn projection_uses_items(node: Node) -> bool {
    unsafe {
        let child = ffi::child(node);
        let parent = ffi::parent(node);
        if child.is_null() || parent.is_null() || ffi::next(node).is_null() { return false; }
        let first = ffi::child(parent);
        ffi::next(child).is_null() && ffi::child(child).is_null()
            && !first.is_null() && !ffi::next(first).is_null()
    }
}
/// Copies text/name/XML (0-6), or all document text (7) for the DOM document property.
#[no_mangle]
pub extern "C" fn elephc_xml_sx_string(handle: i64, operation: i64) -> *const c_char {
    guard(|| output(view(handle).and_then(|v| {
        if operation == 7 { return Some(unsafe { owned_bytes(ffi::full_text(v.doc.raw)) }); }
        let node = if operation == 5 { (!v.identity.is_null()).then_some(v.identity) } else { v.first() };
        node.map(|n| unsafe { match operation {
        0 => owned_bytes(ffi::text(n)), 1 => bytes(ffi::name(n)),
        2 => owned_bytes(ffi::dump(v.doc.raw, n)),
        3 => owned_bytes(ffi::full_text(n)), 4 => owned_bytes(ffi::dump_node(v.doc.raw, n)),
        5 => bytes(ffi::name(n)),
        6 => {
            let local = bytes(ffi::name(n));
            let mut name = bytes(ffi::ns(n, 1));
            if name.is_empty() { local } else { name.push(b':'); name.extend(local); name }
        }
        _ => Vec::new(),
    } }) }).unwrap_or_default()))
}
/// Reads a DOM attribute by its in-scope qualified name, including namespace declarations.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_sx_dom_attribute(handle: i64, name: *const c_char) -> *const c_char {
    guard(|| output(view(handle).and_then(|v| v.first()
        .map(|node| owned_bytes(ffi::dom_attribute_text(node, cstring(bytes(name)).as_ptr())))).unwrap_or_default()))
}
/// Returns the indexed child visited by foreach and explicit iterator methods.
#[no_mangle]
pub extern "C" fn elephc_xml_sx_item(handle: i64, index: i64) -> i64 {
    guard(|| view(handle).and_then(|v| v.item(index.max(0) as usize)).map(insert).unwrap_or(0))
}
/// Rewinds (0), advances (1), checks validity (2), or wraps the current cursor node (3).
#[no_mangle]
pub extern "C" fn elephc_xml_sx_iterator(handle: i64, operation: i64) -> i64 {
    guard(|| match operation {
        0 => {
            let Some(view) = view(handle) else { return 0; };
            let cursor = iteration::Cursor::new(&view);
            REGISTRY.with(|r| { r.borrow_mut().cursors.insert(handle, cursor); });
            1
        }
        1 => {
            REGISTRY.with(|r| {
                if let Some(cursor) = r.borrow_mut().cursors.get_mut(&handle) { cursor.advance(); }
            });
            1
        }
        2 => REGISTRY.with(|r| i64::from(r.borrow().cursors.get(&handle)
            .and_then(iteration::Cursor::current).is_some())),
        3 => {
            let node = REGISTRY.with(|r| r.borrow().cursors.get(&handle)
                .and_then(iteration::Cursor::current));
            let Some(node) = node else { return 0; };
            view(handle).map(|view| insert(view.item_view(node))).unwrap_or(0)
        }
        _ => 0,
    })
}
/// Writes a property/offset, removes one selection (10+kind), or adds a child/attribute.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_sx_mutate(handle: i64, operation: i64, key: *const c_char, value: *const c_char, ns: *const c_char, flags: i64, index: i64) -> i64 {
    guard(|| {
        let Some(v) = view(handle) else { return 0; };
        let key = bytes(key); let text = bytes(value); let uri = bytes(ns);
        match operation {
            2..=5 => v.write(operation, &key, index, &text),
            12..=14 => { v.remove(operation - 10, &key, index); 1 }
            20 | 21 => v.add_checked(&key, (flags & 1 != 0).then_some(text.as_slice()),
                (flags & 2 != 0).then_some(uri.as_slice()), operation == 21)
                .map(|n| {
                    let mut child = v.with_scope(Scope::Node(n));
                    if operation == 20 && (flags & 2 != 0 || key.contains(&b':')) {
                        child.ns = Namespace { value: bytes(ffi::ns(n, 1)), prefix: true };
                    }
                    insert(child)
                }).unwrap_or_else(|error| error as i64),
            22 => v.first().map(|n| { v.assign_node(n, &text); 1 }).unwrap_or(0),
            23 => v.ensure_first().map(|node| insert(v.with_scope(Scope::Node(node)))).unwrap_or(0),
            24 => v.append_sibling().map(insert).unwrap_or(0),
            25 => v.first().map(|node| {
                let mut status = 0;
                let attr = ffi::dom_attribute_target(node, cstring(&key).as_ptr(), cstring(&text).as_ptr(), &mut status);
                if !attr.is_null() { v.assign_node(attr, &text); }
                i64::from(status)
            }).unwrap_or(0),
            26 => v.first().map(|node| { v.assign_dom_node_value(node, &text); 1 }).unwrap_or(0),
            _ => 0,
        }
    })
}
/// Registers a wrapper's XPath prefix even when its live selection currently has no nodes.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_sx_register(handle: i64, prefix: *const c_char, uri: *const c_char) -> i64 {
    guard(|| {
        let Some(v) = view(handle) else { return 0; };
        let node = v.first().unwrap_or(std::ptr::null_mut());
        let context = ffi::xpath_context(v.doc.raw, node);
        if context.is_null() { return 0; }
        let status = ffi::xpath_register(context, prefix, uri);
        ffi::xpath_context_free(context);
        if status != 0 { return 0; }
        v.xpath_ns.borrow_mut().insert(bytes(prefix), bytes(uri));
        1
    })
}
/// Evaluates XPath; -1 is invalid and other negative values encode scalar result kinds.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_sx_xpath(handle: i64, expression: *const c_char) -> i64 {
    guard(|| { diagnostics::reset(); view(handle).map(|v| {
        if v.first().is_none() || matches!(v.scope, Scope::Attributes(..) | Scope::AttributeMiss(..)) { return 0; }
        match v.xpath(&bytes(expression)) {
            Ok(nodes) => insert(v.with_scope(Scope::XPath(Rc::new(nodes)))),
            Err(query::XPathError::Invalid) => -1,
            Err(query::XPathError::Scalar(kind)) => -i64::from(kind) - 1,
        }
    }).unwrap_or(-1) })
}
/// Reads one namespace prefix/URI or the entry count (index -1), encoded as a C string.
/// Index -2 returns one owned snapshot as hex-encoded NUL-separated prefix/URI pairs.
#[no_mangle]
pub extern "C" fn elephc_xml_sx_namespace(handle: i64, declared: i64, recursive: i64, root: i64, index: i64, field: i64) -> *const c_char {
    guard(|| output(view(handle).map(|v| {
        let entries = v.namespaces(declared != 0, recursive != 0, root != 0);
        if index == -2 {
            let mut packed = Vec::new();
            for (prefix, uri) in entries {
                packed.extend(prefix); packed.push(0);
                packed.extend(uri); packed.push(0);
            }
            return encode_hex(&packed).into_bytes();
        }
        if index < 0 { return entries.len().to_string().into_bytes(); }
        entries.get(index as usize).map(|(prefix, uri)| if field == 0 { prefix.clone() } else { uri.clone() }).unwrap_or_default()
    }).unwrap_or_default()))
}

/// Creates a new PHP wrapper over native storage with its own empty XPath context.
#[no_mangle]
pub extern "C" fn elephc_xml_sx_alias(handle: i64) -> i64 {
    guard(|| view(handle).map(|v| {
        let mut alias = v.with_scope(v.scope.clone());
        alias.identity = v.identity;
        alias.refresh_pins();
        insert(alias)
    }).unwrap_or(0))
}

/// Compares PHP's retained base-node identity; distinct XML objects are unordered, not textual.
#[no_mangle]
pub extern "C" fn elephc_xml_sx_compare(left: i64, right: i64) -> i64 {
    guard(|| {
        let (Some(left), Some(right)) = (view(left), view(right)) else { return 2; };
        let equal = if left.identity.is_null() && right.identity.is_null() { left.doc.raw == right.doc.raw }
            else { !left.identity.is_null() && left.identity == right.identity };
        if equal { 0 } else { 2 }
    })
}

/// Encodes bytes as hex: node (0), SimpleXML (1), DOM document (2), or document node (3).
#[no_mangle]
pub extern "C" fn elephc_xml_sx_xml_hex(handle: i64, whole: i64) -> *const c_char {
    guard(|| output(view(handle).and_then(|v| v.first().map(|node| unsafe {
        let mut length = 0;
        let mode = match whole { 0 => 0, 2 => 2, 3 => 3, _ => 1 };
        let raw = ffi::dump_bytes(v.doc.raw, node, mode, &mut length);
        if raw.is_null() { return String::new(); }
        let bytes = std::slice::from_raw_parts(raw.cast::<u8>(), length.max(0) as usize);
        let hex = encode_hex(bytes);
        ffi::string_free(raw.cast());
        hex
    })).unwrap_or_default()))
}

/// Returns the element underlying a children/attributes projection, retaining its namespace filter.
#[no_mangle]
pub extern "C" fn elephc_xml_sx_projection_base(handle: i64) -> i64 {
    guard(|| view(handle).map(|v| {
        let node = match &v.scope {
            Scope::Children(parent, None) | Scope::Attributes(parent, _) | Scope::AttributeMiss(parent, _) => parent.first(),
            _ => v.first(),
        };
        insert(v.with_scope(Scope::Node(node.unwrap_or(std::ptr::null_mut()))))
    }).unwrap_or(0))
}
