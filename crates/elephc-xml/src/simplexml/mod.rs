//! Purpose:
//! Live SimpleXML selections over libxml2 documents, shared by AOT and eval.
//!
//! Called from:
//! - `abi` exports used by the compiler's SimpleXML prelude.
//!
//! Key details:
//! - Every view owns its document through Rc. Selectors are evaluated on access.
//! - Unlinked nodes stay owned until all views die, preventing stale-view UAF.
//! - State stays on its originating thread; no native pointer crosses threads.

mod abi;
mod cloning;
mod diagnostics;
mod ffi;
mod mutation;
mod query;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::{c_char, CStr, CString};
use std::rc::Rc;
use ffi::Node;

/// Owns a libxml2 document and nodes removed while wrappers still reference them.
struct Document { raw: Node, detached: RefCell<Vec<Node>> }
impl Drop for Document {
    /// Releases detached subtrees before the document's namespace and dictionary storage.
    fn drop(&mut self) {
        unsafe {
            for node in self.detached.get_mut().drain(..) { ffi::node_free(node); }
            ffi::doc_free(self.raw);
        }
    }
}

/// An empty selector matches unprefixed nodes, including nodes in a default namespace.
#[derive(Clone, Default)]
struct Namespace { value: String, prefix: bool }
impl Namespace {
    /// Tests a node's namespace using PHP's unqualified/default namespace rules.
    fn matches(&self, node: Node) -> bool {
        unsafe {
            if self.value.is_empty() {
                return borrowed_string_matches(ffi::ns(node, 1), "");
            }
            borrowed_string_matches(ffi::ns(node, i32::from(self.prefix)), &self.value)
        }
    }
}

/// One lazy selection, or a stable single node from numeric indexing or XPath.
#[derive(Clone)]
enum Scope {
    Node(Node),
    Children(Rc<View>, Option<String>),
    Attributes(Rc<View>, Option<String>),
    Index(Rc<View>, usize),
    Nodes(Rc<Vec<Node>>),
    XPath(Rc<Vec<Node>>),
}

/// PHP wrapper state; iterator position does not change through ordinary getters.
#[derive(Clone)]
struct View {
    doc: Rc<Document>, scope: Scope, ns: Namespace,
    identity: Node,
    xpath_ns: Rc<RefCell<BTreeMap<String, String>>>,
}
impl View {
    /// Resolves this selector against the current tree without caching removed children.
    fn nodes(&self) -> Vec<Node> {
        match &self.scope {
            Scope::Node(node) => if node.is_null() { vec![] } else { vec![*node] },
            Scope::Nodes(nodes) | Scope::XPath(nodes) => nodes.as_ref().clone(),
            Scope::Index(parent, index) => parent.node_at(*index).into_iter().collect(),
            Scope::Children(parent, name) | Scope::Attributes(parent, name) => {
                let Some(node) = parent.first() else { return vec![]; };
                let attrs = matches!(self.scope, Scope::Attributes(..));
                if attrs && unsafe { ffi::node_type(node) } != 1 { return vec![]; }
                let mut cursor = unsafe { if attrs { ffi::attrs(node) } else { ffi::child(node) } };
                let mut out = Vec::new();
                while !cursor.is_null() {
                    let kind = unsafe { ffi::node_type(cursor) };
                    if kind == (if attrs { 2 } else { 1 })
                        && self.ns.matches(cursor)
                        && name.as_ref().is_none_or(|name| unsafe { borrowed_string_matches(ffi::name(cursor), name) })
                    { out.push(cursor); }
                    cursor = unsafe { ffi::next(cursor) };
                }
                out
            }
        }
    }
    /// Returns the first matched node, if any.
    fn first(&self) -> Option<Node> { self.node_at(0) }
    /// Resolves one position without materializing the rest of a live selection.
    fn node_at(&self, index: usize) -> Option<Node> {
        match &self.scope {
            Scope::Node(node) => (index == 0 && !node.is_null()).then_some(*node),
            Scope::Nodes(nodes) | Scope::XPath(nodes) => nodes.get(index).copied(),
            Scope::Index(parent, selected) => (index == 0).then(|| parent.node_at(*selected)).flatten(),
            Scope::Children(parent, name) | Scope::Attributes(parent, name) => {
                let parent = parent.first()?;
                let attrs = matches!(self.scope, Scope::Attributes(..));
                if attrs && unsafe { ffi::node_type(parent) } != 1 { return None; }
                let mut node = unsafe { if attrs { ffi::attrs(parent) } else { ffi::child(parent) } };
                let mut position = 0;
                while !node.is_null() {
                    if unsafe { ffi::node_type(node) } == (if attrs { 2 } else { 1 })
                        && self.ns.matches(node)
                        && name.as_ref().is_none_or(|name| unsafe { borrowed_string_matches(ffi::name(node), name) })
                    {
                        if position == index { return Some(node); }
                        position += 1;
                    }
                    node = unsafe { ffi::next(node) };
                }
                None
            }
        }
    }
    /// Returns nodes traversed by foreach and count for this wrapper kind.
    fn items(&self) -> Vec<Node> {
        if matches!(self.scope, Scope::Node(_) | Scope::Index(..)) {
            let view = self.select(7, "", "", false, 0);
            view.nodes()
        } else { self.nodes() }
    }
    /// Materializes an XPath attribute as a filtered attribute list, preserving ordinary offsets.
    fn item(&self, index: usize) -> Option<Self> {
        let node = if matches!(self.scope, Scope::Node(_) | Scope::Index(..)) {
            self.select(7, "", "", false, 0).node_at(index)?
        } else { self.node_at(index)? };
        if matches!(self.scope, Scope::XPath(_)) && unsafe { ffi::node_type(node) } == 2 {
            let parent = self.with_scope(Scope::Node(unsafe { ffi::parent(node) }));
            let name = unsafe { string(ffi::name(node)) };
            let mut selected = self.with_scope(Scope::Attributes(Rc::new(parent), Some(name)));
            selected.ns = Namespace { value: unsafe { string(ffi::ns(node, 0)) }, prefix: false };
            return Some(selected);
        }
        Some(self.with_scope(Scope::Node(node)))
    }
    /// Builds a new wrapper with its own empty XPath context, sharing only native storage.
    fn with_scope(&self, scope: Scope) -> Self {
        let identity = scope_identity(&scope);
        Self { scope, identity, xpath_ns: Rc::new(RefCell::new(BTreeMap::new())), ..self.clone() }
    }
    /// Builds children, attributes, named properties, or a numeric offset selection.
    fn select(&self, kind: i64, name: &str, ns: &str, prefix: bool, index: usize) -> Self {
        let parent = Rc::new(self.clone());
        let scope = match kind {
            0 | 7 => Scope::Children(parent, None),
            1 | 8 => Scope::Attributes(parent, None),
            2 => if matches!(self.scope, Scope::Attributes(..)) {
                Scope::Attributes(parent_for_attributes(self), Some(name.into()))
            } else {
                let owner = if let Scope::Children(owner, None) = &self.scope { owner.clone() } else { parent };
                Scope::Children(owner, Some(name.into()))
            },
            6 => {
                let mut nodes = Vec::new();
                let mut next = self.first().map(|n| unsafe { ffi::child(n) }).unwrap_or(std::ptr::null_mut());
                while !next.is_null() { nodes.push(next); next = unsafe { ffi::next(next) }; }
                Scope::Nodes(Rc::new(nodes))
            }
            3 => Scope::Attributes(if matches!(self.scope, Scope::Attributes(..)) {
                parent_for_attributes(self)
            } else { parent }, Some(name.into())),
            _ => self.node_at(index).map(Scope::Node)
                .unwrap_or_else(|| Scope::Index(parent, index)),
        };
        let filter = if kind <= 1 { Namespace { value: ns.into(), prefix } } else { self.ns.clone() };
        let identity = scope_identity(&scope);
        Self { scope, identity, ns: filter, xpath_ns: Rc::new(RefCell::new(BTreeMap::new())), ..self.clone() }
    }
    /// Detaches a node without freeing memory still visible to another wrapper.
    fn detach(&self, node: Node) {
        if self.doc.detached.borrow().contains(&node) { return; }
        unsafe { ffi::unlink(node); }
        self.doc.detached.borrow_mut().push(node);
    }
}

/// Compares libxml's borrowed UTF-8 bytes without allocating per visited sibling.
unsafe fn borrowed_string_matches(raw: *const c_char, value: &str) -> bool {
    if raw.is_null() { value.is_empty() }
    else { CStr::from_ptr(raw).to_bytes() == value.as_bytes() }
}

/// Captures the native base node retained by PHP wrappers, separate from their live selection.
fn scope_identity(scope: &Scope) -> Node {
    match scope {
        Scope::Node(node) => *node,
        Scope::Children(parent, _) | Scope::Attributes(parent, _) | Scope::Index(parent, _) =>
            parent.first().unwrap_or(std::ptr::null_mut()),
        Scope::Nodes(nodes) | Scope::XPath(nodes) => nodes.first().copied().unwrap_or(std::ptr::null_mut()),
    }
}

/// Resolves the element whose attributes a named attribute wrapper must search.
fn parent_for_attributes(view: &View) -> Rc<View> {
    match &view.scope { Scope::Attributes(parent, _) => parent.clone(), _ => Rc::new(view.clone()) }
}

/// Copies a borrowed C string; null pointers represent an empty string.
unsafe fn string(raw: *const c_char) -> String {
    if raw.is_null() { String::new() } else { CStr::from_ptr(raw).to_string_lossy().into_owned() }
}
/// Copies and frees an xmlChar buffer allocated by the shim.
unsafe fn owned_string(raw: *mut c_char) -> String {
    let result = string(raw);
    if !raw.is_null() { ffi::string_free(raw.cast()); }
    result
}
/// Converts PHP's C-string input to a safe CString without embedded NUL bytes.
fn cstring(value: &str) -> CString { CString::new(value.split('\0').next().unwrap_or("")).unwrap_or_default() }

/// Parses exactly the supplied byte length and returns the document root wrapper.
#[cfg(all(test, elephc_xml_native))]
fn parse(data: &[u8], options: i64, namespace: &str, prefix: bool) -> Option<View> {
    parse_with_url(data, options, namespace, prefix, "")
}

/// Retains a file's base URI while libxml2 resolves relative external subsets and entities.
fn parse_with_url(data: &[u8], options: i64, namespace: &str, prefix: bool, url: &str) -> Option<View> {
    parse_with_diagnostics(data, options, namespace, prefix, url, 1)
}

/// Keeps DOM's structured diagnostics separate from SimpleXML's generic formatter lines.
fn parse_with_diagnostics(data: &[u8], options: i64, namespace: &str, prefix: bool, url: &str, style: i32) -> Option<View> {
    diagnostics::reset();
    crate::ffi::init_library();
    let length = i32::try_from(data.len()).ok()?;
    let url = cstring(url);
    let mut messages = std::ptr::null_mut();
    let raw = unsafe { ffi::parse(data.as_ptr().cast(), length, i32::try_from(options).ok()?, url.as_ptr(), style, &mut messages) };
    unsafe { diagnostics::capture(messages); }
    from_document(raw, namespace, prefix)
}

/// Preserves filename bytes and libxml's base URI and I/O diagnostics for file factories.
unsafe fn parse_file(filename: *const c_char, options: i64, namespace: &str, prefix: bool) -> Option<View> {
    diagnostics::reset();
    crate::ffi::init_library();
    if filename.is_null() { return None; }
    let mut messages = std::ptr::null_mut();
    let raw = ffi::parse_file(filename, i32::try_from(options).ok()?, &mut messages);
    diagnostics::capture(messages);
    from_document(raw, namespace, prefix)
}

/// Adopts a parsed document once and releases rootless results on the failure path.
fn from_document(raw: Node, namespace: &str, prefix: bool) -> Option<View> {
    if raw.is_null() { return None; }
    let root = unsafe { ffi::root(raw) };
    let doc = Rc::new(Document { raw, detached: RefCell::new(vec![]) });
    if root.is_null() { return None; }
    Some(View { doc, scope: Scope::Node(root), identity: root, ns: Namespace { value: namespace.into(), prefix },
        xpath_ns: Rc::new(RefCell::new(BTreeMap::new())) })
}

#[cfg(all(test, elephc_xml_native))]
mod tests;
