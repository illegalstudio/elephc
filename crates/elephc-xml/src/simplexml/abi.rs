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
pub unsafe extern "C" fn elephc_xml_sx_load(data: *const c_char, length: i64, options: i64, ns: *const c_char, prefix: i64) -> i64 {
    guard(|| {
        if data.is_null() || length <= 0 || length > i32::MAX as i64 { return 0; }
        parse(std::slice::from_raw_parts(data.cast(), length as usize), options, &string(ns), prefix != 0).map(insert).unwrap_or(0)
    })
}
/// Releases one wrapper; the document dies only after its final selection is released.
#[no_mangle]
pub extern "C" fn elephc_xml_sx_free(handle: i64) { guard(|| REGISTRY.with(|r| { r.borrow_mut().views.remove(&handle); })); }
/// Returns a fresh handle sharing the exact view, for cloning a PHP object.
#[no_mangle]
pub extern "C" fn elephc_xml_sx_clone(handle: i64) -> i64 { guard(|| view(handle).and_then(|v| v.deep_clone()).map(insert).unwrap_or(0)) }
/// Selects children (0), attributes (1), a property (2), an attribute (3), or offset (4).
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_sx_select(handle: i64, kind: i64, name: *const c_char, ns: *const c_char, prefix: i64, index: i64) -> i64 {
    guard(|| view(handle).map(|v| {
        if kind == 4 && index < 0 { return insert(v.with_scope(Scope::Nodes(vec![]))); }
        insert(v.select(kind, &string(name), &string(ns), prefix != 0, index.max(0) as usize))
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
    guard(|| view(handle).and_then(|v| v.items().get(index.max(0) as usize).copied().map(|n| insert(v.with_scope(Scope::Node(n))))).unwrap_or(0))
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
            20 | 21 => v.add(&key, (flags & 1 != 0).then_some(text.as_str()),
                (flags & 2 != 0).then_some(uri.as_str()), operation == 21)
                .map(|n| insert(v.with_scope(Scope::Node(n)))).unwrap_or(0),
            22 => v.first().map(|n| { v.assign_node(n, &text); 1 }).unwrap_or(0),
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
/// Evaluates XPath into a temporary result-list handle; -1 means an invalid expression.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_sx_xpath(handle: i64, expression: *const c_char) -> i64 {
    guard(|| view(handle).map(|v| v.xpath(&string(expression)).map(|nodes| insert(v.with_scope(Scope::Nodes(nodes)))).unwrap_or(-1)).unwrap_or(-1))
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

/// Creates another wrapper over the same native selection, without PHP clone's deep copy.
#[no_mangle]
pub extern "C" fn elephc_xml_sx_alias(handle: i64) -> i64 { guard(|| view(handle).map(insert).unwrap_or(0)) }

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
