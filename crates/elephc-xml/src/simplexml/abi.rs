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
struct Registry { next: i64, views: HashMap<i64, View> }
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
fn output(value: String) -> *const c_char {
    OUTPUT.with(|cell| { let mut out = cell.borrow_mut(); *out = cstring(&value); out.as_ptr() })
}
/// Parses the full byte length, including embedded NULs, into a document-owned root.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_sx_load(data: *const c_char, length: i64, options: i64, ns: *const c_char, prefix: i64, url: *const c_char) -> i64 {
    guard(|| {
        diagnostics::reset();
        if data.is_null() || length < 0 || length > i32::MAX as i64 { return 0; }
        parse_with_url(std::slice::from_raw_parts(data.cast(), length as usize), options,
            &string(ns), prefix != 0, &string(url)).map(insert).unwrap_or(0)
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
/// Opens a file or URL through libxml so read failures retain the SimpleXML caller's warnings.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_sx_load_file(filename: *const c_char, options: i64, ns: *const c_char, prefix: i64) -> i64 {
    guard(|| parse_file(filename, options, &string(ns), prefix != 0).map(insert).unwrap_or(0))
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
        from_document(raw, &string(ns), prefix != 0).map(insert).unwrap_or(0)
    })
}
/// Consumes formatted diagnostics before user warning handlers can run nested native operations.
#[no_mangle]
pub extern "C" fn elephc_xml_sx_diagnostics() -> *const c_char { guard(|| output(diagnostics::take())) }
/// Releases one wrapper; the document dies only after its final selection is released.
#[no_mangle]
pub extern "C" fn elephc_xml_sx_free(handle: i64) { guard(|| REGISTRY.with(|r| { r.borrow_mut().views.remove(&handle); })); }
/// Returns a fresh handle over an independent document and any detached aliased nodes.
#[no_mangle]
pub extern "C" fn elephc_xml_sx_clone(handle: i64) -> i64 { guard(|| view(handle).and_then(|v| v.deep_clone()).map(insert).unwrap_or(0)) }
/// Selects children (0), attributes (1), a property (2), an attribute (3), or offset (4).
/// Internal modes 7/8 preserve the existing namespace filter for projections and iteration.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_sx_select(handle: i64, kind: i64, name: *const c_char, ns: *const c_char, prefix: i64, index: i64) -> i64 {
    guard(|| view(handle).map(|v| {
        if kind == 4 && index < 0 { return insert(v.with_scope(Scope::Nodes(vec![]))); }
        let mut selected = v.select(kind, &string(name), &string(ns), prefix != 0, index.max(0) as usize);
        if kind == 3 || (kind == 2 && matches!(v.scope, Scope::Attributes(..))) {
            if let Some(node) = selected.first() { selected.scope = Scope::Node(node); selected.identity = node; }
        }
        insert(selected)
    }).unwrap_or(0))
}
/// Reads count (0), presence (1), iteration count (2), truthiness (3), or node kind (4).
#[no_mangle]
pub extern "C" fn elephc_xml_sx_int(handle: i64, operation: i64) -> i64 {
    guard(|| view(handle).map(|v| match operation {
        0 | 2 => v.items().len() as i64,
        1 => i64::from(v.first().is_some()),
        3 => if matches!(v.scope, Scope::Node(_) | Scope::Index(..)) {
            i64::from(!v.items().is_empty() || v.first().is_some_and(|n| unsafe {
                ffi::node_type(n) == 2 || !owned_string(ffi::text(n)).is_empty()
                    || (ffi::node_type(n) == 1 && !ffi::attrs(n).is_null())
            }))
        } else { i64::from(!v.nodes().is_empty()) },
        4 => v.first().map(|n| unsafe { ffi::node_type(n) as i64 }).unwrap_or(0),
        5 => match v.scope { Scope::Children(_, None) => 1, Scope::Children(_, Some(_)) => 2,
            Scope::Attributes(_, None) => 3, Scope::Attributes(_, Some(_)) => 4, _ => 0 },
        6 => v.first().map(|n| unsafe { let child = ffi::child(n);
            i64::from(!child.is_null() && matches!(ffi::node_type(child), 3 | 4)) }).unwrap_or(0),
        7 => v.first().map(|n| unsafe { i64::from(ffi::empty(n) != 0) }).unwrap_or(1),
        9 => i64::from(v.first().is_some_and(|node| node == unsafe { ffi::root(v.doc.raw) })),
        _ => 0,
    }).unwrap_or(0))
}
/// Copies string conversion (0), local name (1), or XML serialization (2).
#[no_mangle]
pub extern "C" fn elephc_xml_sx_string(handle: i64, operation: i64) -> *const c_char {
    guard(|| output(view(handle).and_then(|v| v.first().map(|n| unsafe { match operation {
        0 => owned_string(ffi::text(n)), 1 => string(ffi::name(n)),
        2 => owned_string(ffi::dump(v.doc.raw, n)),
        3 => owned_string(ffi::full_text(n)), 4 => owned_string(ffi::dump_node(v.doc.raw, n)),
        _ => String::new(),
    } })).unwrap_or_default()))
}
/// Returns the indexed child visited by foreach and explicit iterator methods.
#[no_mangle]
pub extern "C" fn elephc_xml_sx_item(handle: i64, index: i64) -> i64 {
    guard(|| view(handle).and_then(|v| v.item(index.max(0) as usize)).map(insert).unwrap_or(0))
}
/// Writes a property/offset, removes one selection (10+kind), or adds a child/attribute.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_sx_mutate(handle: i64, operation: i64, key: *const c_char, value: *const c_char, ns: *const c_char, flags: i64, index: i64) -> i64 {
    guard(|| {
        let Some(v) = view(handle) else { return 0; };
        let key = string(key); let text = string(value); let uri = string(ns);
        match operation {
            2..=5 => v.write(operation, &key, index, &text),
            12..=14 => { v.remove(operation - 10, &key, index); 1 }
            20 | 21 => v.add_checked(&key, (flags & 1 != 0).then_some(text.as_str()),
                (flags & 2 != 0).then_some(uri.as_str()), operation == 21)
                .map(|n| {
                    let mut child = v.with_scope(Scope::Node(n));
                    if operation == 20 && (flags & 2 != 0 || key.contains(':')) {
                        child.ns = Namespace { value: string(ffi::ns(n, 1)), prefix: true };
                    }
                    insert(child)
                }).unwrap_or_else(|error| error as i64),
            22 => v.first().map(|n| { v.assign_node(n, &text); 1 }).unwrap_or(0),
            23 => v.ensure_first().map(|node| insert(v.with_scope(Scope::Node(node)))).unwrap_or(0),
            _ => 0,
        }
    })
}
/// Registers an explicit prefix binding for this element's future XPath queries.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_sx_register(handle: i64, prefix: *const c_char, uri: *const c_char) -> i64 {
    guard(|| {
        let Some(v) = view(handle) else { return 0; };
        let Some(node) = v.first() else { return 0; };
        let context = ffi::xpath_context(v.doc.raw, node);
        if context.is_null() { return 0; }
        let status = ffi::xpath_register(context, prefix, uri);
        ffi::xpath_context_free(context);
        if status != 0 { return 0; }
        v.xpath_ns.borrow_mut().insert(string(prefix), string(uri));
        1
    })
}
/// Evaluates XPath; -1 is invalid and other negative values encode scalar result kinds.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_sx_xpath(handle: i64, expression: *const c_char) -> i64 {
    guard(|| { diagnostics::reset(); view(handle).map(|v| {
        if v.first().is_none() || matches!(v.scope, Scope::Attributes(..)) { return 0; }
        match v.xpath(&string(expression)) {
            Ok(nodes) => insert(v.with_scope(Scope::XPath(nodes))),
            Err(query::XPathError::Invalid) => -1,
            Err(query::XPathError::Scalar(kind)) => -i64::from(kind) - 1,
        }
    }).unwrap_or(-1) })
}
/// Reads one namespace prefix/URI or the entry count (index -1), encoded as a C string.
#[no_mangle]
pub extern "C" fn elephc_xml_sx_namespace(handle: i64, declared: i64, recursive: i64, root: i64, index: i64, field: i64) -> *const c_char {
    guard(|| output(view(handle).map(|v| {
        let entries = v.namespaces(declared != 0, recursive != 0, root != 0);
        if index < 0 { return entries.len().to_string(); }
        entries.get(index as usize).map(|(prefix, uri)| if field == 0 { prefix.clone() } else { uri.clone() }).unwrap_or_default()
    }).unwrap_or_default()))
}

/// Creates a new PHP wrapper over native storage with its own empty XPath context.
#[no_mangle]
pub extern "C" fn elephc_xml_sx_alias(handle: i64) -> i64 {
    guard(|| view(handle).map(|v| {
        let mut alias = v.with_scope(v.scope.clone());
        alias.identity = v.identity;
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

/// Encodes serialized bytes as ASCII hex so the extern C-string ABI preserves all encodings.
#[no_mangle]
pub extern "C" fn elephc_xml_sx_xml_hex(handle: i64, whole: i64) -> *const c_char {
    guard(|| output(view(handle).and_then(|v| v.first().map(|node| unsafe {
        let mut length = 0;
        let raw = ffi::dump_bytes(v.doc.raw, node, i32::from(whole != 0), &mut length);
        if raw.is_null() { return String::new(); }
        let bytes = std::slice::from_raw_parts(raw.cast::<u8>(), length.max(0) as usize);
        let mut hex = String::with_capacity(bytes.len().saturating_mul(2));
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        for byte in bytes { hex.push(DIGITS[(byte >> 4) as usize] as char); hex.push(DIGITS[(byte & 15) as usize] as char); }
        ffi::string_free(raw.cast());
        hex
    })).unwrap_or_default()))
}

/// Returns the element underlying a children/attributes projection, retaining its namespace filter.
#[no_mangle]
pub extern "C" fn elephc_xml_sx_projection_base(handle: i64) -> i64 {
    guard(|| view(handle).map(|v| {
        let node = match &v.scope {
            Scope::Children(parent, None) | Scope::Attributes(parent, _) => parent.first(),
            _ => v.first(),
        };
        insert(v.with_scope(Scope::Node(node.unwrap_or(std::ptr::null_mut()))))
    }).unwrap_or(0))
}
