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

/// Namespace matching follows either a URI or a prefix, with no filter for an empty URI.
#[derive(Clone, Default)]
struct Namespace { value: String, prefix: bool }
impl Namespace {
    /// Tests a node's namespace using PHP's unqualified/default namespace rules.
    fn matches(&self, node: Node) -> bool {
        unsafe {
            if self.value.is_empty() {
                return string(ffi::ns(node, 1)).is_empty();
            }
            string(ffi::ns(node, i32::from(self.prefix))) == self.value
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
    Nodes(Vec<Node>),
}

/// PHP wrapper state; iterator position does not change through ordinary getters.
#[derive(Clone)]
struct View {
    doc: Rc<Document>, scope: Scope, ns: Namespace,
    xpath_ns: Rc<RefCell<BTreeMap<String, String>>>,
}
impl View {
    /// Resolves this selector against the current tree without caching removed children.
    fn nodes(&self) -> Vec<Node> {
        match &self.scope {
            Scope::Node(node) => if node.is_null() { vec![] } else { vec![*node] },
            Scope::Nodes(nodes) => nodes.clone(),
            Scope::Index(parent, index) => parent.nodes().get(*index).copied().into_iter().collect(),
            Scope::Children(parent, name) | Scope::Attributes(parent, name) => {
                let Some(node) = parent.first() else { return vec![]; };
                let attrs = matches!(self.scope, Scope::Attributes(..));
                let mut cursor = unsafe { if attrs { ffi::attrs(node) } else { ffi::child(node) } };
                let mut out = Vec::new();
                while !cursor.is_null() {
                    let kind = unsafe { ffi::node_type(cursor) };
                    if kind == (if attrs { 2 } else { 1 })
                        && self.ns.matches(cursor)
                        && name.as_ref().is_none_or(|name| *name == unsafe { string(ffi::name(cursor)) })
                    { out.push(cursor); }
                    cursor = unsafe { ffi::next(cursor) };
                }
                out
            }
        }
    }
    /// Returns the first matched node, if any.
    fn first(&self) -> Option<Node> { self.nodes().first().copied() }
    /// Returns nodes traversed by foreach and count for this wrapper kind.
    fn items(&self) -> Vec<Node> {
        if matches!(self.scope, Scope::Node(_) | Scope::Index(..)) {
            let view = self.select(0, "", "", false, 0);
            view.nodes()
        } else { self.nodes() }
    }
    /// Builds an independent wrapper while sharing the document and XPath registrations.
    fn with_scope(&self, scope: Scope) -> Self { Self { scope, ..self.clone() } }
    /// Builds children, attributes, named properties, or a numeric offset selection.
    fn select(&self, kind: i64, name: &str, ns: &str, prefix: bool, index: usize) -> Self {
        let parent = Rc::new(self.clone());
        let scope = match kind {
            0 => Scope::Children(parent, None),
            1 => Scope::Attributes(parent, None),
            2 => if matches!(self.scope, Scope::Attributes(..)) {
                Scope::Attributes(parent_for_attributes(self), Some(name.into()))
            } else { Scope::Children(parent, Some(name.into())) },
            3 => Scope::Attributes(if matches!(self.scope, Scope::Attributes(..)) {
                parent_for_attributes(self)
            } else { parent }, Some(name.into())),
            _ => Scope::Index(parent, index),
        };
        let filter = if kind <= 1 { Namespace { value: ns.into(), prefix } } else { self.ns.clone() };
        Self { scope, ns: filter, ..self.clone() }
    }
    /// Copies the whole document and rebuilds selectors against that independent tree.
    fn deep_clone(&self) -> Option<Self> {
        let raw = unsafe { ffi::copy(self.doc.raw) };
        if raw.is_null() { return None; }
        let doc = Rc::new(Document { raw, detached: RefCell::new(vec![]) });
        Some(self.remap(doc))
    }
    /// Preserves a lazy selector's shape and namespace filter across document cloning.
    fn remap(&self, doc: Rc<Document>) -> Self {
        let map = |node| unsafe { ffi::correspond(self.doc.raw, doc.raw, node) };
        let scope = match &self.scope {
            Scope::Node(node) => Scope::Node(map(*node)),
            Scope::Nodes(nodes) => Scope::Nodes(nodes.iter().map(|n| map(*n)).filter(|n| !n.is_null()).collect()),
            Scope::Children(parent, name) => Scope::Children(Rc::new(parent.remap(doc.clone())), name.clone()),
            Scope::Attributes(parent, name) => Scope::Attributes(Rc::new(parent.remap(doc.clone())), name.clone()),
            Scope::Index(parent, index) => Scope::Index(Rc::new(parent.remap(doc.clone())), *index),
        };
        Self { doc, scope, ns: self.ns.clone(), xpath_ns: Rc::new(RefCell::new(self.xpath_ns.borrow().clone())) }
    }
    /// Detaches a node without freeing memory still visible to another wrapper.
    fn detach(&self, node: Node) {
        unsafe { ffi::unlink(node); }
        self.doc.detached.borrow_mut().push(node);
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
fn parse(data: &[u8], options: i64, namespace: &str, prefix: bool) -> Option<View> {
    crate::ffi::init_library();
    let length = i32::try_from(data.len()).ok()?;
    let raw = unsafe { ffi::parse(data.as_ptr().cast(), length, i32::try_from(options).ok()?) };
    if raw.is_null() { return None; }
    let root = unsafe { ffi::root(raw) };
    let doc = Rc::new(Document { raw, detached: RefCell::new(vec![]) });
    if root.is_null() { return None; }
    Some(View { doc, scope: Scope::Node(root), ns: Namespace { value: namespace.into(), prefix },
        xpath_ns: Rc::new(RefCell::new(BTreeMap::new())) })
}
