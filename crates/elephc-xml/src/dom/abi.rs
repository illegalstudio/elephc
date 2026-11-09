//! Purpose:
//! The panic-free `elephc_xml_dom_*` C ABI over the legacy DOM engine, declared by the
//! compiler's DOM prelude as part of the `extern "elephc_xml"` block.
//!
//! Called from:
//! - Compiled PHP programs (and `eval()` through the same compiled declarations).
//!
//! Key details:
//! - Handles are positive `i64`; 0 means PHP null/false. Tree operations return the
//!   resulting node's handle, or `-code` for a DOMException code, or `-100` for php-src's
//!   "Document Fragment is empty" warning.
//! - Nullable strings return NULL for PHP null; returned strings stay valid until the next
//!   string-returning call on this thread.
//! - Parse diagnostics go through the SimpleXML warning channel (`elephc_xml_sx_diagnostics`)
//!   and the libxml error state, exactly as SimpleXML's DOM loader reports them.

use super::*;
use std::panic::{catch_unwind, AssertUnwindSafe};

thread_local! { static OUTPUT: RefCell<Option<CString>> = const { RefCell::new(None) }; }

/// The code `insert`/`remove_child` report for php-src's empty-fragment warning.
const EMPTY_FRAGMENT: i64 = -100;

/// Runs an ABI operation without allowing a Rust panic to unwind into generated code.
fn guard<T: Default>(body: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(body)).unwrap_or_default()
}

/// Stashes a string (or PHP null) until the next string-returning call on this thread.
fn output(value: Option<Vec<u8>>) -> *const c_char {
    OUTPUT.with(|cell| {
        let mut cell = cell.borrow_mut();
        *cell = value.map(cstring);
        cell.as_ref().map_or(std::ptr::null(), |value| value.as_ptr())
    })
}

/// Copies a borrowed C string argument.
unsafe fn arg(raw: *const c_char) -> Vec<u8> {
    optional_bytes(raw).unwrap_or_default()
}

/// Encodes a tree-operation result for the prelude.
fn tree_result(owner: &Entry, result: Result<Node, i64>) -> i64 {
    match result {
        Ok(node) => handle_for(&owner.doc, node),
        Err(-1) => 0,
        Err(0) => EMPTY_FRAGMENT,
        Err(code) => -code,
    }
}

/// `new DOMDocument($version, $encoding)`.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_dom_document_new(version: *const c_char, encoding: *const c_char) -> i64 {
    guard(|| new_document(&arg(version), &arg(encoding)))
}

/// `DOMDocument::loadXML()`: 1 when the document was replaced, 0 on failure.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_dom_load(handle: i64, data: *const c_char, length: i64, options: i64) -> i64 {
    guard(|| {
        if data.is_null() || length < 0 {
            return 0;
        }
        let bytes = std::slice::from_raw_parts(data.cast::<u8>(), length as usize);
        i64::from(load_xml(handle, bytes, options))
    })
}

/// `DOMDocument::load()`: 1 when the document was replaced, 0 on failure.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_dom_load_file(handle: i64, filename: *const c_char, options: i64) -> i64 {
    guard(|| i64::from(load_file(handle, &arg(filename), options)))
}

/// `DOMDocument::saveXML()`: the document (`node` 0) or one node; NULL when saving failed.
#[no_mangle]
pub extern "C" fn elephc_xml_dom_save(document: i64, node: i64, format: i64, no_decl: i64, no_empty_tags: i64) -> *const c_char {
    guard(|| {
        let Some(document) = entry(document) else { return output(None) };
        let node = if node == 0 { None } else { entry(node) };
        output(save_xml(&document, node.as_ref(), format != 0, no_decl != 0, no_empty_tags != 0))
    })
}

/// `DOMDocument::loadHTML()` (empty `filename`) / `loadHTMLFile()`: 1 when the document was
/// replaced, 0 on failure.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_dom_load_html(handle: i64, data: *const c_char, length: i64,
        filename: *const c_char, options: i64) -> i64 {
    guard(|| {
        if length < 0 {
            return 0;
        }
        let bytes = if data.is_null() { &[][..] } else { std::slice::from_raw_parts(data.cast::<u8>(), length as usize) };
        let filename = arg(filename);
        let filename = if filename.is_empty() { None } else { Some(filename) };
        i64::from(load_html(handle, bytes, filename.as_deref(), options))
    })
}

/// `DOMDocument::saveHTML()`: the document (`node` 0) or one node; NULL when php returns false.
#[no_mangle]
pub extern "C" fn elephc_xml_dom_save_html(document: i64, node: i64, format: i64) -> *const c_char {
    guard(|| {
        let Some(document) = entry(document) else { return output(None) };
        let node = if node == 0 { None } else { entry(node) };
        output(save_html(&document, node.as_ref(), format != 0))
    })
}

/// Sets the declared encoding of the document behind `document` (modern `Dom\XMLDocument`
/// factories serialize as UTF-8).
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_dom_set_encoding(document: i64, encoding: *const c_char) {
    guard(|| {
        if let Some(entry) = entry(document) {
            let encoding = cstring(arg(encoding));
            crate::simplexml::set_document_encoding(document_raw(&entry), encoding.as_ptr());
        }
    })
}

/// The node's `nodeType`, or 0 for an unknown handle.
#[no_mangle]
pub extern "C" fn elephc_xml_dom_type(handle: i64) -> i64 {
    guard(|| entry(handle).map_or(0, |entry| i64::from(node_type(entry.node))))
}

/// A related node: 0 parent, 1 first child, 2 last child, 3 previous sibling, 4 next sibling,
/// 5 owner document, 6 document element. 0 when there is none.
#[no_mangle]
pub extern "C" fn elephc_xml_dom_node(handle: i64, relation: i64) -> i64 {
    guard(|| {
        let Some(entry) = entry(handle) else { return 0 };
        let related = unsafe {
            match relation {
                0 => ffi::parent(entry.node),
                1 => if children_valid(entry.node) || node_type(entry.node) == kind::ATTRIBUTE { ffi::child(entry.node) } else { std::ptr::null_mut() },
                2 => if children_valid(entry.node) || node_type(entry.node) == kind::ATTRIBUTE { ffi::last(entry.node) } else { std::ptr::null_mut() },
                3 => if node_type(entry.node) == kind::ATTRIBUTE { std::ptr::null_mut() } else { ffi::previous(entry.node) },
                4 => if node_type(entry.node) == kind::ATTRIBUTE { std::ptr::null_mut() } else { ffi::next(entry.node) },
                5 => match node_type(entry.node) {
                    kind::DOCUMENT | kind::HTML_DOCUMENT => std::ptr::null_mut(),
                    _ => owner_document(entry.node),
                },
                6 => ffi::root(document_raw(&entry)),
                _ => std::ptr::null_mut(),
            }
        };
        handle_for(&entry.doc, related)
    })
}

/// A string property: 0 nodeName, 1 nodeValue, 2 textContent, 3 localName, 4 prefix,
/// 5 namespaceURI, 6 xmlVersion, 7 xmlEncoding, 8 documentURI. NULL is PHP null.
#[no_mangle]
pub extern "C" fn elephc_xml_dom_string(handle: i64, field: i64) -> *const c_char {
    guard(|| {
        let Some(entry) = entry(handle) else { return output(None) };
        let node = entry.node;
        let value = unsafe {
            match field {
                0 => Some(node_name(node)),
                1 => node_value(node),
                2 => text_content(node),
                3 => match node_type(node) {
                    kind::ELEMENT | kind::ATTRIBUTE => optional_bytes(ffi::name(node)),
                    kind::NAMESPACE_DECL => node_name(node).strip_prefix(b"xmlns:").map(<[u8]>::to_vec).or(Some(b"xmlns".to_vec())),
                    _ => None,
                },
                4 => match node_type(node) {
                    kind::ELEMENT | kind::ATTRIBUTE | kind::NAMESPACE_DECL => {
                        Some(optional_bytes(ffi::ns(node, 1)).unwrap_or_default())
                    }
                    _ => None,
                },
                5 => match node_type(node) {
                    kind::ELEMENT | kind::ATTRIBUTE | kind::NAMESPACE_DECL => optional_bytes(ffi::ns(node, 0)),
                    _ => None,
                },
                6 => optional_bytes(ffi::doc_string(document_raw(&entry), 0)),
                7 => optional_bytes(ffi::doc_string(document_raw(&entry), 1)),
                8 => optional_bytes(ffi::doc_string(document_raw(&entry), 2)),
                _ => None,
            }
        };
        output(value)
    })
}

/// Writes `nodeValue` / `textContent`.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_dom_set_text(handle: i64, value: *const c_char) {
    guard(|| {
        if let Some(entry) = entry(handle) {
            set_text(&entry, &arg(value));
        }
    })
}

/// The `index`-th child, or 0.
#[no_mangle]
pub extern "C" fn elephc_xml_dom_child(handle: i64, index: i64) -> i64 {
    guard(|| {
        let Some(entry) = entry(handle) else { return 0 };
        if index < 0 {
            return 0;
        }
        handle_for(&entry.doc, child_at(entry.node, index as usize))
    })
}

/// The number of children (`childNodes->length`).
#[no_mangle]
pub extern "C" fn elephc_xml_dom_child_count(handle: i64) -> i64 {
    guard(|| {
        let Some(entry) = entry(handle) else { return 0 };
        let mut count = 0;
        while !child_at(entry.node, count).is_null() {
            count += 1;
        }
        count as i64
    })
}

/// `getAttribute()`: the value, or NULL when the attribute does not exist.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_dom_attribute(handle: i64, name: *const c_char) -> *const c_char {
    guard(|| match entry(handle) {
        Some(entry) if node_type(entry.node) == kind::ELEMENT => output(get_attribute(entry.node, &arg(name))),
        _ => output(None),
    })
}

/// `getAttributeNode()`: the attribute node, or 0.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_dom_attribute_node(handle: i64, name: *const c_char) -> i64 {
    guard(|| {
        let Some(entry) = entry(handle) else { return 0 };
        if node_type(entry.node) != kind::ELEMENT {
            return 0;
        }
        let name = cstring(arg(name));
        let attribute = ffi::has_prop(entry.node, name.as_ptr());
        if attribute.is_null() || node_type(attribute) != kind::ATTRIBUTE {
            return 0;
        }
        handle_for(&entry.doc, attribute)
    })
}

/// `setAttribute()`: the attribute node's handle.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_dom_set_attribute(handle: i64, name: *const c_char, value: *const c_char) -> i64 {
    guard(|| {
        let Some(entry) = entry(handle) else { return 0 };
        if node_type(entry.node) != kind::ELEMENT {
            return 0;
        }
        handle_for(&entry.doc, set_attribute(&entry, &arg(name), &arg(value)))
    })
}

/// `removeAttribute()`: 1 when an attribute was removed.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_dom_remove_attribute(handle: i64, name: *const c_char) -> i64 {
    guard(|| {
        let Some(entry) = entry(handle) else { return 0 };
        i64::from(node_type(entry.node) == kind::ELEMENT && remove_attribute(&entry, &arg(name)))
    })
}

/// The `index`-th attribute node, or 0.
#[no_mangle]
pub extern "C" fn elephc_xml_dom_attribute_at(handle: i64, index: i64) -> i64 {
    guard(|| {
        let Some(entry) = entry(handle) else { return 0 };
        if index < 0 || node_type(entry.node) != kind::ELEMENT {
            return 0;
        }
        handle_for(&entry.doc, attribute_at(entry.node, index as usize))
    })
}

/// `DOMDocument::create*()`: a detached node of `node_kind` (a libxml2 node type).
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_dom_create(document: i64, node_kind: i64, name: *const c_char, value: *const c_char) -> i64 {
    guard(|| {
        let Some(document) = entry(document) else { return 0 };
        let Ok(node_kind) = i32::try_from(node_kind) else { return 0 };
        match create(&document, node_kind, &arg(name), &arg(value)) {
            Some(node) => handle_for(&document.doc, node),
            None => 0,
        }
    })
}

/// `appendChild()` (`reference` 0) / `insertBefore()`.
#[no_mangle]
pub extern "C" fn elephc_xml_dom_insert(parent: i64, child: i64, reference: i64) -> i64 {
    guard(|| {
        let (Some(parent), Some(child)) = (entry(parent), entry(child)) else { return 0 };
        let reference = if reference == 0 { None } else { entry(reference) };
        let result = insert(&parent, &child, reference.as_ref());
        tree_result(&parent, result)
    })
}

/// `removeChild()`.
#[no_mangle]
pub extern "C" fn elephc_xml_dom_remove_child(parent: i64, child: i64) -> i64 {
    guard(|| {
        let (Some(parent), Some(child)) = (entry(parent), entry(child)) else { return 0 };
        let result = remove_child(&parent, &child);
        tree_result(&parent, result)
    })
}

/// `cloneNode()`.
#[no_mangle]
pub extern "C" fn elephc_xml_dom_clone(handle: i64, deep: i64) -> i64 {
    guard(|| {
        let Some(entry) = entry(handle) else { return 0 };
        clone_node(&entry, deep != 0).map_or(0, |copy| handle_for(&entry.doc, copy))
    })
}

/// `getElementsByTagName()->length`.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_dom_tag_count(handle: i64, name: *const c_char) -> i64 {
    guard(|| entry(handle).map_or(0, |entry| elements_by_tag_name(entry.node, &arg(name)).len() as i64))
}

/// `getElementsByTagName()->item($index)`, or 0.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_dom_tag_item(handle: i64, name: *const c_char, index: i64) -> i64 {
    guard(|| {
        let Some(entry) = entry(handle) else { return 0 };
        if index < 0 {
            return 0;
        }
        let found = elements_by_tag_name(entry.node, &arg(name));
        found.get(index as usize).map_or(0, |node| handle_for(&entry.doc, *node))
    })
}
