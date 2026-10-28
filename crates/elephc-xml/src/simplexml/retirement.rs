//! Purpose:
//! Frees replaced native trees while retaining exactly the nodes exposed by live views.
//!
//! Called from:
//! - View construction, mutation, cursors, and document cloning.
//!
//! Key details:
//! - Documents hold weak indexes, so dead views and retired trees leave no growing lists.
//! - Retired subtrees retain their namespace owners until their final node pin drops.

use super::*;
use std::collections::HashSet;

/// Keeps one exposed pointer alive after its containing subtree leaves the document.
pub(super) struct NodePin {
    node: Node,
    doc: Weak<Document>,
    retired: RefCell<Option<Rc<RetiredSubtree>>>,
}
impl Drop for NodePin {
    /// Removes dead pointer bookkeeping before releasing its possible native tree owner.
    fn drop(&mut self) {
        if let Some(doc) = self.doc.upgrade() { doc.pins.borrow_mut().remove(&self.node); }
    }
}

/// Owns one removed tree and any older trees providing its inherited namespace storage.
pub(super) struct RetiredSubtree {
    pub(super) node: Node,
    doc: Rc<Document>,
    _clone_owner: Option<Rc<CloneOwner>>,
    _parents: Vec<Rc<RetiredSubtree>>,
}
impl Drop for RetiredSubtree {
    /// Frees children before their document, clone, and ancestor namespace owners can die.
    fn drop(&mut self) {
        self.doc.detached.borrow_mut().remove(&self.node);
        unsafe { ffi::node_free(self.node); }
    }
}

impl Document {
    /// Adopts an attached document with empty weak indexes and no independent tree owners.
    pub(super) fn new(raw: Node) -> Rc<Self> {
        Rc::new(Self { raw, detached: RefCell::new(HashMap::new()), pins: RefCell::new(HashMap::new()) })
    }

    /// Shares an exposed pointer and discovers an already retired ancestor when necessary.
    pub(super) fn pin(self: &Rc<Self>, node: Node) -> Rc<NodePin> {
        if let Some(pin) = self.pins.borrow().get(&node).and_then(Weak::upgrade) { return pin; }
        let mut ancestor = node;
        let mut retired = None;
        while !ancestor.is_null() {
            if let Some(owner) = self.detached.borrow().get(&ancestor).and_then(Weak::upgrade) {
                retired = Some(owner);
                break;
            }
            ancestor = unsafe { ffi::parent(ancestor) };
        }
        let pin = Rc::new(NodePin { node, doc: Rc::downgrade(self), retired: RefCell::new(retired) });
        self.pins.borrow_mut().insert(node, Rc::downgrade(&pin));
        pin
    }

    /// Publishes a removed tree while keeping its ownership separate from the document index.
    pub(super) fn adopt_retired(self: &Rc<Self>, node: Node, clone_owner: Option<Rc<CloneOwner>>,
        parents: Vec<Rc<RetiredSubtree>>) -> Rc<RetiredSubtree> {
        if let Some(owner) = &clone_owner {
            if owner.node == node { owner.transferred.set(true); }
        }
        let owner = Rc::new(RetiredSubtree { node, doc: self.clone(), _clone_owner: clone_owner, _parents: parents });
        self.detached.borrow_mut().insert(node, Rc::downgrade(&owner));
        owner
    }

    /// Unlinks once, freeing unobserved subtrees immediately and retaining observed storage.
    pub(super) fn retire(self: &Rc<Self>, node: Node, clone_owner: Option<Rc<CloneOwner>>) {
        if self.detached.borrow().get(&node).and_then(Weak::upgrade).is_some() { return; }
        unsafe { ffi::unlink(node); }
        let mut exposed = Vec::new();
        let mut pending = vec![node];
        while let Some(current) = pending.pop() {
            if let Some(pin) = self.pins.borrow().get(&current).and_then(Weak::upgrade) { exposed.push(pin); }
            unsafe {
                let kind = ffi::node_type(current);
                if kind == 1 {
                    let mut attribute = ffi::attrs(current);
                    while !attribute.is_null() { pending.push(attribute); attribute = ffi::next(attribute); }
                }
                // Entity-reference children point into the document's shared declarations.
                if kind == 5 || kind == 17 { continue; }
                let mut child = ffi::child(current);
                while !child.is_null() { pending.push(child); child = ffi::next(child); }
            }
        }
        if exposed.is_empty() { unsafe { ffi::node_free(node); } return; }
        let mut parents = Vec::new();
        let mut seen = HashSet::new();
        for pin in &exposed {
            if let Some(parent) = pin.retired.borrow().as_ref() {
                if seen.insert(Rc::as_ptr(parent)) { parents.push(parent.clone()); }
            }
        }
        let owner = self.adopt_retired(node, clone_owner, parents);
        for pin in exposed { *pin.retired.borrow_mut() = Some(owner.clone()); }
    }

    /// Takes a strong snapshot while cloning the currently observed detached trees.
    pub(super) fn retired_snapshot(&self) -> Vec<Rc<RetiredSubtree>> {
        self.detached.borrow().values().filter_map(Weak::upgrade).collect()
    }
}
