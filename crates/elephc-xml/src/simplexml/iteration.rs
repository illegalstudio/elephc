//! Purpose:
//! Advances SimpleXML iterators without repeatedly materializing sibling selections.
//!
//! Called from:
//! - The per-handle iterator ABI in `simplexml::abi`.
//!
//! Key details:
//! - Live selections read the current node's next link only when advancing.
//! - Explicit node lists share immutable storage and use direct indexing.
//! - Each cursor owns its selector and current node, including retired document storage.

use super::*;

/// One initialized iterator owns its selector and pins nodes not held by a raw list.
pub(super) struct Cursor { owner: View, state: CursorState }

/// Explicit lists are pinned by the selector; live sibling iteration pins its current node.
enum CursorState {
    Siblings { current: Node, pin: Option<Rc<NodePin>>, kind: i32, namespace: Namespace, name: Option<Vec<u8>> },
    Nodes { nodes: Rc<Vec<Node>>, position: usize },
    Empty,
}

impl Cursor {
    /// Starts from the current selection, applying Node/Index child iteration once.
    pub(super) fn new(view: &View) -> Self {
        let selected = if matches!(view.scope, Scope::Node(_) | Scope::Index(..)) {
            view.select(7, "", "", false, 0)
        } else { view.clone() };
        let attrs = matches!(selected.scope, Scope::Attributes(..));
        let state = match &selected.scope {
            Scope::Nodes(nodes) | Scope::XPath(nodes) => CursorState::Nodes { nodes: nodes.clone(), position: 0 },
            Scope::Children(parent, name) | Scope::Attributes(parent, name) => {
                match parent.first().filter(|node| !attrs || unsafe { ffi::node_type(*node) } == 1) {
                    None => CursorState::Empty,
                    Some(parent) => {
                        let first = unsafe { if attrs { ffi::attrs(parent) } else { ffi::child(parent) } };
                        let kind = if attrs { 2 } else { 1 };
                        let current = matching_sibling(first, kind, &selected.ns, name.as_deref());
                        let pin = (!current.is_null()).then(|| selected.doc.pin(current));
                        CursorState::Siblings { current, pin, kind, namespace: selected.ns.clone(), name: name.clone() }
                    }
                }
            }
            Scope::Node(_) | Scope::Index(..) => CursorState::Empty,
        };
        Self { owner: selected, state }
    }

    /// Returns the current node without rescanning its siblings.
    pub(super) fn current(&self) -> Option<Node> {
        match &self.state {
            CursorState::Siblings { current, .. } => (!current.is_null()).then_some(*current),
            CursorState::Nodes { nodes, position } => nodes.get(*position).copied(),
            CursorState::Empty => None,
        }
    }

    /// Acquires the next node's pin before the old retired tree can be released.
    pub(super) fn advance(&mut self) {
        match &mut self.state {
            CursorState::Siblings { current, pin, kind, namespace, name } => {
                if !current.is_null() {
                    let next = matching_sibling(unsafe { ffi::next(*current) }, *kind, namespace, name.as_deref());
                    let next_pin = (!next.is_null()).then(|| self.owner.doc.pin(next));
                    *current = next;
                    *pin = next_pin;
                }
            }
            CursorState::Nodes { position, .. } => *position = position.saturating_add(1),
            CursorState::Empty => (),
        }
    }
}

/// Finds the next selected sibling without allocating strings or a list of later matches.
fn matching_sibling(mut node: Node, kind: i32, namespace: &Namespace, name: Option<&[u8]>) -> Node {
    while !node.is_null() {
        if node_matches(node, kind, namespace, name) { return node; }
        node = unsafe { ffi::next(node) };
    }
    node
}

/// Shares one selection predicate between live cursors, indexed access, and full enumeration.
pub(super) fn node_matches(node: Node, kind: i32, namespace: &Namespace, name: Option<&[u8]>) -> bool {
    (unsafe { ffi::node_type(node) }) == kind && namespace.matches(node)
        && name.is_none_or(|name| unsafe { borrowed_string_matches(ffi::name(node), name) })
}
