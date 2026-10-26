//! Purpose:
//! Advances SimpleXML iterators without repeatedly materializing sibling selections.
//!
//! Called from:
//! - The per-handle iterator ABI in `simplexml::abi`.
//!
//! Key details:
//! - Live selections read the current node's next link only when advancing.
//! - Explicit node lists share immutable storage and use direct indexing.
//! - The registry view owns every cursor node, including detached document nodes.

use super::*;

/// One initialized iterator, stored separately from cloneable document views.
pub(super) enum Cursor {
    Siblings { current: Node, kind: i32, namespace: Namespace, name: Option<String> },
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
        match selected.scope {
            Scope::Nodes(nodes) | Scope::XPath(nodes) => Self::Nodes { nodes, position: 0 },
            Scope::Children(parent, name) | Scope::Attributes(parent, name) => {
                let Some(parent) = parent.first() else { return Self::Empty; };
                if attrs && unsafe { ffi::node_type(parent) } != 1 { return Self::Empty; }
                let first = unsafe { if attrs { ffi::attrs(parent) } else { ffi::child(parent) } };
                let kind = if attrs { 2 } else { 1 };
                let current = matching_sibling(first, kind, &selected.ns, name.as_deref());
                Self::Siblings { current, kind, namespace: selected.ns, name }
            }
            Scope::Node(_) | Scope::Index(..) => Self::Empty,
        }
    }

    /// Returns the current node without rescanning its siblings.
    pub(super) fn current(&self) -> Option<Node> {
        match self {
            Self::Siblings { current, .. } => (!current.is_null()).then_some(*current),
            Self::Nodes { nodes, position } => nodes.get(*position).copied(),
            Self::Empty => None,
        }
    }

    /// Observes live sibling links after user mutations, including deletion of the current node.
    pub(super) fn advance(&mut self) {
        match self {
            Self::Siblings { current, kind, namespace, name } => {
                if !current.is_null() {
                    *current = matching_sibling(unsafe { ffi::next(*current) }, *kind, namespace, name.as_deref());
                }
            }
            Self::Nodes { position, .. } => *position = position.saturating_add(1),
            Self::Empty => (),
        }
    }
}

/// Finds the next selected sibling without allocating strings or a list of later matches.
fn matching_sibling(mut node: Node, kind: i32, namespace: &Namespace, name: Option<&str>) -> Node {
    while !node.is_null() {
        if node_matches(node, kind, namespace, name) { return node; }
        node = unsafe { ffi::next(node) };
    }
    node
}

/// Shares one selection predicate between live cursors, indexed access, and full enumeration.
pub(super) fn node_matches(node: Node, kind: i32, namespace: &Namespace, name: Option<&str>) -> bool {
    (unsafe { ffi::node_type(node) }) == kind && namespace.matches(node)
        && name.is_none_or(|name| unsafe { borrowed_string_matches(ffi::name(node), name) })
}
