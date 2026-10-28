//! Purpose:
//! Live SimpleXML selections over libxml2 documents, shared by AOT and eval.
//!
//! Called from:
//! - `abi` exports used by the compiler's SimpleXML prelude.
//!
//! Key details:
//! - Every view owns its document through Rc. Selectors are evaluated on access.
//! - Unlinked subtrees live until their last exposed node is released.
//! - Independent clones release with their views; removal transfers storage to live node pins.
//! - State stays on its originating thread; no native pointer crosses threads.

mod abi;
mod cloning;
mod diagnostics;
mod ffi;
mod iteration;
mod mutation;
mod query;
mod retirement;
use retirement::{NodePin, RetiredSubtree};

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::ffi::{c_char, CStr, CString};
use std::rc::{Rc, Weak};
use ffi::Node;

/// Owns a libxml2 document; weak indexes never keep retired native storage alive.
struct Document {
    raw: Node,
    detached: RefCell<HashMap<Node, Weak<RetiredSubtree>>>,
    pins: RefCell<HashMap<Node, Weak<NodePin>>>,
}
impl Drop for Document {
    /// Releases the attached document after all independently retired owners have died.
    fn drop(&mut self) { unsafe { ffi::doc_free(self.raw); } }
}

/// Owns one parentless clone until exposed pins take responsibility for its storage.
struct CloneOwner { node: Node, _doc: Rc<Document>, transferred: Cell<bool> }
impl Drop for CloneOwner {
    /// A transferred owner can outlive the native node without dereferencing its old pointer.
    fn drop(&mut self) {
        if !self.transferred.get() && unsafe { ffi::parent(self.node).is_null() } {
            unsafe { ffi::node_free(self.node); }
        }
    }
}

/// An empty selector matches unprefixed nodes, including nodes in a default namespace.
#[derive(Clone, Default)]
struct Namespace { value: Vec<u8>, prefix: bool }
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
    Children(Rc<View>, Option<Vec<u8>>),
    Attributes(Rc<View>, Option<Vec<u8>>),
    Index(Rc<View>, usize),
    Nodes(Rc<Vec<Node>>),
    XPath(Rc<Vec<Node>>),
}

/// PHP wrapper state; iterator position does not change through ordinary getters.
#[derive(Clone)]
struct View {
    _clone_owner: Option<Rc<CloneOwner>>,
    doc: Rc<Document>, scope: Scope, ns: Namespace,
    identity: Node,
    _pins: Rc<Vec<Rc<NodePin>>>,
    xpath_ns: Rc<RefCell<BTreeMap<Vec<u8>, Vec<u8>>>>,
}
impl View {
    /// Constructs a view with explicit pins for its stable identity and raw node lists.
    fn new(doc: Rc<Document>, scope: Scope, identity: Node, ns: Namespace,
        clone_owner: Option<Rc<CloneOwner>>, xpath_ns: Rc<RefCell<BTreeMap<Vec<u8>, Vec<u8>>>>) -> Self {
        let mut view = Self { _clone_owner: clone_owner, doc, scope, identity, _pins: Rc::new(vec![]), xpath_ns, ns };
        view.refresh_pins();
        view
    }
    /// Acquires replacement pins before old selectors or retired owners can be released.
    fn refresh_pins(&mut self) {
        let mut nodes = vec![self.identity];
        match &self.scope {
            Scope::Node(node) => nodes.push(*node),
            Scope::Nodes(list) | Scope::XPath(list) => nodes.extend(list.iter().copied()),
            _ => (),
        }
        nodes.retain(|node| !node.is_null());
        nodes.sort_unstable(); nodes.dedup();
        self._pins = Rc::new(nodes.into_iter().map(|node| self.doc.pin(node)).collect());
    }
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
                    if iteration::node_matches(cursor, if attrs { 2 } else { 1 }, &self.ns, name.as_deref())
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
                    if iteration::node_matches(node, if attrs { 2 } else { 1 }, &self.ns, name.as_deref())
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
    /// Counts stable explicit lists without copying them into a temporary vector.
    fn item_count(&self) -> usize {
        match &self.scope {
            Scope::Nodes(nodes) | Scope::XPath(nodes) => nodes.len(),
            _ => self.items().len(),
        }
    }
    /// Materializes an XPath attribute as a filtered attribute list, preserving ordinary offsets.
    fn item(&self, index: usize) -> Option<Self> {
        let node = if matches!(self.scope, Scope::Node(_) | Scope::Index(..)) {
            self.select(7, "", "", false, 0).node_at(index)?
        } else { self.node_at(index)? };
        Some(self.item_view(node))
    }
    /// Wraps the current cursor node with the same XPath attribute semantics as indexed access.
    fn item_view(&self, node: Node) -> Self {
        if matches!(self.scope, Scope::XPath(_)) && unsafe { ffi::node_type(node) } == 2 {
            let parent = self.with_scope(Scope::Node(unsafe { ffi::parent(node) }));
            let name = unsafe { bytes(ffi::name(node)) };
            let mut selected = self.with_scope(Scope::Attributes(Rc::new(parent), Some(name)));
            selected.ns = Namespace { value: unsafe { bytes(ffi::ns(node, 0)) }, prefix: false };
            return selected;
        }
        let mut selected = self.with_scope(Scope::Node(node));
        if matches!(self.scope, Scope::XPath(_)) { selected.ns = Namespace::default(); }
        selected
    }
    /// Builds a new wrapper with its own empty XPath context, sharing only native storage.
    fn with_scope(&self, scope: Scope) -> Self {
        let identity = scope_identity(&scope);
        let mut selected = Self { scope, identity, xpath_ns: Rc::new(RefCell::new(BTreeMap::new())), ..self.clone() };
        selected.refresh_pins();
        selected
    }
    /// Builds children, attributes, named properties, or a numeric offset selection.
    fn select(&self, kind: i64, name: impl AsRef<[u8]>, ns: impl AsRef<[u8]>, prefix: bool, index: usize) -> Self {
        let name = name.as_ref();
        let ns = ns.as_ref();
        // The child group stays live within its original element. Resolve an
        // existing parent now so removing its ancestor cannot rebind this group.
        // Missing ancestors retain their lazy path for chained writes.
        let parent = Rc::new(self.first().map(|node| self.with_scope(Scope::Node(node)))
            .unwrap_or_else(|| self.clone()));
        let scope = match kind {
            12..=14 => Scope::Node(self.first().map(|node| unsafe {
                match kind { 12 => ffi::parent(node), 13 => ffi::previous(node), _ => ffi::next(node) }
            }).unwrap_or(std::ptr::null_mut())),
            15 => {
                let mut nodes = Vec::new();
                let mut node = self.first().filter(|node| unsafe { ffi::node_type(*node) == 1 })
                    .map(|node| unsafe { ffi::attrs(node) }).unwrap_or(std::ptr::null_mut());
                while !node.is_null() { nodes.push(node); node = unsafe { ffi::next(node) }; }
                Scope::Nodes(Rc::new(nodes))
            }
            0 | 7 => Scope::Children(parent, None),
            1 | 8 => Scope::Attributes(parent, None),
            2 => if matches!(self.scope, Scope::Attributes(..)) {
                Scope::Attributes(parent_for_attributes(self), Some(name.into()))
            } else {
                let owner = if let Scope::Children(owner, None) = &self.scope { owner.clone() } else { parent };
                Scope::Children(owner, Some(name.into()))
            },
            6 | 9 | 10 | 11 => {
                let mut nodes = Vec::new();
                let mut next = if kind == 11 { unsafe { ffi::child(self.doc.raw) } } else { self.first().map(|n| if kind == 10 { n } else { unsafe { ffi::child(n) } })
                    .unwrap_or(std::ptr::null_mut())
                };
                while !next.is_null() {
                    let node_type = unsafe { ffi::node_type(next) };
                    if kind == 6 || kind == 11 || (!matches!(node_type, 3 | 4) && (node_type != 1 || self.ns.matches(next))) {
                        nodes.push(next);
                    }
                    if kind != 6 && kind != 11 && node_type == 17 { break; }
                    next = unsafe { ffi::next(next) };
                }
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
        let mut selected = Self { scope, identity, ns: filter, xpath_ns: Rc::new(RefCell::new(BTreeMap::new())), ..self.clone() };
        selected.refresh_pins();
        selected
    }
    /// Detaches a node without freeing memory still visible to another wrapper.
    fn detach(&self, node: Node) {
        self.doc.retire(node, self._clone_owner.clone());
    }
}

/// Compares libxml's borrowed UTF-8 bytes without allocating per visited sibling.
unsafe fn borrowed_string_matches(raw: *const c_char, value: impl AsRef<[u8]>) -> bool {
    let value = value.as_ref();
    if raw.is_null() { value.is_empty() }
    else { CStr::from_ptr(raw).to_bytes() == value }
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
unsafe fn bytes(raw: *const c_char) -> Vec<u8> {
    if raw.is_null() { Vec::new() } else { CStr::from_ptr(raw).to_bytes().to_vec() }
}
/// Copies and frees an xmlChar buffer allocated by the shim.
unsafe fn owned_bytes(raw: *mut c_char) -> Vec<u8> {
    let result = bytes(raw);
    if !raw.is_null() { ffi::string_free(raw.cast()); }
    result
}
/// Decodes known UTF-8 native test fixtures without allowing lossy production conversions.
#[cfg(all(test, elephc_xml_native))]
unsafe fn string(raw: *const c_char) -> String { String::from_utf8(bytes(raw)).expect("UTF-8 test fixture") }
/// Releases an owned libxml buffer after decoding a known UTF-8 test fixture.
#[cfg(all(test, elephc_xml_native))]
unsafe fn owned_string(raw: *mut c_char) -> String { String::from_utf8(owned_bytes(raw)).expect("UTF-8 test fixture") }

/// Converts PHP's C-string input to a safe CString without embedded NUL bytes.
fn cstring(value: impl AsRef<[u8]>) -> CString {
    CString::new(value.as_ref().split(|byte| *byte == 0).next().unwrap_or(b"")).unwrap_or_default()
}

/// Parses exactly the supplied byte length and returns the document root wrapper.
#[cfg(all(test, elephc_xml_native))]
fn parse(data: &[u8], options: i64, namespace: impl AsRef<[u8]>, prefix: bool) -> Option<View> {
    parse_with_url(data, options, namespace, prefix, "")
}

/// Retains a file's base URI while libxml2 resolves relative external subsets and entities.
fn parse_with_url(data: &[u8], options: i64, namespace: impl AsRef<[u8]>, prefix: bool, url: impl AsRef<[u8]>) -> Option<View> {
    parse_with_diagnostics(data, options, namespace, prefix, url, 1)
}

/// Keeps DOM's structured diagnostics separate from SimpleXML's generic formatter lines.
fn parse_with_diagnostics(data: &[u8], options: i64, namespace: impl AsRef<[u8]>, prefix: bool, url: impl AsRef<[u8]>, style: i32) -> Option<View> {
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
unsafe fn parse_file(filename: *const c_char, options: i64, namespace: impl AsRef<[u8]>, prefix: bool) -> Option<View> {
    diagnostics::reset();
    crate::ffi::init_library();
    if filename.is_null() { return None; }
    let mut messages = std::ptr::null_mut();
    let raw = ffi::parse_file(filename, i32::try_from(options).ok()?, &mut messages);
    diagnostics::capture(messages);
    from_document(raw, namespace, prefix)
}

/// Adopts a parsed document once and releases rootless results on the failure path.
fn from_document(raw: Node, namespace: impl AsRef<[u8]>, prefix: bool) -> Option<View> {
    if raw.is_null() { return None; }
    let root = unsafe { ffi::root(raw) };
    let doc = Document::new(raw);
    if root.is_null() { return None; }
    Some(View::new(doc, Scope::Node(root), root, Namespace { value: namespace.as_ref().to_vec(), prefix },
        None, Rc::new(RefCell::new(BTreeMap::new()))))
}

#[cfg(all(test, elephc_xml_native))]
mod tests;

#[cfg(all(test, elephc_xml_native))]
mod tests_html_import;

#[cfg(all(test, elephc_xml_native))]
mod tests_retirement;

#[cfg(all(test, elephc_xml_native))]
mod tests_bytes;

#[cfg(all(test, elephc_xml_native))]
mod tests_dom_encoding;

#[cfg(all(test, elephc_xml_native))]
mod tests_dom_attributes;

#[cfg(all(test, elephc_xml_native))]
mod tests_add_and_text;

#[cfg(all(test, elephc_xml_native))]
mod tests_xpath_context;

#[cfg(all(test, elephc_xml_native))]
mod tests_dom_views;
