//! Purpose:
//! Copies root documents or detached non-root subtrees with independent node storage.
//!
//! Called from:
//! - The SimpleXML clone ABI.
//!
//! Key details:
//! - Detached nodes are copied through libxml2's opaque public API.
//! - Selector remapping uses opaque shim traversal and preserves namespace filters.

use super::*;

impl View {
    /// Copies a root document, or detaches only the non-root base node's copied subtree.
    pub(super) fn deep_clone(&self) -> Option<Self> {
        if !self.identity.is_null() && self.identity != unsafe { ffi::root(self.doc.raw) } {
            let copy = unsafe { ffi::copy_node(self.identity, self.doc.raw, 1) };
            if copy.is_null() { return None; }
            self.doc.detached.borrow_mut().push(copy);
            self.doc.detached_set.borrow_mut().insert(copy);
            return Some(self.remap_with(self.doc.clone(), &|node| {
                corresponding_node(self.identity, copy, node)
            }));
        }
        let raw = unsafe { ffi::copy(self.doc.raw) };
        if raw.is_null() { return None; }
        let doc = Rc::new(Document { raw, detached: RefCell::new(vec![]), detached_set: RefCell::new(HashSet::new()) });
        for node in self.doc.detached.borrow().iter() {
            let copy = unsafe { ffi::copy_node(*node, raw, 1) };
            if copy.is_null() { return None; }
            doc.detached.borrow_mut().push(copy);
            doc.detached_set.borrow_mut().insert(copy);
        }
        Some(self.remap(doc))
    }

    /// Preserves lazy selectors while binding concrete aliases to the copied tree or detached list.
    fn remap(&self, doc: Rc<Document>) -> Self {
        self.remap_with(doc.clone(), &|node| self.map_node(&doc, node))
    }

    /// Rebuilds selectors using the root-document or bounded-subtree correspondence map.
    fn remap_with(&self, doc: Rc<Document>, map: &impl Fn(Node) -> Node) -> Self {
        let scope = match &self.scope {
            Scope::Node(node) => Scope::Node(map(*node)),
            Scope::Nodes(nodes) => Scope::Nodes(Rc::new(nodes.iter().map(|node| map(*node)).filter(|node| !node.is_null()).collect())),
            Scope::XPath(nodes) => Scope::XPath(Rc::new(nodes.iter().map(|node| map(*node)).filter(|node| !node.is_null()).collect())),
            Scope::Children(parent, name) => Scope::Children(Rc::new(parent.remap_with(doc.clone(), map)), name.clone()),
            Scope::Attributes(parent, name) => Scope::Attributes(Rc::new(parent.remap_with(doc.clone(), map)), name.clone()),
            Scope::Index(parent, index) => Scope::Index(Rc::new(parent.remap_with(doc.clone(), map)), *index),
        };
        let identity = map(self.identity);
        Self { doc, scope, identity, ns: self.ns.clone(), xpath_ns: Rc::new(RefCell::new(self.xpath_ns.borrow().clone())) }
    }

    /// Finds one node in the live copy before searching independently copied detached subtrees.
    fn map_node(&self, doc: &Document, node: Node) -> Node {
        if node.is_null() { return node; }
        let mapped = unsafe { ffi::correspond(self.doc.raw, doc.raw, node) };
        if !mapped.is_null() { return mapped; }
        for (source, destination) in self.doc.detached.borrow().iter().zip(doc.detached.borrow().iter()) {
            let mapped = corresponding_node(*source, *destination, node);
            if !mapped.is_null() { return mapped; }
        }
        std::ptr::null_mut()
    }
}

/// Walks corresponding opaque node lists, including element attributes, without reading layouts.
fn corresponding_node(mut source: Node, mut destination: Node, node: Node) -> Node {
    unsafe {
        while !source.is_null() && !destination.is_null() {
            if source == node { return destination; }
            if ffi::node_type(source) == 1 {
                let mapped = corresponding_node(ffi::attrs(source), ffi::attrs(destination), node);
                if !mapped.is_null() { return mapped; }
            }
            let mapped = corresponding_node(ffi::child(source), ffi::child(destination), node);
            if !mapped.is_null() { return mapped; }
            source = ffi::next(source);
            destination = ffi::next(destination);
        }
    }
    std::ptr::null_mut()
}
