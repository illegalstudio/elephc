//! Purpose:
//! The legacy DOM engine (`DOMDocument`, `DOMNode` and friends) over libxml2: document
//! ownership, one stable handle per native node, and php-src's legacy tree algorithms
//! (`ext/dom/node.c`, `document.c`).
//!
//! Called from:
//! - `crate::dom::abi`, the `elephc_xml_dom_*` C ABI the compiler's DOM prelude declares.
//!
//! Key details:
//! - Every handle keeps its document alive (`Rc<DomDocument>`), so a node a PHP object can
//!   reach is never freed underneath it. Nodes that leave the tree (`removeChild()`, fresh
//!   `create*()` nodes, replaced attributes) are recorded as detached roots and freed with
//!   their document unless they were linked back in.
//! - The same native node always maps to the same handle, which lets the prelude keep one
//!   wrapper object per node so `===` holds as in PHP.
//! - `loadXML()` replaces the document behind an existing `DOMDocument` handle, as php-src
//!   swaps the `xmlDoc` behind the same object.
//! - Text insertion never goes through libxml2's merging helpers: the shim links text nodes
//!   by hand exactly where php-src does.

pub(crate) mod abi;
mod ffi;
#[cfg(all(test, elephc_xml_native))]
mod tests;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ffi::{c_char, CStr, CString};
use std::rc::Rc;

use ffi::Node;

/// libxml2 node type numbers (`xmlElementType`), which are also PHP's `XML_*_NODE` constants.
pub(crate) mod kind {
    pub(crate) const ELEMENT: i32 = 1;
    pub(crate) const ATTRIBUTE: i32 = 2;
    pub(crate) const TEXT: i32 = 3;
    pub(crate) const CDATA: i32 = 4;
    pub(crate) const ENTITY_REF: i32 = 5;
    pub(crate) const ENTITY: i32 = 6;
    pub(crate) const PI: i32 = 7;
    pub(crate) const COMMENT: i32 = 8;
    pub(crate) const DOCUMENT: i32 = 9;
    pub(crate) const DOCUMENT_TYPE: i32 = 10;
    pub(crate) const FRAGMENT: i32 = 11;
    pub(crate) const NOTATION: i32 = 12;
    pub(crate) const HTML_DOCUMENT: i32 = 13;
    pub(crate) const DTD: i32 = 14;
    pub(crate) const ELEMENT_DECL: i32 = 15;
    pub(crate) const ATTRIBUTE_DECL: i32 = 16;
    pub(crate) const ENTITY_DECL: i32 = 17;
    pub(crate) const NAMESPACE_DECL: i32 = 18;
}

/// php-src's DOMException codes (`ext/dom/dom_ce.h`).
pub(crate) mod error {
    pub(crate) const HIERARCHY_REQUEST: i64 = 3;
    pub(crate) const WRONG_DOCUMENT: i64 = 4;
    pub(crate) const NO_MODIFICATION_ALLOWED: i64 = 7;
    pub(crate) const NOT_FOUND: i64 = 8;
    pub(crate) const INVALID_STATE: i64 = 11;
}

/// Owns one libxml2 document and the subtrees that left it.
pub(crate) struct DomDocument {
    raw: Node,
    detached: RefCell<HashSet<Node>>,
}

impl Drop for DomDocument {
    /// Frees detached roots that were never linked back, then the document.
    fn drop(&mut self) {
        for node in self.detached.borrow_mut().drain() {
            unsafe {
                if !ffi::parent(node).is_null() {
                    continue;
                }
                if ffi::node_type(node) == kind::ATTRIBUTE {
                    ffi::free_prop(node);
                } else {
                    ffi::free_node(node);
                }
            }
        }
        unsafe { ffi::free_doc(self.raw) };
    }
}

impl DomDocument {
    /// Adopts a parsed or freshly created document.
    fn new(raw: Node) -> Rc<Self> {
        Rc::new(Self { raw, detached: RefCell::new(HashSet::new()) })
    }

    /// Records a parentless subtree this document must free unless it is linked back in.
    fn adopt_detached(&self, node: Node) {
        self.detached.borrow_mut().insert(node);
    }

    /// Forgets a detached root that was just linked into a tree.
    fn relink(&self, node: Node) {
        self.detached.borrow_mut().remove(&node);
    }
}

/// One live handle: a node and the document that owns it.
#[derive(Clone)]
pub(crate) struct Entry {
    pub(crate) doc: Rc<DomDocument>,
    pub(crate) node: Node,
}

#[derive(Default)]
struct Registry {
    next: i64,
    entries: HashMap<i64, Entry>,
    by_node: HashMap<Node, i64>,
}

thread_local! { static REGISTRY: RefCell<Registry> = RefCell::new(Registry::default()); }

/// Returns the stable handle of `node` in `doc`, minting one on first sight; 0 for null.
pub(crate) fn handle_for(doc: &Rc<DomDocument>, node: Node) -> i64 {
    if node.is_null() {
        return 0;
    }
    REGISTRY.with(|registry| {
        let mut registry = registry.borrow_mut();
        if let Some(handle) = registry.by_node.get(&node) {
            return *handle;
        }
        let Some(next) = registry.next.checked_add(1) else { return 0 };
        registry.next = next;
        registry.entries.insert(next, Entry { doc: doc.clone(), node });
        registry.by_node.insert(node, next);
        next
    })
}

/// Resolves a handle; `None` for 0 or an unknown handle.
pub(crate) fn entry(handle: i64) -> Option<Entry> {
    REGISTRY.with(|registry| registry.borrow().entries.get(&handle).cloned())
}

/// Points an existing handle at another node and document (`loadXML()` on a `DOMDocument`).
fn rebind(handle: i64, doc: Rc<DomDocument>, node: Node) {
    REGISTRY.with(|registry| {
        let mut registry = registry.borrow_mut();
        if let Some(old) = registry.entries.get(&handle).map(|entry| entry.node) {
            registry.by_node.remove(&old);
        }
        registry.entries.insert(handle, Entry { doc, node });
        registry.by_node.insert(node, handle);
    })
}

/// Creates an empty document (`new DOMDocument($version, $encoding)`) and returns its handle.
pub(crate) fn new_document(version: &[u8], encoding: &[u8]) -> i64 {
    crate::ffi::init_library();
    let version = cstring(version);
    let raw = unsafe { ffi::new_doc(version.as_ptr()) };
    if raw.is_null() {
        return 0;
    }
    if !encoding.is_empty() {
        let encoding = cstring(encoding);
        unsafe { crate::simplexml::set_document_encoding(raw, encoding.as_ptr()) };
    }
    handle_for(&DomDocument::new(raw), raw)
}

/// Parses `data` into the document behind `handle`; false (and warnings) on failure.
pub(crate) fn load_xml(handle: i64, data: &[u8], options: i64) -> bool {
    let Ok(length) = i32::try_from(data.len()) else { return false };
    let Ok(options) = i32::try_from(options) else { return false };
    crate::simplexml::diagnostics_reset();
    crate::ffi::init_library();
    let mut messages = std::ptr::null_mut();
    let raw = unsafe { ffi::parse(data.as_ptr().cast(), length, options, std::ptr::null(), 2, &mut messages) };
    unsafe { crate::simplexml::diagnostics_capture(messages) };
    adopt_loaded(handle, raw)
}

/// Opens `filename` into the document behind `handle`; false (and warnings) on failure.
pub(crate) fn load_file(handle: i64, filename: &[u8], options: i64) -> bool {
    let Ok(options) = i32::try_from(options) else { return false };
    crate::simplexml::diagnostics_reset();
    crate::ffi::init_library();
    let filename = cstring(filename);
    let mut messages = std::ptr::null_mut();
    let raw = unsafe { ffi::parse_file(filename.as_ptr(), options, &mut messages) };
    unsafe { crate::simplexml::diagnostics_capture(messages) };
    adopt_loaded(handle, raw)
}

/// Parses HTML bytes (`filename` `None`) or an HTML file into the document behind `handle`
/// with libxml2's HTML parser, as `loadHTML()` / `loadHTMLFile()` do.
pub(crate) fn load_html(handle: i64, data: &[u8], filename: Option<&[u8]>, options: i64) -> bool {
    let Ok(length) = i32::try_from(data.len()) else { return false };
    let Ok(options) = i32::try_from(options) else { return false };
    crate::simplexml::diagnostics_reset();
    crate::ffi::init_library();
    let filename = filename.map(cstring);
    let mut messages = std::ptr::null_mut();
    let raw = unsafe {
        ffi::parse_html(data.as_ptr().cast(), length,
            filename.as_ref().map_or(std::ptr::null(), |name| name.as_ptr()), options, &mut messages)
    };
    unsafe { crate::simplexml::diagnostics_capture(messages) };
    adopt_loaded(handle, raw)
}

/// Serializes the document (`node` `None`) or one node as HTML, as `saveHTML()` does.
pub(crate) fn save_html(document: &Entry, node: Option<&Entry>, format: bool) -> Option<Vec<u8>> {
    let mut length = 0;
    let raw = unsafe {
        ffi::save_html(document.doc.raw, node.map_or(std::ptr::null_mut(), |node| node.node),
            i32::from(format), &mut length)
    };
    if raw.is_null() {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(raw.cast::<u8>(), length.max(0) as usize).to_vec() };
    unsafe { ffi::string_free(raw.cast()) };
    Some(bytes)
}

/// Swaps a parsed document behind the `DOMDocument` handle; the old one lives on through
/// any node handle that still points into it.
fn adopt_loaded(handle: i64, raw: Node) -> bool {
    if raw.is_null() || entry(handle).is_none() {
        return false;
    }
    rebind(handle, DomDocument::new(raw), raw);
    true
}

/// Serializes the document (`node` 0) or one node, as `DOMDocument::saveXML()` does.
pub(crate) fn save_xml(document: &Entry, node: Option<&Entry>, format: bool, no_decl: bool, no_empty_tags: bool) -> Option<Vec<u8>> {
    let mut length = 0;
    let raw = unsafe {
        ffi::save(document.doc.raw, node.map_or(std::ptr::null_mut(), |node| node.node),
            i32::from(format), i32::from(no_decl), i32::from(no_empty_tags), &mut length)
    };
    if raw.is_null() {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(raw.cast::<u8>(), length.max(0) as usize).to_vec() };
    unsafe { ffi::string_free(raw.cast()) };
    Some(bytes)
}

/// The node type, with HTML documents reported as `XML_HTML_DOCUMENT_NODE` like libxml2.
pub(crate) fn node_type(node: Node) -> i32 {
    unsafe { ffi::node_type(node) }
}

/// Borrowed C string bytes, or `None` for NULL.
pub(crate) unsafe fn optional_bytes(raw: *const c_char) -> Option<Vec<u8>> {
    if raw.is_null() { None } else { Some(CStr::from_ptr(raw).to_bytes().to_vec()) }
}

/// Copies and frees a libxml2-allocated string.
pub(crate) unsafe fn owned_bytes(raw: *mut c_char) -> Option<Vec<u8>> {
    let bytes = optional_bytes(raw);
    if !raw.is_null() {
        ffi::string_free(raw.cast());
    }
    bytes
}

/// A NUL-free C string; PHP strings with NUL bytes are cut there, as libxml2 sees them.
pub(crate) fn cstring(value: impl AsRef<[u8]>) -> CString {
    CString::new(value.as_ref().split(|byte| *byte == 0).next().unwrap_or(b"")).unwrap_or_default()
}

/// `nodeName` per node type (php-src `dom_node_node_name_read`).
pub(crate) fn node_name(node: Node) -> Vec<u8> {
    unsafe {
        match ffi::node_type(node) {
            kind::ELEMENT | kind::ATTRIBUTE => {
                let local = optional_bytes(ffi::name(node)).unwrap_or_default();
                match optional_bytes(ffi::ns(node, 1)) {
                    Some(prefix) if !prefix.is_empty() => [prefix, b":".to_vec(), local].concat(),
                    _ => local,
                }
            }
            kind::TEXT => b"#text".to_vec(),
            kind::CDATA => b"#cdata-section".to_vec(),
            kind::COMMENT => b"#comment".to_vec(),
            kind::DOCUMENT | kind::HTML_DOCUMENT => b"#document".to_vec(),
            kind::FRAGMENT => b"#document-fragment".to_vec(),
            kind::NAMESPACE_DECL => match optional_bytes(ffi::ns(node, 1)) {
                Some(prefix) if !prefix.is_empty() => [b"xmlns:".to_vec(), prefix].concat(),
                _ => b"xmlns".to_vec(),
            },
            _ => optional_bytes(ffi::name(node)).unwrap_or_default(),
        }
    }
}

/// `nodeValue` per node type (php-src `dom_node_node_value_read`); `None` is PHP null.
pub(crate) fn node_value(node: Node) -> Option<Vec<u8>> {
    unsafe {
        match ffi::node_type(node) {
            kind::ATTRIBUTE | kind::TEXT | kind::ELEMENT | kind::COMMENT | kind::CDATA | kind::PI => {
                Some(owned_bytes(ffi::content(node)).unwrap_or_default())
            }
            kind::NAMESPACE_DECL => optional_bytes(ffi::ns(node, 0)),
            _ => None,
        }
    }
}

/// `textContent` (php-src `dom_node_text_content_read`): document-like nodes are null.
pub(crate) fn text_content(node: Node) -> Option<Vec<u8>> {
    match node_type(node) {
        kind::DOCUMENT | kind::HTML_DOCUMENT | kind::DOCUMENT_TYPE | kind::NOTATION => None,
        _ => Some(unsafe { owned_bytes(ffi::content(node)) }.unwrap_or_default()),
    }
}

/// Whether a node type can hold children (php-src `dom_node_children_valid`).
fn children_valid(node: Node) -> bool {
    !matches!(
        node_type(node),
        kind::DOCUMENT_TYPE | kind::DTD | kind::PI | kind::COMMENT | kind::TEXT | kind::CDATA | kind::NOTATION
    )
}

/// Whether a node is read-only in legacy DOM (php-src `dom_node_is_read_only`).
fn is_read_only(node: Node) -> bool {
    matches!(
        node_type(node),
        kind::ENTITY_REF | kind::ENTITY | kind::DOCUMENT_TYPE | kind::NOTATION | kind::DTD
            | kind::ELEMENT_DECL | kind::ATTRIBUTE_DECL | kind::ENTITY_DECL | kind::NAMESPACE_DECL
    )
}

/// Whether inserting `child` under `parent` would make a node its own ancestor
/// (php-src `dom_hierarchy`).
fn creates_cycle(parent: Node, child: Node) -> bool {
    let mut current = parent;
    while !current.is_null() {
        if current == child {
            return true;
        }
        current = unsafe { ffi::parent(current) };
    }
    false
}

/// The legacy insertion checks (php-src `dom_node_check_legacy_insertion_validity`);
/// `Err(code)` is the DOMException code to throw, `Err(0)` the empty-fragment warning.
fn check_insertion(parent: &Entry, child: &Entry) -> Result<(), i64> {
    let child_parent = unsafe { ffi::parent(child.node) };
    if is_read_only(parent.node) || (!child_parent.is_null() && is_read_only(child_parent)) {
        return Err(error::NO_MODIFICATION_ALLOWED);
    }
    if creates_cycle(parent.node, child.node) {
        return Err(error::HIERARCHY_REQUEST);
    }
    let parent_doc = unsafe { ffi::node_document(parent.node) };
    let child_doc = unsafe { ffi::node_document(child.node) };
    if child_doc != parent_doc && !child_doc.is_null() {
        return Err(error::WRONG_DOCUMENT);
    }
    let child_kind = node_type(child.node);
    if child_kind == kind::FRAGMENT && unsafe { ffi::child(child.node) }.is_null() {
        return Err(0);
    }
    let parent_kind = node_type(parent.node);
    if parent_kind == kind::ATTRIBUTE && child_kind != kind::TEXT && child_kind != kind::ENTITY_REF {
        return Err(error::HIERARCHY_REQUEST);
    }
    if child_kind == kind::ATTRIBUTE && parent_kind != kind::ELEMENT {
        return Err(error::HIERARCHY_REQUEST);
    }
    if child_kind == kind::DOCUMENT || child_kind == kind::HTML_DOCUMENT {
        return Err(error::HIERARCHY_REQUEST);
    }
    Ok(())
}

/// Detaches `node` from its parent, keeping it owned by its document.
fn unlink_retaining(doc: &DomDocument, node: Node) {
    if !unsafe { ffi::parent(node) }.is_null() {
        unsafe { ffi::unlink(node) };
    }
    doc.adopt_detached(node);
}

/// Inserts `child` into `parent` before `reference` (`None` appends), following php-src's
/// legacy `appendChild()` / `insertBefore()`. Returns the inserted node, or the DOMException
/// code (`0` for the empty-fragment warning).
pub(crate) fn insert(parent: &Entry, child: &Entry, reference: Option<&Entry>) -> Result<Node, i64> {
    if !children_valid(parent.node) {
        return Err(-1);
    }
    check_insertion(parent, child)?;
    if let Some(reference) = reference {
        if unsafe { ffi::parent(reference.node) } != parent.node {
            return Err(error::NOT_FOUND);
        }
    }
    let child_kind = node_type(child.node);
    if child_kind == kind::FRAGMENT {
        return Ok(insert_fragment(parent, child, reference.map(|reference| reference.node)));
    }
    if !unsafe { ffi::parent(child.node) }.is_null() {
        unsafe { ffi::unlink(child.node) };
    }
    child.doc.relink(child.node);
    let inserted = unsafe {
        match reference {
            Some(reference) => {
                let previous = ffi::previous(reference.node);
                if child_kind == kind::TEXT
                    && (node_type(reference.node) == kind::TEXT
                        || (!previous.is_null() && node_type(previous) == kind::TEXT))
                {
                    ffi::link_text_before(reference.node, child.node);
                    child.node
                } else {
                    if child_kind == kind::ATTRIBUTE {
                        replace_same_name_attribute(parent, child);
                    }
                    ffi::add_prev_sibling(reference.node, child.node)
                }
            }
            None => {
                let last = ffi::last(parent.node);
                if child_kind == kind::TEXT && !last.is_null() && node_type(last) == kind::TEXT {
                    ffi::link_text_last(parent.node, child.node);
                    child.node
                } else {
                    if child_kind == kind::ATTRIBUTE {
                        replace_same_name_attribute(parent, child);
                    }
                    ffi::add_child(parent.node, child.node)
                }
            }
        }
    };
    if inserted.is_null() {
        return Err(error::INVALID_STATE);
    }
    Ok(inserted)
}

/// Detaches an attribute of the same name before an attribute node is inserted, as php-src
/// does (it frees it; here it stays owned by the document in case PHP still holds it).
fn replace_same_name_attribute(parent: &Entry, child: &Entry) {
    let name = cstring(unsafe { optional_bytes(ffi::name(child.node)) }.unwrap_or_default());
    let existing = unsafe { ffi::has_prop(parent.node, name.as_ptr()) };
    if !existing.is_null() && node_type(existing) != kind::ATTRIBUTE_DECL && existing != child.node {
        unlink_retaining(&parent.doc, existing);
    }
}

/// Moves a fragment's children into `parent` before `reference` (php-src `dom_insert_fragment`)
/// and returns the first inserted node; the fragment is left empty.
fn insert_fragment(parent: &Entry, fragment: &Entry, reference: Option<Node>) -> Node {
    let mut first_inserted: Node = std::ptr::null_mut();
    let mut current = unsafe { ffi::child(fragment.node) };
    while !current.is_null() {
        let next = unsafe { ffi::next(current) };
        unsafe { ffi::unlink(current) };
        let inserted = unsafe {
            match reference {
                Some(reference) => ffi::add_prev_sibling(reference, current),
                None => ffi::add_child(parent.node, current),
            }
        };
        if first_inserted.is_null() {
            first_inserted = inserted;
        }
        current = next;
    }
    first_inserted
}

/// `removeChild()`: detaches `child` from `parent` (php-src `dom_node_remove_child`).
pub(crate) fn remove_child(parent: &Entry, child: &Entry) -> Result<Node, i64> {
    if !children_valid(parent.node) {
        return Err(-1);
    }
    if is_read_only(parent.node) || is_read_only(child.node) {
        return Err(error::NO_MODIFICATION_ALLOWED);
    }
    if unsafe { ffi::parent(child.node) } != parent.node {
        return Err(error::NOT_FOUND);
    }
    unlink_retaining(&parent.doc, child.node);
    Ok(child.node)
}

/// Creates a detached node of `node_kind` in `document`; `None` if libxml2 refuses.
pub(crate) fn create(document: &Entry, node_kind: i32, name: &[u8], value: &[u8]) -> Option<Node> {
    let doc = document.doc.raw;
    let name = cstring(name);
    let value_c = cstring(value);
    let node = unsafe {
        match node_kind {
            kind::ELEMENT => ffi::new_element(doc, std::ptr::null_mut(), name.as_ptr(),
                if value.is_empty() { std::ptr::null() } else { value_c.as_ptr() }),
            kind::TEXT => ffi::new_text(doc, value_c.as_ptr()),
            kind::COMMENT => ffi::new_comment(doc, value_c.as_ptr()),
            kind::CDATA => ffi::new_cdata(doc, value.as_ptr().cast(), i32::try_from(value.len()).ok()?),
            kind::PI => ffi::new_pi(doc, name.as_ptr(), if value.is_empty() { std::ptr::null() } else { value_c.as_ptr() }),
            kind::ATTRIBUTE => ffi::new_attribute(doc, name.as_ptr(), std::ptr::null()),
            kind::FRAGMENT => ffi::new_fragment(doc),
            _ => return None,
        }
    };
    if node.is_null() {
        return None;
    }
    document.doc.adopt_detached(node);
    Some(node)
}

/// `cloneNode()`: a detached copy (`deep` copies descendants and attributes).
pub(crate) fn clone_node(node: &Entry, deep: bool) -> Option<Node> {
    let doc = unsafe { ffi::node_document(node.node) };
    let copy = unsafe { ffi::copy_node(node.node, doc, if deep { 1 } else { 2 }) };
    if copy.is_null() {
        return None;
    }
    node.doc.adopt_detached(copy);
    Some(copy)
}

/// Writes `nodeValue` / `textContent` (php-src `dom_node_node_value_write`): element and
/// attribute children are replaced by one text node; text-like nodes get the new content.
/// Old children are detached rather than freed, since PHP objects may still hold them.
pub(crate) fn set_text(node: &Entry, value: &[u8]) {
    let node_kind = node_type(node.node);
    if matches!(node_kind, kind::ELEMENT | kind::ATTRIBUTE | kind::FRAGMENT) {
        loop {
            let child = unsafe { ffi::child(node.node) };
            if child.is_null() {
                break;
            }
            unlink_retaining(&node.doc, child);
        }
    }
    if matches!(
        node_kind,
        kind::ELEMENT | kind::ATTRIBUTE | kind::FRAGMENT | kind::TEXT | kind::COMMENT | kind::CDATA | kind::PI
    ) {
        let value = cstring(value);
        unsafe { ffi::set_content(node.node, value.as_ptr()) };
    }
}

/// `getAttribute()`: the attribute's value, or `None` when absent.
pub(crate) fn get_attribute(element: Node, name: &[u8]) -> Option<Vec<u8>> {
    let name = cstring(name);
    unsafe {
        if ffi::has_prop(element, name.as_ptr()).is_null() {
            return None;
        }
        owned_bytes(ffi::get_prop(element, name.as_ptr()))
    }
}

/// `setAttribute()`: creates or replaces the attribute and returns its node.
pub(crate) fn set_attribute(element: &Entry, name: &[u8], value: &[u8]) -> Node {
    let name = cstring(name);
    let value = cstring(value);
    unsafe {
        let existing = ffi::has_prop(element.node, name.as_ptr());
        if !existing.is_null() && node_type(existing) == kind::ATTRIBUTE {
            // Keep the attribute node (and any PHP object for it); replace only its value.
            ffi::set_content(existing, value.as_ptr());
            return existing;
        }
        ffi::set_prop(element.node, name.as_ptr(), value.as_ptr())
    }
}

/// `removeAttribute()`: detaches the attribute node; false when it does not exist.
pub(crate) fn remove_attribute(element: &Entry, name: &[u8]) -> bool {
    let name = cstring(name);
    let existing = unsafe { ffi::has_prop(element.node, name.as_ptr()) };
    if existing.is_null() || node_type(existing) != kind::ATTRIBUTE {
        return false;
    }
    unlink_retaining(&element.doc, existing);
    true
}

/// The `index`-th attribute node of an element, or null.
pub(crate) fn attribute_at(element: Node, index: usize) -> Node {
    let mut current = unsafe { ffi::attrs(element) };
    let mut position = 0;
    while !current.is_null() {
        if position == index {
            return current;
        }
        position += 1;
        current = unsafe { ffi::next(current) };
    }
    std::ptr::null_mut()
}

/// The `index`-th child node, or null.
pub(crate) fn child_at(parent: Node, index: usize) -> Node {
    if !children_valid(parent) && node_type(parent) != kind::ATTRIBUTE {
        return std::ptr::null_mut();
    }
    let mut current = unsafe { ffi::child(parent) };
    let mut position = 0;
    while !current.is_null() {
        if position == index {
            return current;
        }
        position += 1;
        current = unsafe { ffi::next(current) };
    }
    std::ptr::null_mut()
}

/// Elements below `root` in document order whose qualified name matches (`*` matches all),
/// as `getElementsByTagName()` sees them.
pub(crate) fn elements_by_tag_name(root: Node, name: &[u8]) -> Vec<Node> {
    let mut found = Vec::new();
    let mut stack = Vec::new();
    let mut child = unsafe { ffi::child(root) };
    // Depth-first, preserving document order.
    loop {
        while !child.is_null() {
            if node_type(child) == kind::ELEMENT {
                if name == b"*" || node_name(child) == name {
                    found.push(child);
                }
                let first = unsafe { ffi::child(child) };
                if !first.is_null() {
                    stack.push(unsafe { ffi::next(child) });
                    child = first;
                    continue;
                }
            }
            child = unsafe { ffi::next(child) };
        }
        match stack.pop() {
            Some(next) => child = next,
            None => break,
        }
    }
    found
}

/// Why `simplexml_import_dom()` refuses a node (php-src `simplexml_import_dom` messages).
pub(crate) enum ImportRefusal {
    /// "Imported Node must have associated Document".
    NoDocument,
    /// "Invalid Nodetype to import".
    InvalidNodeType,
}

/// The tree `simplexml_import_dom()` views for a DOM handle: the document element of a
/// document, or the element itself, with the document that keeps the tree alive.
pub(crate) fn simplexml_import_target(handle: i64) -> Result<(Node, Node, Rc<DomDocument>), ImportRefusal> {
    let Some(entry) = entry(handle) else { return Err(ImportRefusal::InvalidNodeType) };
    let node = match node_type(entry.node) {
        kind::DOCUMENT | kind::HTML_DOCUMENT => unsafe { ffi::root(entry.node) },
        kind::ELEMENT => entry.node,
        _ => return Err(ImportRefusal::InvalidNodeType),
    };
    if node.is_null() {
        return Err(ImportRefusal::InvalidNodeType);
    }
    let doc = owner_document(node);
    if doc.is_null() {
        return Err(ImportRefusal::NoDocument);
    }
    Ok((doc, node, entry.doc))
}

/// The document that owns a node (the document itself for a document node).
pub(crate) fn owner_document(node: Node) -> Node {
    unsafe { ffi::node_document(node) }
}

/// The document node behind an entry, which may differ from `ownerDocument` for nodes of a
/// replaced document.
pub(crate) fn document_raw(entry: &Entry) -> Node {
    entry.doc.raw
}
